# ADR-0007 — Layered Cargo workspace and manifest codegen

- **Status:** Accepted
- **Date:** 2026-06-14
- **Supersedes:** —
- **Companion:** [ADR-0002](0002-per-route-authorization-crd.md), [ADR-0006](0006-validating-authpolicy.md)
- **Scope:** Repository structure & build tooling — `v1alpha1`
- **Informed by:** [research/kopiur.md](../research/kopiur.md), [research/cel-policy.md](../research/cel-policy.md)

## Context

AuthRoute began as a single binary crate (the ext_authz listener). The accepted
design now requires code that must be shared without dragging the async runtime
or Kubernetes client along:

- [ADR-0006](0006-validating-authpolicy.md) §2 mandates a **shared validation
  path** — the admission webhook and the request-time evaluator must compile and
  type-check CEL through the *same* code, so admission and runtime never disagree.
- [ADR-0006](0006-validating-authpolicy.md) §3 calls for a **separate `webhook`
  binary depending only on `api`**.
- [ADR-0002](0002-per-route-authorization-crd.md) defines a CRD whose YAML must
  match the Rust types exactly.

`CLAUDE.md` defers heavier tooling (mise, CI, release automation, docs site,
skills) until earned, and asks that any tooling adoption be recorded as an ADR.
The reference operator ([research/kopiur.md](../research/kopiur.md)) resolves the
same forces with a layered workspace plus an `xtask` codegen helper.

## Decision

**Adopt a layered Cargo workspace with a runtime-free `api` crate, and generate
`deploy/` manifests from the Rust types via an `xtask` binary.**

1. **Layout.**
   - `crates/api` — CRD types ([`AuthPolicy`], [`Subject`]) and the policy
     validators (`compile_policy`, `compile_path_regex`). **No `tokio`, no
     `kube::Client`**: it depends on `kube` with `default-features = false,
     features = ["derive"]` only, so both the webhook and the controller can
     reuse it. This crate is the shared validation path ADR-0006 §2 requires.
   - `crates/authroute` — the ext_authz / portal service binary (depends on `api`).
   - `crates/xtask` — build helper that writes `deploy/crds/` from the Rust types.
   - Future `controller` and `webhook` binaries join as `crates/*`, each
     depending on `api` (ADR-0006 §3).
2. **`k8s-openapi` version feature.** The version feature (`latest`) is enabled
   only by the final **binary** crates and `api`'s dev-dependency (for its own
   tests); the `api` library never enables it, per `k8s-openapi`'s library rule.
3. **Generated manifests, not hand-written.** `cargo run -p xtask -- codegen`
   serializes `AuthPolicy::crd()` to `deploy/crds/authroute.dev_authpolicies.yaml`
   (committed, regenerable, stable on re-run). RBAC generation is deferred to the
   controller milestone, when its watches are known.
4. **Invalid states unrepresentable, carried into the types.** Per
   [ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md), the target
   kind is a closed enum (`TargetRefKind::HttpRoute`), surfaced in the CRD schema
   as `enum: [HTTPRoute]`.

## Consequences

- The webhook (ADR-0006) and request-time evaluator can be built against `api`
  with a guarantee they share one parse/eval path; neither pulls in the other's
  dependencies.
- CRD YAML cannot drift from the Rust types — it is generated, and a schema
  snapshot test in `api` guards the resource identity.
- `xtask` is the first tooling beyond the binary; it stays minimal (one codegen
  command) consistent with `CLAUDE.md`'s deferred-tooling stance.

### Known deviation from ADR-0006 §1.ii — CEL static type-checking

ADR-0006 §1.ii requires CEL expressions to **type-check** against the activation
schema at admission. The chosen Rust crate (`cel` / `cel-rust` 0.13,
[research/cel-policy.md](../research/cel-policy.md)) ships a parser and
interpreter but **no standalone static type checker**. The `api` validator
therefore approximates type-checking: a policy is accepted only if it (a) parses
and (b) evaluates to a `bool` against a representative sample `Subject`
activation. This catches syntax errors, unknown-variable references, and obvious
type errors, but not type errors that surface only for specific data. The
guarantee ADR-0006 §2 depends on — admission and runtime never disagree — still
holds, because both call the identical `compile_policy` path. If full static
type-checking is later required, it warrants a follow-up ADR (e.g. integrating a
CEL type checker or `kube-cel`); ADR-0006 is not superseded.
