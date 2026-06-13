# ADR-0004 — Envoy Gateway integration mechanism

- **Status:** Accepted
- **Date:** 2026-06-13
- **Supersedes:** —
- **Companion:** [ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md), [ADR-0002](0002-per-route-authorization-crd.md), [ADR-0003](0003-identity-via-oidc-oauth.md)
- **Scope:** Data-plane integration — `v1alpha1`
- **Informed by:** [research/envoy-gateway.md](../research/envoy-gateway.md), [research/authelia.md](../research/authelia.md)

## Context

[ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md) couples AuthRoute
to **Envoy Gateway**. Envoy Gateway's `SecurityPolicy` CRD already attaches to a
`Gateway`/`HTTPRoute` via `targetRefs` and can configure **both** OIDC login and
**external authorization** (`extAuth.grpc` / `extAuth.http` with `backendRefs`,
`headersToBackend`, 403-on-deny). This ADR must decide exactly how AuthRoute plugs
in, given that overlap:

- **Division of labor for login.** Does AuthRoute *own* the OIDC flow
  (Authelia-style portal/redirect), or delegate login to Envoy Gateway's built-in
  OIDC `SecurityPolicy` and act *purely* as the ext_authz decision service that
  evaluates group/user rules? (Coordinate with [ADR-0003](0003-identity-via-oidc-oauth.md).)
- **ext_authz transport.** `extAuth.grpc` vs. `extAuth.http` for the
  authorization callout — protocol, request/response contract, headers consumed
  and emitted, performance and streaming considerations.
- **Who writes `SecurityPolicy`.** Does AuthRoute *generate and reconcile*
  `SecurityPolicy` resources from the per-route CRD in
  [ADR-0002](0002-per-route-authorization-crd.md) (so users only author
  AuthRoute's CRD), or is AuthRoute *referenced by* user-authored
  `SecurityPolicy` resources as the ext_authz backend?
- Failure mode: fail-open vs. fail-closed when AuthRoute is unavailable; how
  denials and errors surface to the client.
- Coupling/version assumptions on Envoy Gateway and the Gateway API.

## Decision

**AuthRoute runs as an HTTP external-authorization (forward-auth) service** — the
`extAuth.http` backend of an Envoy Gateway `SecurityPolicy`. It makes the
allow/deny decision **per request**. It does *not* delegate login to Envoy's
built-in OIDC and does *not* compile policy into Envoy's static `authorization`
rules engine.

1. **Forward-auth via `extAuth.http`.** A `SecurityPolicy` targeting the route
   references AuthRoute's Kubernetes Service as the `extAuth.http` backend. For
   each request Envoy forwards request context (`X-Forwarded-Host` /
   `-Method` / `-Uri` / `-Proto`, cookies) to AuthRoute's `/authz` endpoint.
   HTTP (not `grpc`) is chosen for the forward-auth style and for parity with
   Authelia's `handler_authz_impl_extauthz.go` reference
   ([research/authelia.md](../research/authelia.md)).
2. **Decision responses:**
   - **Allow** → `200` plus the `Remote-*` identity headers, propagated upstream
     via `extAuth.headersToBackend` ([ADR-0003](0003-identity-via-oidc-oauth.md) §5).
   - **Unauthenticated** (browser, no valid session cookie) → redirect to the
     auth portal (`auth.example.com`, ADR-0003) with a return-URL, where the OIDC
     flow runs and the domain-wide SSO cookie is set.
   - **Authenticated but not permitted** → `403`.
3. **Fail closed.** `extAuth.failOpen = false` (the default). Because this is a
   security control, an unavailable AuthRoute denies rather than admits traffic.
4. **Why not the alternatives:** Envoy's built-in `oidc` is per-route and gives no
   cross-subdomain SSO (ADR-0003); its `authorization` engine only matches static
   JWT claims/CIDRs, not AuthRoute's session-derived, dynamically-resolved
   `Subject` (see [research/envoy-gateway.md](../research/envoy-gateway.md)).

## Consequences

- AuthRoute is **on the request hot path**: per-request latency and availability
  become SLOs. Session validation should be cheap/cacheable to keep added latency
  low.
- **Fail-closed means an AuthRoute outage blocks every protected app**, so it must
  be run highly available. This is an accepted, deliberate trade-off.
- We reuse Envoy Gateway's mature `extAuth` plumbing instead of reimplementing a
  data plane — at the cost of binding tightly to Envoy Gateway / Gateway API.
- **Open (decided in [ADR-0002](0002-per-route-authorization-crd.md)):** whether
  AuthRoute *reconciles* the `SecurityPolicy` that wires `extAuth` to each route
  from its per-route CRD, or whether users author the `SecurityPolicy` and merely
  reference AuthRoute's Service.

