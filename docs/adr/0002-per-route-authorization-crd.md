# ADR-0002 — Per-route authorization custom resource

- **Status:** Accepted
- **Date:** 2026-06-13
- **Supersedes:** —
- **Companion:** [ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md), [ADR-0003](0003-identity-via-oidc-oauth.md), [ADR-0004](0004-envoy-gateway-integration-mechanism.md)
- **Scope:** CRD surface — `v1alpha1`
- **Informed by:** [research/gateway-api.md](../research/gateway-api.md), [research/authelia.md](../research/authelia.md), [research/envoy-gateway.md](../research/envoy-gateway.md)

## Context

[ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md) commits AuthRoute
to expressing authorization intent as a custom resource attached to each route,
rather than a central config file. This ADR must decide the **shape of that CRD**:

- What is the resource called, and what is its scope (namespaced vs. cluster)?
- How does it select/attach to a route? Does it `targetRef` a Gateway API
  `HTTPRoute` (the way Envoy Gateway's `SecurityPolicy` does), reference a
  `Gateway`, or carry route-matching of its own?
- How is the core intent modeled — the **needs-auth toggle** and the set of
  **allowed groups and/or users**? Following ADR-0001's load-bearing principle,
  "public vs. requires-auth-and-these-principals" should be a typed, exhaustive
  enum so invalid combinations (e.g. "public *and* restricted to group X") are
  unrepresentable.
- How does this resource relate to Envoy Gateway's `SecurityPolicy` — is it a
  higher-level abstraction AuthRoute reconciles *into* a `SecurityPolicy`, or
  does it sit beside one? (Coordinate with [ADR-0004](0004-envoy-gateway-integration-mechanism.md).)
- What does `.status` report (attached? accepted by the gateway? conflicts?), and
  what happens when multiple policies target the same route?

## Options considered

Now that [ADR-0003](0003-identity-via-oidc-oauth.md) and
[ADR-0004](0004-envoy-gateway-integration-mechanism.md) are settled (AuthRoute owns
OIDC + a domain-wide session, and enforces as an `extAuth.http` forward-auth
service), the per-route CRD only needs to express **which routes require which
principals** and let the operator wire enforcement. The options below were weighed;
the chosen option in each is collected in the Decision section.

### D1 — Resource form & route attachment
- **A (recommended): a Gateway API *Policy* CRD using `targetRef`.** Reuse
  `LocalPolicyTargetReferenceWithSectionName` to target an `HTTPRoute` (and, via
  `sectionName`, a single `HTTPRouteRule`). Inherits the ecosystem's
  `PolicyStatus`/`Accepted` conventions (see [research/gateway-api.md](../research/gateway-api.md)).
- B: a bespoke CRD with its own host/path selectors. *Rejected — reinvents a
  standardized, well-understood pattern.*
- C: annotations on `HTTPRoute`. *Rejected — untyped, no status, awkward for lists
  of principals.*

### D2 — Scope: per-route vs. inheritable default
- **A (recommended): support both.** A **Direct** policy attaches to an
  `HTTPRoute` (per-route); an **Inherited** policy attaches to a `Gateway` as a
  default ("everything behind this gateway requires auth"), overridden by any
  Direct route policy. Mirrors Gateway API's Direct/Inherited split.
  - Open sub-choice: one CRD kind distinguished by the
    `gateway.networking.k8s.io/policy` label + target kind, **or** two kinds
    (`RouteAuthPolicy` / `GatewayAuthPolicy`). Leaning two kinds for clarity.
- B: Direct per-route only. Simpler, but no "secure by default" — every route must
  opt in, and a missed route is silently public.

### D3 — How the auth requirement is modeled (the core enum)
Per ADR-0001's principle, make "public vs. authenticated vs. restricted" one
exhaustive enum so contradictory states can't be expressed.
- **A (recommended): a single externally-tagged `rule` enum:**
  - `public` — no auth, bypass.
  - `authenticated` — any valid AuthRoute session.
  - `restricted: { allow: [...] }` — authenticated **and** matches the allow-list.
- B: flat fields `requireAuth: bool` + optional `allowedGroups/Users`. *Rejected —
  permits contradictions (`requireAuth: false` with an allow-list set), violating
  the load-bearing principle.*

### D4 — Allow-list (principal) representation
Identity is `Subject { username, groups[] }` ([ADR-0003](0003-identity-via-oidc-oauth.md)).
- **A (recommended): a list of typed matchers** — externally-tagged
  `{ group: "x" } | { user: "y" }` with **any-of** (OR) semantics. Extensible to
  `{ claim: { name, value } }` later without breaking the shape.
- B: two flat lists `groups: []`, `users: []`. Simpler, matches Authelia's subject
  fields, but less extensible and OR-semantics are implicit.

### D5 — How `extAuth` is wired, and who authors the `SecurityPolicy`
Real-world Authelia wires ext-auth **gateway-wide**: a single `SecurityPolicy`
targets the `Gateway` and forwards *all* traffic to the auth service, which decides
per request (see [research/envoy-gateway.md](../research/envoy-gateway.md) and
[research/authelia.md](../research/authelia.md)). AuthRoute follows this: the
per-route CRDs are **not** compiled into many per-route `SecurityPolicy` resources;
they populate AuthRoute's runtime decision table, and one gateway-wide
`SecurityPolicy` does the wiring. The remaining choice is who owns that one resource:
- **A (chosen): AuthRoute's Helm chart deploys the gateway-wide
  `SecurityPolicy`.** The chart exposes `targetRefs` (which `Gateway`(s) to
  protect) as a value, and templates the rest from what AuthRoute requires:
  `backendRefs` → AuthRoute's Service, the ext-authz `path`, `failOpen: false`,
  and the `headersToExtAuth` (incl. `cookie`) / `headersToBackend` (`Remote-*`)
  lists. Declarative, install-time, GitOps-friendly — and **the controller needs
  no RBAC over `SecurityPolicy`** because it never reconciles it. Matches how this
  is run today.
- B: the **controller** generates/owns the `SecurityPolicy` at runtime
  (`ownerRef`). *Rejected* — needs RBAC over an Envoy CRD and a reconcile loop for
  a resource that changes only at install/upgrade time.
- C: users hand-author the `SecurityPolicy` entirely. *Rejected as the default* —
  error-prone (must match AuthRoute's expected `path`/header lists exactly); the
  chart does this for them. (Advanced users can still bring their own.)

### D5b — Matching a request to a route policy (new, forced by D5)
Because enforcement is gateway-wide, at request time AuthRoute receives only
forwarded metadata (host/path from `x-forwarded-*`, cookie) and must resolve which
`RouteAuthPolicy` applies.
- **A (recommended):** AuthRoute watches `HTTPRoute`s and the policies that
  `targetRef` them, and builds an in-memory table keyed by the route's
  **hostnames + path matches**. At request time it matches the forwarded host/path
  to a route, then applies that route's policy (default-deny or the Inherited
  Gateway default if none matches). This keeps authoring declarative (D1 `targetRef`)
  while enforcement is a fast runtime lookup.
- B: policies carry their own host/path matchers (no `targetRef` resolution).
  Simpler runtime, but duplicates routing info already in the `HTTPRoute` and can
  drift from it. *(Conflicts with D1-A.)*

### D6 — Conflicts & precedence
- **A (recommended):** at most one Direct policy per `(route, sectionName)`; a
  second fails to attach and reports a `ResolvedRefs`/`Accepted=False` condition
  (per Gateway API). A Direct route policy overrides an Inherited Gateway default.

### D7 — Namespacing & status
- **Recommended:** namespaced CRD living beside the `HTTPRoute`; local `targetRef`
  (same-namespace traffic only, per Gateway API's cross-namespace caveat). `.status`
  mirrors `PolicyStatus` with an `Accepted` condition and `ResolvedRefs` for a
  missing target/`sectionName`.

### Recommended shape (sketch)
```yaml
apiVersion: authroute.dev/v1alpha1
kind: RouteAuthPolicy            # Direct policy (D1-A, D2-A)
metadata:
  name: protect-grafana
  namespace: monitoring
spec:
  targetRef:                     # D1-A
    group: gateway.networking.k8s.io
    kind: HTTPRoute
    name: grafana
    # sectionName: <rule-name>   # optional: protect one rule
  rule:                          # D3-A — exactly one of:
    restricted:
      allow:                     # D4-A — any-of
        - group: admins
        - user: alice@example.com
---
apiVersion: authroute.dev/v1alpha1
kind: GatewayAuthPolicy          # Inherited default (D2-A)
metadata: { name: require-login, namespace: gateway-system }
spec:
  targetRef: { group: gateway.networking.k8s.io, kind: Gateway, name: public }
  rule: { authenticated: {} }
```

## Decision

The per-route authorization surface is two Gateway API-style **Policy** CRDs in
group `authroute.dev`:

1. **`RouteAuthPolicy`** (Direct) and **`GatewayAuthPolicy`** (Inherited default),
   both namespaced (D1, D2, D7). Each carries a `targetRef`
   (`LocalPolicyTargetReferenceWithSectionName`): `RouteAuthPolicy` targets an
   `HTTPRoute` (optionally one `HTTPRouteRule` via `sectionName`);
   `GatewayAuthPolicy` targets a `Gateway` to set a default for everything behind
   it. A Direct policy overrides the Inherited default (D6).
2. **The intent is one exhaustive `rule` enum** (D3), externally tagged:
   - `public` — bypass, no auth.
   - `authenticated` — any valid AuthRoute session.
   - `restricted: { allow: [...] }` — authenticated **and** matching the allow-list.
3. **The allow-list is a list of typed, any-of matchers** (D4): `{ group: … }` or
   `{ user: … }`, evaluated against the session `Subject { username, groups[] }`
   from [ADR-0003](0003-identity-via-oidc-oauth.md). The matcher enum is open to
   future variants (e.g. `{ claim: … }`).
4. **Enforcement is wired gateway-wide, by Helm, not by the controller** (D5).
   AuthRoute's Helm chart ships one `SecurityPolicy` per protected `Gateway` —
   `targetRefs` is a chart value; `backendRefs`, ext-authz `path`, `failOpen:
   false`, and the `headersToExtAuth`/`headersToBackend` lists are templated from
   what AuthRoute requires. The controller holds **no RBAC over `SecurityPolicy`**.
5. **The controller resolves requests to policies at runtime** (D5b): it watches
   `HTTPRoute`s and the policies that `targetRef` them and maintains an in-memory
   table keyed by route hostnames + path matches. Per request it matches the
   forwarded host/path to a route and applies that route's policy, falling back to
   the Inherited `GatewayAuthPolicy` or, absent that, **default-deny**.
6. **Status** mirrors `PolicyStatus` (D6, D7): an `Accepted` condition per targeted
   ancestor, and `ResolvedRefs`/`Accepted=False` when the target or `sectionName`
   doesn't exist or a second Direct policy contends for the same `(route,
   sectionName)`.

See the "Recommended shape (sketch)" above for the concrete YAML.

## Consequences

- The gateway-wide `SecurityPolicy` is shipped by the **Helm chart** with
  `targetRefs` as a value (D5-A); the **controller needs no RBAC over
  `SecurityPolicy`** and runs no reconcile loop for it. The chart and AuthRoute
  must agree on the ext-authz `path` and header lists.
- AuthRoute must maintain a **runtime host/path → policy table** from watched
  `HTTPRoute`s and policies (D5b-A); route/policy changes must reconcile into it,
  and a request matching no route falls back to default-deny or the Inherited
  Gateway default.
- The `rule` enum keeps invalid policy states unrepresentable (ADR-0001), at the
  cost of a slightly less "obvious" surface than flat booleans.
- Supporting Inherited Gateway defaults (D2-A) adds **override/merge semantics**
  the reconciler and `.status` must make legible.

