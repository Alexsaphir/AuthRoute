# ADR-0001 — AuthRoute: a Kubernetes-native auth gateway on Envoy Gateway

- **Status:** Accepted
- **Date:** 2026-06-13
- **Supersedes:** —
- **Companion:** [ADR-0002](0002-per-route-authorization-crd.md), [ADR-0003](0003-identity-via-oidc-oauth.md), [ADR-0004](0004-envoy-gateway-integration-mechanism.md)
- **Scope:** Product vision, high-level topology, language/runtime — `v1alpha1`
- **Informed by:** [docs/research/](../research/)

## Context

[Authelia](https://github.com/authelia/authelia) is a popular self-hosted
authentication and authorization portal for protecting web applications behind a
reverse proxy. Two things make it awkward in a Kubernetes-native, GitOps world:

1. **Static configuration.** Access control rules live in a central
   configuration file. Adding or changing protection for a route means editing
   and redeploying that central config, rather than declaring intent next to the
   workload it protects.
2. **Identity coupling.** Authelia commonly pairs with LLDAP as the user/group
   backend. We would rather treat an external **OIDC/OAuth** provider as the
   identity source and not run a directory service.

Meanwhile, the ecosystem has converged on the **Gateway API** and, concretely,
**Envoy Gateway** as a data plane. Envoy Gateway already ships a `SecurityPolicy`
CRD that attaches to a `Gateway` or `HTTPRoute` via `targetRefs` and can wire up
both OIDC login and **external authorization** (`extAuth.grpc` / `extAuth.http`),
denying with 403 and propagating identity headers downstream via
`headersToBackend`. (Sources: Envoy Gateway docs — ext-auth and OIDC tasks.)

We want an Authelia alternative that is **dynamically configured from Kubernetes
resources** and **tightly coupled to Envoy Gateway**: each protected route
carries its own custom resource declaring whether it needs auth and which
groups/users are allowed, reconciled by an operator — no central config file.

## Decision

We will build **AuthRoute** as a **Rust `kube-rs` Kubernetes operator** that
provides dynamic, per-route authentication and authorization for traffic served
by **Envoy Gateway**, with identity sourced from an external **OIDC/OAuth**
provider (no LLDAP).

High-level topology:

- **Per-route policy as a custom resource.** Authorization intent (needs-auth?
  which groups/users?) is expressed as a CRD attached to a route, living
  alongside the workload — not in a central file. *(Shape decided in ADR-0002.)*
- **OIDC/OAuth for identity.** Users and groups come from an external OIDC
  provider; claims map to the groups/users referenced by route policies.
  *(Decided in ADR-0003.)*
- **Coupled to Envoy Gateway through forward-auth only.** AuthRoute's *sole*
  coupling to the data plane is Envoy Gateway's external-authorization
  (`extAuth`, forward-auth) hook: a `SecurityPolicy` forwards requests to
  AuthRoute, which returns allow/deny. AuthRoute deliberately does **not** use
  Envoy Gateway's built-in OIDC (it is per-route and cannot back a shared
  cross-subdomain session) — AuthRoute owns the OIDC flow itself. *(Mechanism in
  ADR-0004; identity/session in ADR-0003.)*

Load-bearing principle to preserve in every change: **AuthRoute is an operator
that reconciles Kubernetes resources into authorization behavior, and policy is
modeled so that invalid or ambiguous states are hard to express** (favoring
typed, exhaustive Rust enums over loosely-coupled optional fields). This mirrors
the design discipline of `home-operations/kopiur`, the reference for this project.

### Non-goals (for `v1alpha1`)

- Not a general-purpose IdP — we consume OIDC, we do not issue primary identities.
- Not a directory service — no LLDAP/user store of our own.
- Not tied to ingress controllers other than Envoy Gateway / Gateway API.

## Consequences

- AuthRoute is firmly a Kubernetes operator; its surface is CRDs + reconcilers,
  and its lifecycle/testing follow operator patterns (kube-rs, kind-based e2e).
- Protection becomes a property co-located with the route, enabling GitOps and
  per-team ownership instead of a shared config file — but it raises new
  questions about defaults and cluster-wide policy that future ADRs must address.
- Binding tightly to Envoy Gateway's `extAuth` (forward-auth) hook buys us a
  maintained request-interception mechanism and narrows scope, at the cost of
  portability to other gateways. (Identity/OIDC is AuthRoute's own, not Envoy's.)
- This ADR sets direction only. The concrete, testable decisions — CRD shape
  (ADR-0002), identity/OIDC mapping (ADR-0003), and the Envoy integration
  mechanism (ADR-0004) — are deferred to their own records and still open.
