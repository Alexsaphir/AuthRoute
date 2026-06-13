# ADR-0000 — Record architecture decisions

- **Status:** Accepted
- **Date:** 2026-06-13
- **Supersedes:** —
- **Companion:** [ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md)
- **Scope:** Project-wide process

## Context

AuthRoute is a new project with several consequential, hard-to-reverse decisions
ahead of it: how requests are authorized at the edge, how identity is sourced,
how it couples to Envoy Gateway and the Gateway API. Six months from now we (and
anyone joining) will need to know *why* each path was taken, not just *what* the
code does. Reconstructing intent from a diff is expensive and lossy.

We want a lightweight, durable way to capture decisions at the moment they are
made, kept in the repository next to the code they govern. We are deliberately
modeling this practice on `home-operations/kopiur`, a Rust `kube-rs` operator of
the same shape, which uses Architecture Decision Records (ADRs) as its backbone.

## Decision

We will keep **Architecture Decision Records** in `docs/adr/`, one decision per
file, named `NNNN-kebab-case-title.md` with a zero-padded sequential number.

- Each ADR follows [`template.md`](template.md): a metadata block
  (`Status`, `Date`, `Supersedes`, `Companion`, `Scope`) followed by
  `## Context`, `## Decision`, `## Consequences`.
- **Status lifecycle:** `Proposed → Accepted → (Deprecated | Superseded by NNNN)`.
- ADRs are **immutable once Accepted.** We do not rewrite history when we change
  our minds — we write a *new* ADR that supersedes the old one, set the old one's
  status to `Superseded by NNNN`, and keep it in the tree as a record. Superseded
  and abandoned drafts stay; they explain how thinking evolved.
- ADRs are the **source of truth** for design intent. Other docs, code comments,
  and `CLAUDE.md` reference decisions by number and section (e.g. "ADR-0002 §3")
  rather than restating them.
- Record a decision as an ADR when it is significant and costly to reverse
  (framework, CRD surface, auth model, external integration, storage). Do **not**
  ADR routine choices (formatting, a helper, an obvious dependency).

To add one: copy `template.md`, take the next number, fill it in, open it as
`Proposed`, and move it to `Accepted` once agreed.

## Consequences

- Design intent is captured at decision time and survives turnover and refactors.
- A small, ongoing discipline: significant changes come with an ADR in the same
  change set, and accepted ADRs are never edited in place — only superseded.
- The `docs/adr/` directory grows monotonically, including dead drafts. That is
  intended; the history is the value.
- This ADR records only the *process*. The first design ADRs are seeded
  separately (see [ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md)).
