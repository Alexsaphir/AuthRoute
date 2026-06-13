# Gateway API (`kubernetes-sigs/gateway-api`)

Reviewed June 2026. Focus: how a custom resource attaches to a route, since
AuthRoute's per-route auth resource must do exactly this (ADR-0002).

## Files read

- `apis/v1/policy_types.go` — the policy-attachment target references and status.
- `apis/v1alpha2/policy_types.go` — type aliases re-exporting the v1 types.
- `apis/v1/httproute_types.go`, `apis/v1/shared_types.go`, `apis/v1/object_reference_types.go`.
- `geps/` — `gep-713` (policy attachment), `gep-1713`, `gep-2648`, `gep-2649`.

## Key findings

### Policy attachment is a standardized pattern — don't reinvent it

`apis/v1/policy_types.go` defines the canonical way a "policy" CRD targets a
Gateway API object:

- **`LocalPolicyTargetReference`** — `{Group, Kind, Name}`, same namespace.
- **`NamespacedPolicyTargetReference`** — adds optional `Namespace` for
  cross-namespace targeting. Note the rule: even when targeting another
  namespace, the policy "MUST only apply to traffic originating from the same
  namespace as the policy."
- **`LocalPolicyTargetReferenceWithSectionName`** — embeds the local ref plus an
  optional `SectionName`. `SectionName` selects a sub-part of the target:
  - `Gateway` → Listener name
  - `HTTPRoute` → **HTTPRouteRule name**
  - `Service` → port name

  If the named section doesn't exist, "the Policy must fail to attach, and the
  policy implementation should record a `ResolvedRefs` or similar Condition."

### Direct vs. Inherited

A policy CRD is labeled `gateway.networking.k8s.io/policy: Direct | Inherited`
(`PolicyLabelKey`):

- **Direct** — affects only the object it attaches to.
- **Inherited** — applied at a parent (e.g. Gateway) it cascades to all attached
  routes/backends.

### Standardized status

`PolicyStatus` / `PolicyAncestorStatus` carry an `Accepted` condition
(`PolicyConditionAccepted`) reporting whether each targeted ancestor accepted or
rejected the policy and why.

## Implications for AuthRoute (ADR-0002)

- AuthRoute's per-route auth resource should be a **Gateway API Policy**: reuse
  `LocalPolicyTargetReferenceWithSectionName` to target an `HTTPRoute` (and a
  specific rule via `SectionName`), rather than inventing a bespoke selector.
- Strongly consider supporting **both** Direct (per-route override) and Inherited
  (Gateway-wide default: "everything behind this gateway requires auth") — this is
  an explicit ADR-0002 decision.
- Mirror `PolicyStatus`/`Accepted` conventions in `.status`, including the
  "fail to attach + report condition" behavior for missing `SectionName`.
