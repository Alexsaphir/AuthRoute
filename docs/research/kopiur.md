# kopiur (`home-operations/kopiur`)

Reviewed June 2026. Unlike the other three notes, this is about **how the project
is engineered and managed**, not its domain. kopiur is a Rust `kube-rs` operator
of the same shape AuthRoute will be, and is AuthRoute's reference for layout,
conventions, and the ADR practice (see [ADR-0000](../adr/0000-record-architecture-decisions.md)).

## What it is

A Kopia-native Kubernetes backup operator written in Rust on `kube-rs`. Makes
kopia repositories first-class Kubernetes resources via CRDs and reconcilers.

## Engineering patterns worth copying

### Layered Cargo workspace

A multi-crate workspace with a strict dependency direction:

- `crates/api` — CRD types + pure validators. **No controller/runtime deps**
  (no `tokio`, no `kube::Client`), so it's safe to reuse from both the webhook and
  the controller.
- specialized libs (e.g. their `kopia` subprocess client, a `telemetry` crate).
- binaries (`controller`, `webhook`, plus a job/CLI binary) that depend on `api`.
- `xtask` — a build helper binary that generates `deploy/crds` and `deploy/rbac`
  YAML from the Rust types (codegen, not hand-written manifests).
- `crates/e2e` — kind-based end-to-end harness, feature-gated and `#[ignore]`d.

### The load-bearing design principle

> Every "exactly one of" surface in the CRDs is a Rust `enum`, so an invalid
> state is unrepresentable and reconcilers `match` exhaustively. Prefer
> `enum` + exhaustive `match` over `if let` / `_ =>` catch-alls.

AuthRoute adopts this verbatim (it's the principle stated in
[ADR-0001](../adr/0001-authroute-a-kubernetes-native-auth-gateway.md)).

### ADRs as the backbone

`docs/adr/NNNN-title.md`, immutable once accepted, superseded rather than
rewritten, and **referenced by section from docs and `CLAUDE.md`** (e.g.
"ADR-0005 §1"). `CLAUDE.md` points the LLM at the ADRs as source of truth, states
the design principle, and pins the working style.

### LLM workflow via skills (not commands/hooks)

`.claude/skills/` holds domain-scoped skills — `kopiur-design` (CRD/reconciler
rules), `documentation`, `error-handling-and-e2e` — each with a "when to use" and
hard rules. No `settings.json`, custom commands, or hooks. (AuthRoute has
**deferred** this; see CLAUDE.md "Tooling status".)

### Error and test discipline

- `thiserror` enum per crate, one variant per failure mode; messages state what
  failed, why, and how to fix it. Exhaustive `class()` mapping, no `_ =>` arm.
- Degrade don't crash for non-critical subsystems; fail loudly on critical paths.
- Every feature ships tests at every tier it touches; every bug fix ships a
  regression test that fails without the fix.

### Tooling (deferred by AuthRoute for now)

`mise` as the single source of truth for tools + tasks; `lefthook` pre-commit;
`release-please` + conventional commits → automated CHANGELOG; `renovate`;
`cargo-deny` (license/supply-chain); `mkdocs` Material docs site with strict link
checking and example YAML included via snippets (never inline).

## Implications for AuthRoute

- Plan for a layered workspace early: a runtime-free `api` crate for CRD types +
  validators, reused by the controller and the forward-auth/portal service.
- Generate `deploy/` manifests from Rust types (an `xtask`), don't hand-write CRDs.
- Keep the "invalid states unrepresentable" discipline in the policy types.
- Adopt the tooling (mise, CI, release-please, mkdocs, skills) incrementally, each
  via its own ADR — matching the scope decision recorded in CLAUDE.md.
