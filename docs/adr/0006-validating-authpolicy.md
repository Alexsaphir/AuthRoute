# ADR-0006 — Validating AuthPolicy via an admission webhook

- **Status:** Accepted
- **Date:** 2026-06-14
- **Supersedes:** —
- **Companion:** [ADR-0002](0002-per-route-authorization-crd.md)
- **Scope:** AuthPolicy validation & status — `v1alpha1`
- **Informed by:** [research/gateway-api.md](../research/gateway-api.md), [research/cel-policy.md](../research/cel-policy.md), [research/kopiur.md](../research/kopiur.md)

## Context

[ADR-0002](0002-per-route-authorization-crd.md) defines the `AuthPolicy` CRD: it
`targetRef`s an `HTTPRoute` (§D1) and carries policy as **CEL expressions** —
`defaultPolicy` plus an ordered `extraPolicy` list of `{ pathRegex, policy }`
(§D3–D4). Three things in that surface are *strings referencing other things* and
can be wrong in ways the CRD's OpenAPI schema cannot catch:

- a **CEL expression** that doesn't parse, doesn't type-check against the activation
  schema, or doesn't return `bool`;
- a **`pathRegex`** that isn't a valid regular expression;
- a **`targetRef`** pointing at an `HTTPRoute` that doesn't exist.

AuthRoute is **fail-closed on the request hot path**
([ADR-0004](0004-envoy-gateway-integration-mechanism.md),
[ADR-0005](0005-session-storage.md)), so a malformed policy must **never** reach
request-time evaluation. ADR-0002 §D3 made the deliberate trade of CEL's
stringly-typed flexibility for **admission-time** validation; this ADR decides the
mechanism and how results are reported.

OpenAPI structural validation and CRD `x-kubernetes-validations` (CEL) can enforce
*shape* (required `defaultPolicy`, non-empty strings, list bounds) but **cannot**
compile a CEL expression that lives inside a field value, compile a regex, or check
that a referenced `HTTPRoute` exists (a cross-resource lookup). Schema validation is
therefore necessary but insufficient.

## Decision

**A Kubernetes `ValidatingAdmissionWebhook`, served by a dedicated AuthRoute webhook
binary, validates every `AuthPolicy` create/update** and rejects invalid ones
synchronously. The controller separately reports ongoing attachment status.

1. **Webhook checks (reject admission with an actionable message on any failure):**
   1. **Target exists** — the `HTTPRoute` named by `targetRef` exists in the
      policy's namespace (local `targetRef`, ADR-0002 §D7).
   2. **CEL valid** — `defaultPolicy` and every `extraPolicy[].policy` parse,
      **type-check** against the fixed activation schema (`user: string`,
      `groups: list<string>`, `claims: map<string, dyn>` — ADR-0002 §D4), and yield
      `bool`.
   3. **Regex valid** — every `extraPolicy[].pathRegex` compiles with the **same
      regex engine** used at request time.
2. **Shared validation path.** The webhook calls the runtime-free `api` crate's
   parse/type-check routines (ADR-0002 §D4 implementation note), so a policy that
   passes admission is **guaranteed to compile and type-check identically** in the
   request-time decision service. Admission and runtime never disagree.
3. **Webhook binary.** A separate `webhook` binary in the layered workspace
   (kopiur pattern — [research/kopiur.md](../research/kopiur.md)), depending only on
   `api`. Its `ValidatingWebhookConfiguration`, serving cert, and Service are
   Helm-managed; it is scoped to the `authroute.dev` `AuthPolicy` resource only.
4. **`failurePolicy: Fail`.** Consistent with AuthRoute's fail-closed posture: if
   the webhook is unavailable, `AuthPolicy` **writes** are rejected rather than
   admitted unvalidated. Because the webhook is scoped to `AuthPolicy` only, an
   outage blocks just those writes — not unrelated cluster operations, and not the
   request hot path (the webhook is not on it).
5. **Status is the controller's job, not the webhook's** — the cross-resource
   caveat. The webhook's existence check (1.i) is point-in-time; the `HTTPRoute` can
   be deleted *after* the `AuthPolicy` is admitted. So the controller reports Gateway
   API `PolicyStatus` conditions over time: `Accepted`, `ResolvedRefs=False` when the
   target later goes missing, and a conflict condition when a second
   `AuthPolicy` contends for the same `HTTPRoute` (ADR-0002 §D6). The
   controller must re-evaluate on `HTTPRoute` changes, not only on `AuthPolicy`
   changes.
6. **Cheap structural checks stay in the schema.** Required `defaultPolicy`,
   non-empty strings, and list bounds use OpenAPI / `x-kubernetes-validations` for
   the earliest, cheapest feedback and to reduce webhook load; the webhook owns only
   the checks the schema cannot express.

Division of labor: **webhook = synchronous fail-fast on malformed/missing-target
policies at write time; controller `.status` = eventual truth about live
attachment.**

## Consequences

- Authors get **immediate, actionable errors at `kubectl apply` time** (bad CEL, bad
  regex, missing route) instead of silent request-time denials — the payoff for
  ADR-0002 §D3's stringly-typed CEL choice.
- A policy admitted by the webhook is **guaranteed to evaluate at request time**
  (shared `api`-crate path), so the hot path needs no defensive re-parsing beyond
  loading the cached program.
- New operational surface: a **webhook deployment** with a TLS serving cert and a
  `ValidatingWebhookConfiguration` (Helm-managed, cert rotation to handle). With
  `failurePolicy: Fail`, webhook downtime blocks `AuthPolicy` writes — accepted,
  since policy authoring is not the request hot path and the scope is narrow.
- **Two layers of truth** (synchronous webhook + asynchronous controller status)
  must stay consistent; the controller has to watch `HTTPRoute`s and reconcile
  `ResolvedRefs` when targets appear/disappear after admission.
- The webhook cannot catch everything statically — e.g. a CEL expression that is
  type-valid but semantically wrong (`'true'` where the author meant a group check)
  still admits. Validation guarantees *well-formed*, not *correct*, policy.