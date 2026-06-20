# ADR-0008 — Self-managed serving TLS for the admission webhook

- **Status:** Accepted
- **Date:** 2026-06-20
- **Supersedes:** —
- **Amends:** [ADR-0006](0006-validating-authpolicy.md) §3 (cert management only)
- **Companion:** [ADR-0006](0006-validating-authpolicy.md), [ADR-0007](0007-layered-workspace-and-manifest-codegen.md)
- **Scope:** Webhook serving TLS & `caBundle` lifecycle — `v1alpha1`

## Context

[ADR-0006](0006-validating-authpolicy.md) establishes a `ValidatingAdmissionWebhook`
served by a dedicated binary. A Kubernetes admission webhook must be served over
TLS, and the API server verifies the served certificate against the `caBundle`
published in the `ValidatingWebhookConfiguration`. ADR-0006 §3 stated, in
passing, that "its `ValidatingWebhookConfiguration`, serving cert, and Service
are Helm-managed" — i.e. the cert would come from external tooling
(cert-manager, or a Helm cert-generation hook) and the `caBundle` would be wired
in at install time.

That passing choice has costs the rest of the project deliberately avoids:

- **A hard dependency on cert-manager** (a cluster-wide component) for a
  single narrow webhook, or a Helm `lookup`/hook dance that is brittle and
  order-sensitive.
- `CLAUDE.md` defers heavy tooling until earned; pinning cert rotation to an
  external operator is exactly the kind of dependency to avoid at `v1alpha1`.

The webhook is **not on the request hot path** (ADR-0006 §4) and its scope is
one resource, so an ephemeral, self-signed serving identity is acceptable: there
is no client-trust requirement beyond the API server, which we control via the
`caBundle`. The workspace already carries `rcgen` for exactly this.

## Decision

**We will have the webhook binary mint its own self-signed serving certificate
at startup and publish the matching `caBundle` into its own
`ValidatingWebhookConfiguration`.** No external cert tooling is required.

1. **Mint on boot.** On startup the webhook generates a self-signed certificate
   (via `rcgen`) whose SANs are the in-cluster Service DNS names
   (`<name>.<namespace>.svc[.cluster.local]`). The certificate is marked as a CA
   so it is its own trust anchor.
2. **Self-publish the `caBundle`.** The webhook patches the `caBundle` of the
   `ValidatingWebhookConfiguration` named for it to the freshly minted CA, then
   serves. This needs `get`/`update` on `validatingwebhookconfigurations`
   (granted by the `xtask`-generated RBAC, ADR-0007).
3. **The configuration ships with an empty `caBundle`.** The generated manifest
   (`deploy/webhook/`) carries `failurePolicy: Fail`, `sideEffects: None`, and an
   empty `caBundle`; the running webhook fills it in. Best-effort: a missing
   configuration is logged, not fatal, so the binary stays runnable out-of-cluster.
4. **Ephemeral by design.** A restart mints a fresh cert and re-patches the
   `caBundle`. There is no cert store, no rotation timer, and no `Secret` to
   manage. With a single replica there is no cross-replica cert coordination.

This **amends ADR-0006 §3's cert-management clause only**. Everything else in
ADR-0006 stands: the webhook decision, the three checks (§1), the shared `api`
validation path (§2), `failurePolicy: Fail` (§4), and the controller's ownership
of status (§5).

## Consequences

- **No cert-manager dependency** and no Helm cert hooks. The webhook is
  self-contained: deploy the manifest, run the pod, it wires its own trust.
- **The webhook needs cluster write on `validatingwebhookconfigurations`** (its
  own object). A narrow, expected grant, but it is write access — noted in the
  generated RBAC.
- **Multiple replicas need rethinking.** Each replica would mint a different cert
  and the last to patch wins, so other replicas would fail TLS. At `v1alpha1` the
  webhook runs as a single replica; HA for the webhook (shared cert via a
  `Secret`, or a leader that owns the `caBundle`) is a future ADR if needed. This
  is distinct from the *controller's* HA (ADR-0004), which concerns the hot path.
- **Restart blips.** Between a restart minting a new cert and the `caBundle`
  patch landing, admission calls can fail TLS; with `failurePolicy: Fail` that
  briefly blocks `AuthPolicy` writes (never the hot path). Accepted for the
  authoring path.
- If a future requirement (true webhook HA, external trust) outgrows this, it
  warrants a follow-up ADR; ADR-0006 §3 is amended, not the whole webhook design.