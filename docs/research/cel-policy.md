# CEL for policy expressions

Reviewed June 2026. A technique study (not a project review): can **CEL** (Google's
Common Expression Language) express AuthRoute authorization policy — "allow this
user", "allow this group" — and how does it relate to ADR-0002's typed allow-list?

## What CEL is

A small, **non-Turing-complete** expression language: every expression terminates,
is side-effect free, and can be statically type-checked against a declared
environment before it ever runs. C-like syntax. Designed for exactly this job —
embedding user-authored predicates into a host that decides allow/deny. An
expression evaluates against an **activation** (a set of named variables the host
binds) and returns a value (for policy: a `bool`).

## Ecosystem precedent (why it fits an auth gateway)

CEL is already the de-facto policy language across the stack AuthRoute lives in:

- **Kubernetes** — `ValidatingAdmissionPolicy`, CRD validation rules
  (`x-kubernetes-validations`), and the authorizer library. Identity checks read
  `request.userInfo.username` and `request.userInfo.groups`, e.g.
  `"admin" in request.userInfo.groups`. This is the exact user/group shape
  AuthRoute has (ADR-0003 `Subject{username, groups[]}`).
- **Envoy** — the **RBAC filter** takes an optional CEL `condition` alongside its
  typed principals/permissions; an extra clause that must hold for a policy to
  match.
- **Kyverno `AuthorizationPolicy`** — analyzes an incoming Envoy **ext_authz
  `CheckRequest`** with CEL and emits the `CheckResponse`. This is the closest
  prior art to AuthRoute: CEL driving an ext_authz decision.

## The Rust crate

- Crate: **`cel`** `0.13.0` (renamed from `cel-interpreter`; actively maintained
  through 2026, `cel-rust/cel-rust`). Pure-Rust parser + interpreter, no CGo/cgo
  dependency on Google's Go/C++ impl.
- Shape of the API:
  ```rust
  use cel::{Context, Program};

  let program = Program::compile(r#""admins" in groups || user == "alice""#)?;

  let mut ctx = Context::default();
  ctx.add_variable("user", subject.username.clone());
  ctx.add_variable("groups", subject.groups.clone()); // Vec<String> -> CEL list
  let allowed: bool = program.execute(&ctx)?.try_into()?;
  ```
- Types map cleanly to `Subject`: `String` ↔ CEL string, `Vec<String>` ↔ CEL list,
  `HashMap` ↔ CEL map (useful for future raw OIDC claims). `Program` compiles once
  and is reused per request; custom host functions register via
  `ctx.add_function(...)`.

## Expressing AuthRoute policy

Bind the session `Subject` into the activation as `user` and `groups`. Then:

| Intent | CEL |
| --- | --- |
| allow a user | `user == "alice@example.com"` |
| allow a group | `"admins" in groups` |
| any-of (OR) — today's allow-list | `"admins" in groups \|\| user == "alice@example.com"` |
| **all-of (AND)** | `user == "bob" && "admins" in groups` |
| any authenticated | `user != ""` |
| future: claim-based | `claims.email_verified == true` |

The AND case is exactly the expressiveness AuthRoute **loses** with the flat OR
allow-list (see [authelia.md](authelia.md) — Authelia's `[][]string` AND/OR). CEL
regains it, plus claim predicates, for free.

## Tension with ADR-0001

A typed, exhaustive surface (`rule` enum + `{group}|{user}` matchers) would keep
invalid states unrepresentable (ADR-0001). CEL is the opposite: a **stringly-typed
escape hatch**. A typo or type error (`groups == "admin"` comparing a list to a
string) is representable and, naively, only fails at **request time** — the worst
time for an auth gateway.

Mitigation, and the reason this is still viable: CEL **type-checks statically**.
AuthRoute can compile + type-check the expression against the known activation
schema (`user: string`, `groups: list<string>`, later `claims: map`) **at admission
/ reconcile time** and report a `Accepted=False` / `ResolvedRefs` condition on the
policy `.status` (the same place ADR-0002 §D6–D7 already surface conflicts). That
moves the failure back to author time, restoring most of the "invalid states are
caught early" guarantee — though not at the Rust *type* level.

## Implication for AuthRoute

**Decided in [ADR-0002](../adr/0002-per-route-authorization-crd.md) §D3–D4**: policy
is a **CEL expression**, not a typed `rule` enum. The spec is `defaultPolicy: '<cel>'`
plus an ordered `extraPolicy: [{ pathRegex, policy: '<cel>' }]`; expressions evaluate
against `user`/`groups` (and reserved `claims`) and must compile + type-check at
admission time, with errors on `.status`. The points below recorded the reasoning
that led there:

- The typed matchers couldn't express AND / claim-based policy; CEL can, and is the
  ecosystem-standard idiom (`"admins" in groups`).
- The ADR-0001 "invalid states unrepresentable" guarantee is preserved at **author
  time** via CEL static type-checking at admission, not at the Rust type level —
  the deliberate trade.
- `cel` crate lives in the runtime-free `api` crate so reconciler-validation and
  request-time evaluation share one parse/type-check/eval path.
- Out of scope (unchanged): method/network selectors remain unsupported (see
  [authelia.md](authelia.md)).