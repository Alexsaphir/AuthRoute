# ADR-0003 — Identity via OIDC / OAuth

- **Status:** Accepted
- **Date:** 2026-06-13
- **Supersedes:** —
- **Companion:** [ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md), [ADR-0004](0004-envoy-gateway-integration-mechanism.md)
- **Scope:** Identity model — `v1alpha1`
- **Informed by:** [research/authelia.md](../research/authelia.md), [research/envoy-gateway.md](../research/envoy-gateway.md)

## Context

[ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md) replaces Authelia's
LLDAP backend with an external **OIDC/OAuth** provider as the source of identity.
This ADR must decide how identity is configured and consumed:

- How is the OIDC provider configured in-cluster — a dedicated CRD, a referenced
  Secret for `clientID`/`clientSecret`, the issuer URL, scopes, redirect/logout
  paths? Can multiple providers coexist (per-namespace, per-route)?
- **Claim-to-principal mapping.** The route policy in
  [ADR-0002](0002-per-route-authorization-crd.md) names *groups* and *users*.
  Which OIDC claims yield those (e.g. `groups`, `email`, `sub`), and how is that
  mapping expressed and validated?
- Where does the OIDC *login* flow happen — owned by AuthRoute, or delegated to
  Envoy Gateway's built-in OIDC `SecurityPolicy`? (This is entangled with
  [ADR-0004](0004-envoy-gateway-integration-mechanism.md); decide them together.)
- Session/token handling: cookies vs. bearer tokens, lifetime, refresh, and what
  identity is forwarded to the authorization decision and to upstreams
  (`headersToBackend`).

## Decision

**AuthRoute owns the OIDC/OAuth flow itself** and manages a **domain-wide session
cookie** for single sign-on across subdomains — the Authelia model, *not* Envoy
Gateway's built-in per-route OIDC.

1. **AuthRoute is the OIDC Relying Party.** It runs the Authorization Code flow
   (with PKCE) against an external OIDC/OAuth provider. We do **not** use Envoy
   Gateway's `SecurityPolicy.oidc`: that is scoped per-`SecurityPolicy`/per-route
   and cannot provide one shared session across many subdomains (see
   [research/envoy-gateway.md](../research/envoy-gateway.md)).
2. **AuthRoute hosts a user-facing auth portal** on a dedicated subdomain (e.g.
   `auth.example.com`): login initiation, the provider callback, and logout.
3. **Single sign-on via a parent-domain cookie.** On successful login AuthRoute
   issues its own session, carried in a cookie scoped to the parent domain (e.g.
   `Domain=example.com`) so one sign-in is valid across all subdomains. Multiple
   cookie domains may be configured for distinct protected domains.
4. **Identity model.** A session resolves to a `Subject { username, groups[],
   name, email }` (mirroring Authelia's `Subject`). These are derived from OIDC
   ID-token / `userinfo` claims via a configurable claim→field mapping (e.g.
   `groups` claim → `groups`). `groups` and `username` are what per-route policies
   ([ADR-0002](0002-per-route-authorization-crd.md)) match on.
5. **Downstream identity headers.** On an allow decision, identity is forwarded to
   upstreams as `Remote-User`, `Remote-Groups`, `Remote-Name`, `Remote-Email`
   (Authelia-compatible), surfaced through Envoy's `extAuth.headersToBackend`
   ([ADR-0004](0004-envoy-gateway-integration-mechanism.md) §`headersToBackend`).

## Consequences

- AuthRoute is **user-facing and on the request path** (the portal plus the
  decision endpoint of ADR-0004 are two surfaces of one service); availability and
  latency become first-class concerns.
- AuthRoute owns **session/cookie security**: signing/encryption of the cookie,
  CSRF/state protection on the callback, `Secure` / `HttpOnly` / `SameSite`,
  expiry and rotation. Getting these right is squarely our responsibility now.
- SSO across subdomains is gained; **single logout** is harder — clearing the
  AuthRoute cookie is straightforward, but propagating logout to the OIDC provider
  requires the optional end-session endpoint.
- **Open follow-up:** session storage strategy — stateless signed/encrypted cookie
  vs. a server-side session store (and what backs it). Deferred to its own ADR.

