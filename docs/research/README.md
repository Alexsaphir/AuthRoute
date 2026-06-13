# Research notes

Findings from reviewing prior art and reference implementations, captured to
inform AuthRoute's design. ADRs in [`../adr/`](../adr/) cite these notes.

Each file records what was actually read (with repository file paths) and what it
implies for AuthRoute — not a general summary of the project.

| Project | Notes | Why we looked |
| --- | --- | --- |
| Gateway API (`kubernetes-sigs/gateway-api`) | [gateway-api.md](gateway-api.md) | The policy-attachment pattern AuthRoute's per-route CRD should mirror (ADR-0002). |
| Envoy Gateway (`envoyproxy/gateway`) | [envoy-gateway.md](envoy-gateway.md) | The `SecurityPolicy` / `extAuth` / `oidc` mechanism AuthRoute plugs into (ADR-0003, ADR-0004). |
| Authelia (`authelia/authelia`) | [authelia.md](authelia.md) | The access-control semantics, session/cookie SSO model, and forward-auth contract AuthRoute reimplements K8s-natively. |
| kopiur (`home-operations/kopiur`) | [kopiur.md](kopiur.md) | The project-management / engineering template (ADR practice, layout, tooling) AuthRoute is modeled on. |
| Envoy HTTP `ext_authz` + Authelia handlers | [ext-authz-contract.md](ext-authz-contract.md) | The exact request/response contract AuthRoute's forward-auth decision endpoint implements. |

> Reviewed June 2026 against the then-current `main` of each repo. API fields and
> file paths may drift; treat these as a snapshot, re-check upstream before relying
> on a specific field.
