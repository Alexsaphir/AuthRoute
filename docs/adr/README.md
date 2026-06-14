# Architecture Decision Records

This directory holds AuthRoute's **Architecture Decision Records** — one
significant, hard-to-reverse decision per file. The practice itself is defined in
[ADR-0000](0000-record-architecture-decisions.md).

## What an ADR is here

A short, **immutable** record of one decision: its context, the choice, and the
consequences. Once an ADR is `Accepted` we don't edit it — if we change our mind
we write a new ADR that **supersedes** it and mark the old one accordingly. Dead
drafts stay in the tree; the history is the point.

## Status lifecycle

```
Proposed → Accepted → (Deprecated | Superseded by NNNN)
```

## Records

| #    | Title                                                                              | Status   |
| ---- | ---------------------------------------------------------------------------------- | -------- |
| 0000 | [Record architecture decisions](0000-record-architecture-decisions.md)             | Accepted |
| 0001 | [AuthRoute: a Kubernetes-native auth gateway on Envoy Gateway](0001-authroute-a-kubernetes-native-auth-gateway.md) | Accepted |
| 0002 | [Per-route authorization custom resource](0002-per-route-authorization-crd.md)     | Accepted |
| 0003 | [Identity via OIDC / OAuth](0003-identity-via-oidc-oauth.md)                        | Accepted |
| 0004 | [Envoy Gateway integration mechanism](0004-envoy-gateway-integration-mechanism.md) | Accepted |
| 0005 | [Session storage: server-side store over the Redis protocol](0005-session-storage.md) | Accepted |
| 0006 | [Validating AuthPolicy via an admission webhook](0006-validating-authpolicy.md)     | Accepted |

Supporting research notes live in [`../research/`](../research/).

## Adding an ADR

1. Copy [`template.md`](template.md).
2. Take the next sequential number; name it `NNNN-kebab-case-title.md`.
3. Fill in Context / Decision / Consequences; open it as `Proposed`.
4. Move it to `Accepted` once agreed, and add a row to the table above.
