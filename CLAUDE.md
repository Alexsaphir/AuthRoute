# AuthRoute

AuthRoute is a **Kubernetes-native alternative to Authelia**: a Rust `kube-rs`
operator that provides dynamic, per-route authentication and authorization for
traffic served by **Envoy Gateway**. Each protected route carries a custom
resource declaring whether it needs auth and which groups/users are allowed;
identity comes from an external **OIDC/OAuth** provider (no LLDAP). Configuration
is reconciled from Kubernetes resources — there is no central config file.

## Architecture decisions are the source of truth

- **Read [`docs/adr/`](docs/adr/) before making design changes.** Decisions about
  the product shape, CRD surface, identity model, and Envoy Gateway integration
  live there. Start with [ADR-0001](docs/adr/0001-authroute-a-kubernetes-native-auth-gateway.md).
- Record any new significant, hard-to-reverse decision as an ADR in the same
  change set. See [ADR-0000](docs/adr/0000-record-architecture-decisions.md) for
  the process.
- **Do not rewrite an `Accepted` ADR** — supersede it with a new one and update
  the old one's status. Reference decisions by number/section (e.g. "ADR-0002 §3")
  instead of restating them.
- Preserve the load-bearing principle from ADR-0001: AuthRoute reconciles
  Kubernetes resources into authorization behavior, and **policy is modeled so
  invalid/ambiguous states are hard to express** — prefer typed, exhaustive Rust
  enums over loosely-coupled optional fields.

## Working style

- **Phased.** Land one milestone, verify it (`cargo test` and `clippy` green),
  then start the next. Don't claim a milestone done without showing passing output.
- **Commit/push only when asked.** Work on a feature branch.

## Tooling status

Heavier project tooling (mise tasks, CI workflows, release automation, a docs
site, Claude skills) is **intentionally deferred** until the project earns it.
When we adopt any of it, that adoption gets its own ADR.
