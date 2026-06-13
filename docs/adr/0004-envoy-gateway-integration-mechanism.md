# ADR-0004 — Envoy Gateway integration mechanism

- **Status:** Proposed
- **Date:** 2026-06-13
- **Supersedes:** —
- **Companion:** [ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md), [ADR-0002](0002-per-route-authorization-crd.md), [ADR-0003](0003-identity-via-oidc-oauth.md)
- **Scope:** Data-plane integration — `v1alpha1`

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

_To be decided._

## Consequences

_To be completed once the decision is made._
