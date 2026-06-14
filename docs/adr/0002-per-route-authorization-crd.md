# ADR-0002 — Per-route authorization custom resource

- **Status:** Accepted
- **Date:** 2026-06-14
- **Supersedes:** —
- **Companion:** [ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md), [ADR-0003](0003-identity-via-oidc-oauth.md), [ADR-0004](0004-envoy-gateway-integration-mechanism.md), [ADR-0006](0006-validating-authpolicy.md)
- **Scope:** CRD surface — `v1alpha1`
- **Informed by:** [research/gateway-api.md](../research/gateway-api.md), [research/authelia.md](../research/authelia.md), [research/envoy-gateway.md](../research/envoy-gateway.md), [research/cel-policy.md](../research/cel-policy.md)

## Context

[ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md) commits AuthRoute
to expressing authorization intent as a custom resource attached to each route,
rather than a central config file. This ADR decides the **shape of that CRD**:

- What is the resource called, how many kinds, and what is its scope (namespaced
  vs. cluster)?
- How does it select/attach to a route? Does it `targetRef` a Gateway API
  `HTTPRoute` (the way Envoy Gateway's `SecurityPolicy` does), reference a
  `Gateway`, or carry route-matching of its own?
- **How is the authorization decision expressed** — a fixed enum of cases, or a
  general predicate? ADR-0001's principle pushes toward making invalid/ambiguous
  states unrepresentable; that must be weighed against the expressiveness real
  policies need (conjunctions, claim checks), and against per-sub-path overrides.
- How does this resource relate to Envoy Gateway's `SecurityPolicy` — is it a
  higher-level abstraction AuthRoute reconciles *into* a `SecurityPolicy`, or
  does it sit beside one? (Coordinate with [ADR-0004](0004-envoy-gateway-integration-mechanism.md).)
- What does `.status` report (attached? accepted by the gateway? conflicts?), and
  what happens when multiple policies target the same route, or none does?

## Options considered

With [ADR-0003](0003-identity-via-oidc-oauth.md) and
[ADR-0004](0004-envoy-gateway-integration-mechanism.md) settled (AuthRoute owns
OIDC + a domain-wide session, and enforces as an `extAuth.http` forward-auth
service), the per-route CRD needs to express **which routes admit which requests**
and let the operator wire enforcement. The chosen option in each sub-decision is
collected in the Decision section.

### D1 — Resource form & route attachment
- **A (chosen): a CRD owned by this project, following the Gateway API
  Policy-attachment pattern.** AuthRoute defines its own CRD in group
  `authroute.dev` (not a reused upstream type), which **targets an `HTTPRoute`** via
  a Gateway API `targetRef` — reusing `LocalPolicyTargetReference` (`{group, kind,
  name}`, same namespace). The whole `HTTPRoute` is the unit of attachment;
  sub-path refinement is handled by `extraPolicy` (D4), not by a
  `sectionName`-pinned `HTTPRouteRule`. Project-owned schema,
  standard attachment mechanism. How attachment validity and status are
  checked/reported is decided in [ADR-0006](0006-validating-authpolicy.md).
- B: a project CRD with its **own** host/path selectors instead of `targetRef`.
  *Rejected — reinvents a standardized, well-understood attachment pattern and can
  drift from the `HTTPRoute` it duplicates.*
- C: annotations on `HTTPRoute`. *Rejected — untyped, no status, awkward for
  expressions.*

### D2 — Number of kinds, scope, and how "secure by default" is achieved
- **A (chosen): a single kind, `AuthPolicy`, that targets an `HTTPRoute` only
  (Direct attachment).** No Gateway-level attachment and no separate "inherited
  default" kind. Secure-by-default is provided by the **default-deny** fallback
  (D5b) — a route with no `AuthPolicy` is denied — so an inheritable Gateway
  default is unnecessary. Simplest surface: one schema, one reconciler path, one
  attachment target.
- B (rejected): **two kinds** — `RouteAuthPolicy` (targets `HTTPRoute`) plus an
  Inherited `GatewayAuthPolicy` (targets a `Gateway` to set a default for
  everything behind it). The Direct/Inherited split is a real Gateway API pattern
  ([research/gateway-api.md](../research/gateway-api.md)), but it buys little here:
  default-deny already secures unconfigured routes, while Inherited policies add
  override/merge semantics, a second schema, and Gateway-target RBAC for no clear
  gain. *Rejected to keep the surface minimal; can be added later if a real need
  for a single overridable cluster default appears.*

### D3 — How the authorization decision is expressed (the core model)
- **A (chosen): a CEL boolean expression.** Each policy carries a CEL string
  evaluated against the session `Subject`; `true` allows, `false` denies. CEL is
  the ecosystem-standard policy language — Kubernetes `ValidatingAdmissionPolicy`,
  Envoy's RBAC `condition`, and Kyverno's `AuthorizationPolicy` (CEL over an Envoy
  ext_authz `CheckRequest`, the closest prior art) — and is non-Turing-complete and
  **statically type-checkable** ([research/cel-policy.md](../research/cel-policy.md)).
  The cases a fixed enum would offer collapse to expressions: public → `'true'`,
  authenticated → `'user != ""'`, restricted → `'"admins" in groups'`.
- B (rejected): a typed, exhaustive `rule` enum (`public` / `authenticated` /
  `restricted: { allow: [{group}|{user}] }`). It keeps invalid states
  unrepresentable at the Rust type level (ADR-0001), but **cannot express
  conjunction** ("user `bob` **and** in group `admins`") or claim-based predicates
  — expressiveness Authelia's `[][]string` AND/OR model has
  ([research/authelia.md](../research/authelia.md)) — and every new policy shape
  needs a new field plus a reconciler arm. The expressiveness gap decided against
  it.
- **Trade & mitigation:** a CEL string is *not* a typed Rust enum, so a typo or
  type error (`groups == "admin"`) is representable. We recover ADR-0001's "invalid
  states caught early" guarantee at **author time** rather than the Rust type level
  by type-checking at admission (D4) — never at request time, since AuthRoute is
  fail-closed on the hot path ([ADR-0004](0004-envoy-gateway-integration-mechanism.md)).

### D4 — Policy spec shape, activation, and validation
- **A (chosen): `defaultPolicy` plus an ordered `extraPolicy` override list.**
  `defaultPolicy` is a CEL expression for the whole target; `extraPolicy` is an
  optional, **ordered** list of `{ pathRegex, policy }`. At request time the
  **first** entry whose `pathRegex` matches the request path wins; otherwise
  `defaultPolicy` applies. Ordered first-match mirrors Authelia's rule evaluation;
  `pathRegex` sub-divides the already-`targetRef`-ed route (analogous to Authelia's
  `resources` regex) — a refinement *within* the route, not its own route selection
  (so it does not conflict with D5b).
- **Activation (fixed, versioned).** Expressions reference the session `Subject`
  ([ADR-0003](0003-identity-via-oidc-oauth.md)): `user: string`,
  `groups: list<string>` (tested with CEL `in`), and a reserved
  `claims: map<string, dyn>` for raw OIDC claims (additive when wired). An
  expression **must** evaluate to `bool`.
- **Validation (decided in [ADR-0006](0006-validating-authpolicy.md)).** Every
  `defaultPolicy` and `extraPolicy[].policy` must compile and type-check against the
  activation schema and yield `bool`, and every `pathRegex` must be a valid regex.
  These checks run in an admission webhook, so an invalid policy is **rejected at
  write time** and never reaches the hot path.
- **Implementation note.** The `cel` crate lives in the runtime-free `api` crate
  (per ADR-0001 / kopiur layering) so the admission webhook
  ([ADR-0006](0006-validating-authpolicy.md)) and the request-time decision service
  share one parse/type-check/evaluate path.

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
  no RBAC over `SecurityPolicy`** because it never reconciles it.
- B: the **controller** generates/owns the `SecurityPolicy` at runtime
  (`ownerRef`). *Rejected* — needs RBAC over an Envoy CRD and a reconcile loop for
  a resource that changes only at install/upgrade time.
- C: users hand-author the `SecurityPolicy` entirely. *Rejected as the default* —
  error-prone (must match AuthRoute's expected `path`/header lists exactly); the
  chart does this for them. (Advanced users can still bring their own.)

> Because the gateway-wide `SecurityPolicy` forwards **all** traffic to AuthRoute,
> a request whose route carries no `AuthPolicy` still reaches the decision service
> and is **denied** (D5b). To expose a route you attach an `AuthPolicy` — even a
> public one is explicit (`defaultPolicy: 'true'`).

### D5b — Matching a request to a route policy, and evaluating it
Because enforcement is gateway-wide, at request time AuthRoute receives only
forwarded metadata (host/path from `x-forwarded-*`, cookie) and must resolve which
policy applies and evaluate it.
- **A (chosen):** AuthRoute watches `HTTPRoute`s and the `AuthPolicy`s that
  `targetRef` them, and builds an in-memory table keyed by the route's
  **hostnames + path matches**. Per request it (1) matches the forwarded host/path
  to a route → its `AuthPolicy`; if none → **default-deny**. (2) Selects the
  expression — first `extraPolicy[].pathRegex` matching the path, else
  `defaultPolicy`. (3) Evaluates it against the `Subject` activation. `true` →
  **allow** (inject `Remote-*` headers, ADR-0003); `false` → **deny**, resolved
  two-phase (per [research/authelia.md](../research/authelia.md)): unauthenticated →
  redirect to the OIDC portal; authenticated → `403`.
- **Reconcile on changes to *either* resource.** Because the table key is derived
  from the `HTTPRoute` while the decision comes from the `AuthPolicy`, the controller
  must rebuild the affected entry when **either** changes:
  - an `AuthPolicy` is created/updated/deleted (its expressions or `targetRef`
    change), and
  - the **referenced `HTTPRoute`** changes — its `hostnames` or path matches are
    edited (the table key drifts), or it is added/deleted (a policy's target
    appears/disappears, which also drives `ResolvedRefs`,
    [ADR-0006](0006-validating-authpolicy.md)).

  So the controller watches `HTTPRoute`s for their own sake, not only as a lookup —
  an edit to a route that a policy targets reconciles that policy's table entry even
  though the `AuthPolicy` object is unchanged.
- B: policies carry their own host/path matchers (no `targetRef` resolution).
  Simpler runtime, but duplicates routing info already in the `HTTPRoute` and can
  drift from it. *(Conflicts with D1-A.)*

### D6 — Conflicts & precedence
- **A (chosen):** at most one `AuthPolicy` per `HTTPRoute`; a second
  fails to attach and reports a `ResolvedRefs`/`Accepted=False` condition (per
  Gateway API). Sub-path precedence lives entirely inside one policy's ordered
  `extraPolicy` list (D4), so there are no cross-policy override semantics.

### D7 — Namespacing
- **Chosen:** namespaced CRD living beside the `HTTPRoute`; local `targetRef`
  (same-namespace traffic only, per Gateway API's cross-namespace caveat).
  Validation and `.status`/`PolicyStatus` reporting are decided in
  [ADR-0006](0006-validating-authpolicy.md).

### Recommended shape (sketch)
```yaml
apiVersion: authroute.dev/v1alpha1
kind: AuthPolicy                 # single kind (D2-A), targets an HTTPRoute (D1-A)
metadata:
  name: protect-grafana
  namespace: monitoring
spec:
  targetRef:                     # D1-A
    group: gateway.networking.k8s.io
    kind: HTTPRoute
    name: grafana
  defaultPolicy: '"admins" in groups || user == "alice@example.com"'   # D3-A
  extraPolicy:                   # D4-A — optional, ordered; first match wins
    - pathRegex: '^/public(/.*)?$'
      policy: 'true'             # carve out a public sub-path
    - pathRegex: '^/api(/.*)?$'
      policy: 'user != ""'       # any authenticated user
```

Per default-deny (D5b), the sketch's public sub-path is explicit
(`policy: 'true'`), never implicit.

## Decision

The per-route authorization surface is a single Gateway API-style **Policy** CRD in
group `authroute.dev`:

1. **`AuthPolicy`** (Direct), namespaced (D1, D2, D7), carrying a `targetRef`
   (`LocalPolicyTargetReference`) that targets an **`HTTPRoute`** as a whole.
   Sub-path scoping is done with `extraPolicy` (D4), not a `sectionName`. There is
   **no** Gateway-level or inherited-default kind; secure-by-default comes from
   default-deny (D5b).
2. **The authorization decision is a CEL boolean expression** (D3). The `spec`
   carries a required `defaultPolicy` and an optional, **ordered** `extraPolicy`
   list of `{ pathRegex, policy }`; per request, the first matching `pathRegex`
   wins, else `defaultPolicy`. Expressions evaluate against the session `Subject`
   activation — `user: string`, `groups: list<string>`, reserved
   `claims: map` ([ADR-0003](0003-identity-via-oidc-oauth.md)) — and must yield
   `bool` (`true` = allow, `false` = deny).
3. **Validity is enforced by an admission webhook** (D4,
   [ADR-0006](0006-validating-authpolicy.md)): each expression must compile,
   type-check against the activation schema, and yield `bool`; each `pathRegex` must
   compile; the targeted `HTTPRoute` must exist. Invalid policies are rejected at
   write time and never reach the hot path.
4. **Enforcement is wired gateway-wide, by Helm, not by the controller** (D5).
   AuthRoute's Helm chart ships one `SecurityPolicy` per protected `Gateway` —
   `targetRefs` is a chart value; `backendRefs`, ext-authz `path`, `failOpen:
   false`, and the `headersToExtAuth`/`headersToBackend` lists are templated from
   what AuthRoute requires. The controller holds **no RBAC over `SecurityPolicy`**.
5. **The controller resolves and evaluates requests at runtime** (D5b): it watches
   `HTTPRoute`s and the `AuthPolicy`s that `targetRef` them and maintains an
   in-memory table keyed by route hostnames + path matches. Per request it matches
   the forwarded host/path to a route, selects the expression (`extraPolicy`
   first-match else `defaultPolicy`), evaluates it against the `Subject`, and
   returns allow (with `Remote-*` headers) or deny — unauthenticated denials
   redirect to the OIDC portal, authenticated denials return `403`
   ([ADR-0004](0004-envoy-gateway-integration-mechanism.md)). **A request matching
   no `AuthPolicy` is denied (default-deny).**
6. **Validation and status are decided in [ADR-0006](0006-validating-authpolicy.md)**:
   an admission webhook rejects invalid policies synchronously, and the controller
   reports ongoing `PolicyStatus`/`Accepted`/`ResolvedRefs` conditions (e.g. the
   target `HTTPRoute` later deleted, or a second `AuthPolicy` contending for the same
   `HTTPRoute`, D6).

See the "Recommended shape (sketch)" above for the concrete YAML.

## Consequences

- **Minimal surface**: one kind, one attachment target (`HTTPRoute`), one reconciler
  path. No Direct/Inherited override/merge semantics to implement or explain. If a
  single overridable cluster-wide default is ever needed, a Gateway-targeting kind
  can be added later (D2-B) without disturbing `AuthPolicy`.
- **Secure by default, but explicit**: default-deny means an unconfigured route is
  unreachable until an `AuthPolicy` exists — even a public route must be declared
  (`defaultPolicy: 'true'`). This is the deliberate inverse of the static-config
  pain point: nothing is silently exposed, but every exposure is an explicit object.
- **Expressiveness**: conjunction, disjunction, negation, and (once `claims` is
  wired) claim-based policy are all expressible — matching or exceeding Authelia's
  subject model. Operators reuse the CEL idiom they already write for
  Kubernetes/Envoy/Kyverno (`"admins" in groups`).
- **Tension with ADR-0001, accepted with mitigation**: a CEL string is not a typed
  Rust enum, so "invalid states unrepresentable" no longer holds at the type level
  — a typo or type error is expressible. The guarantee moves to **author time**:
  an admission webhook type-checks the CEL against a fixed activation and rejects
  bad policies at write time ([ADR-0006](0006-validating-authpolicy.md)) rather than
  failing a live request. This is the deliberate trade — compile-time-in-Rust safety
  for admission-validated expressiveness.
- **New dependency**: the `cel` crate, in the runtime-free `api` crate, shared by
  reconciler validation and the decision service. Expressions compile once per
  policy version and are cached; evaluation is per request on the hot path and must
  stay cheap.
- **`pathRegex` granularity**: `extraPolicy` sub-divides an already-`targetRef`-ed
  route — a refinement of D5b, not a contradiction (the policy still does not
  *select* its route by path). It is the **only** sub-route mechanism: there is no
  `sectionName`, so all path scoping lives in one policy's ordered list.
  `extraPolicy` is **order-sensitive** (first match wins), so reordering changes
  behavior — `.status`/docs must make the order legible.
- The gateway-wide `SecurityPolicy` is shipped by the **Helm chart** with
  `targetRefs` as a value (D5-A); the **controller needs no RBAC over
  `SecurityPolicy`** and runs no reconcile loop for it. The chart and AuthRoute must
  agree on the ext-authz `path` and header lists.
- AuthRoute must maintain a **runtime host/path → policy table** from watched
  `HTTPRoute`s and `AuthPolicy`s (D5b-A). It reconciles the table on changes to
  **either** resource — an `AuthPolicy` create/update/delete **or** an edit/delete of
  a referenced `HTTPRoute` (whose `hostnames`/path matches form the table key) — so
  the index can't drift from the routes it mirrors. A request matching no route is
  denied.