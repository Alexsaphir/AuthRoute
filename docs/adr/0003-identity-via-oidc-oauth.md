# ADR-0003 — Identity via OIDC / OAuth

- **Status:** Proposed
- **Date:** 2026-06-13
- **Supersedes:** —
- **Companion:** [ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md), [ADR-0004](0004-envoy-gateway-integration-mechanism.md)
- **Scope:** Identity model — `v1alpha1`

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

_To be decided._

## Consequences

_To be completed once the decision is made._
