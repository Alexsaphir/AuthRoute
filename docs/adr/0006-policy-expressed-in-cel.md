# ADR-0006 — Authorization policy expressed in CEL

- **Status:** Accepted
- **Date:** 2026-06-14
- **Supersedes:** [ADR-0002](0002-per-route-authorization-crd.md) §D3, §D4 (the `rule` enum and typed allow-list)
- **Companion:** [ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md), [ADR-0002](0002-per-route-authorization-crd.md), [ADR-0003](0003-identity-via-oidc-oauth.md)
- **Scope:** CRD policy surface — `v1alpha1`
- **Informed by:** [research/cel-policy.md](../research/cel-policy.md), [research/authelia.md](../research/authelia.md)

## Context

[ADR-0002](0002-per-route-authorization-crd.md) modeled the authorization intent as
a typed, exhaustive `rule` enum (`public` / `authenticated` / `restricted: { allow:
[{group}|{user}] }`, §D3–D4), chosen so contradictory states are unrepresentable
([ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md)'s load-bearing
principle).

Reviewing that surface against real-world needs surfaced two gaps
([research/cel-policy.md](../research/cel-policy.md)):

- The flat any-of allow-list **cannot express conjunction** ("user `bob` **and** in
  group `admins`") or **claim-based** predicates — expressiveness Authelia's
  `[][]string` AND/OR model has ([research/authelia.md](../research/authelia.md)).
- The decision logic is a closed enum; every new shape of policy needs a new CRD
  field and reconciler arm.

**CEL** (Common Expression Language) is the de-facto policy language of the
surrounding stack — Kubernetes (`ValidatingAdmissionPolicy`, CRD validation, the
authorizer), Envoy's RBAC `condition`, and Kyverno's `AuthorizationPolicy` (CEL over
an Envoy ext_authz `CheckRequest`, the closest prior art to AuthRoute). It is
non-Turing-complete (every expression terminates, no side effects) and
**statically type-checkable** against a declared variable environment. A maintained
pure-Rust implementation exists (the `cel` crate). This ADR decides to express
AuthRoute policy as CEL, and fixes the shape of the policy spec.

This **supersedes ADR-0002 §D3 and §D4 only**. ADR-0002's resource form and
attachment (§D1 `targetRef`), the two kinds (§D2 `RouteAuthPolicy` /
`GatewayAuthPolicy`), the gateway-wide Helm-owned `SecurityPolicy` (§D5), runtime
route resolution (§D5b), conflicts (§D6), and namespacing/status (§D7) **stand
unchanged**. CEL replaces the contents of the policy `spec`, not how it attaches.

## Decision

We will express every authorization decision as a **CEL boolean expression**, and
the policy `spec` carries a default expression plus ordered per-sub-path overrides.

### 1. Spec shape

A policy (`RouteAuthPolicy` or `GatewayAuthPolicy`, per ADR-0002 §D1–D2) keeps its
`targetRef` and replaces the `rule` enum with:

```yaml
apiVersion: authroute.dev/v1alpha1
kind: RouteAuthPolicy
metadata: { name: protect-grafana, namespace: monitoring }
spec:
  targetRef:                       # unchanged — ADR-0002 §D1
    group: gateway.networking.k8s.io
    kind: HTTPRoute
    name: grafana
  defaultPolicy: '"admins" in groups || user == "alice@example.com"'
  extraPolicy:                     # optional, ordered; first match wins
    - pathRegex: '^/public(/.*)?$'
      policy: 'true'               # carve out a public sub-path
    - pathRegex: '^/api(/.*)?$'
      policy: 'user != ""'         # any authenticated user
```

- **`defaultPolicy`** (required): a CEL expression evaluated for the whole target.
- **`extraPolicy`** (optional): an **ordered list** of `{ pathRegex, policy }`. At
  request time, the **first** entry whose `pathRegex` matches the request's path
  wins; if none match, `defaultPolicy` applies. (Ordered first-match mirrors
  Authelia's rule evaluation — [research/authelia.md](../research/authelia.md).)

The old enum maps directly, so nothing is lost: `public` → `'true'`,
`authenticated` → `'user != ""'`, `restricted {allow group admins}` →
`'"admins" in groups'`.

### 2. Activation (the variables an expression may reference)

Expressions are evaluated against the session `Subject`
([ADR-0003](0003-identity-via-oidc-oauth.md)):

- `user: string` — the username (empty string when unauthenticated).
- `groups: list<string>` — group memberships; tested with CEL `in`.
- `claims: map<string, dyn>` — **reserved** for raw OIDC claims (forward-compatible;
  may be empty until wired). The activation schema is **fixed and versioned**; new
  variables are an additive change.

An expression **must** evaluate to `bool`. `true` = allow, `false` = deny.

### 3. Request-time evaluation

Extending ADR-0002 §D5b's runtime lookup:

1. Resolve the forwarded host/path to a route → its policy (else the Inherited
   `GatewayAuthPolicy`, else **default-deny**).
2. Select the expression: first `extraPolicy[].pathRegex` matching the request
   path, else `defaultPolicy`.
3. Evaluate it against the `Subject` activation.
4. `true` → **allow** (inject `Remote-*` headers, ADR-0003). `false` → **deny**,
   resolved two-phase (per [research/authelia.md](../research/authelia.md)): if the
   request is **unauthenticated**, redirect to the OIDC login portal; if it is
   **authenticated** but the expression is false, return **403**.

### 4. Validation moves to admission/reconcile time

Because a CEL string can be malformed or ill-typed, and because AuthRoute is
fail-closed on the hot path ([ADR-0005](0005-session-storage.md)), every expression
and every `pathRegex` is validated **when the policy is reconciled, not at request
time**:

- `defaultPolicy` and each `extraPolicy[].policy` must **compile** and
  **type-check** against the fixed activation schema (§2) and yield `bool`.
- each `pathRegex` must compile as a regular expression.

Failures set `Accepted=False` with a descriptive reason on `.status` (the same place
ADR-0002 §D6–D7 reports conflicts). An invalid policy never reaches the decision
path; the route falls back to default-deny.

## Consequences

- **Expressiveness**: conjunction, disjunction, negation, and (once `claims` is
  wired) claim-based policy are all expressible — closing the gaps in ADR-0002 §D4
  and matching/exceeding Authelia's subject model.
- **Ecosystem fit**: operators already write CEL for Kubernetes/Envoy/Kyverno; the
  `user`/`groups` idiom (`"admins" in groups`) is identical to
  `ValidatingAdmissionPolicy`.
- **Tension with ADR-0001, accepted with mitigation**: a CEL string is *not* a
  typed Rust enum, so "invalid states unrepresentable" no longer holds at the Rust
  type level — a typo or type error is *expressible*. We recover the guarantee at
  **author time**: CEL's static type-checking against a fixed activation runs at
  admission/reconcile, and bad policies are rejected on `.status` rather than
  failing a live request (§4). This is the deliberate trade — compile-time-in-Rust
  safety for admission-time-validated expressiveness.
- **New dependency**: the `cel` crate, in the runtime-free `api` crate (so both the
  reconciler's validator and the decision service share one
  parse/type-check/evaluate path). Expressions compile once per policy version and
  are cached; evaluation is per request on the hot path and must stay cheap.
- **`pathRegex` granularity**: `extraPolicy` sub-divides an already-`targetRef`-ed
  route into sub-paths — a refinement of ADR-0002 §D5b, not a contradiction (the
  policy still does not *select* its route by path; `targetRef` does). It overlaps
  with `sectionName` (ADR-0002 §D1) but is finer-grained; both may coexist.
- **Ordered list semantics**: `extraPolicy` is order-sensitive (first match wins),
  so reordering changes behavior — `.status` / docs must make the order legible, and
  authors must understand it the way they would Authelia's rule order.
- ADR-0002's untouched sections (§D1, §D2, §D5, §D5b, §D6, §D7) continue to govern;
  this ADR is additive to them.