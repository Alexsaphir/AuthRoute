# ADR-0005 — Session storage: server-side store over the Redis protocol

- **Status:** Accepted
- **Date:** 2026-06-13
- **Supersedes:** —
- **Companion:** [ADR-0003](0003-identity-via-oidc-oauth.md), [ADR-0004](0004-envoy-gateway-integration-mechanism.md)
- **Scope:** Session persistence — `v1alpha1`
- **Informed by:** [research/authelia.md](../research/authelia.md)

## Context

[ADR-0003](0003-identity-via-oidc-oauth.md) decided AuthRoute owns the OIDC flow
and issues a domain-wide SSO session, but left **how the session is stored** as an
open follow-up: a self-contained (stateless) signed/encrypted cookie, versus an
opaque cookie backed by a server-side store.

The constraints that decide this:

- AuthRoute is on the **request hot path** — every request to a protected route
  triggers an `extAuth` check ([ADR-0004](0004-envoy-gateway-integration-mechanism.md)),
  and it is **fail-closed**.
- As an OIDC relying party it holds **OIDC access/refresh tokens** we'd rather not
  put in the browser.
- An auth portal is expected to support **revocation / single-logout** ("log out
  everywhere now"), which a stateless cookie cannot do before its TTL expires.
- The cluster already runs **DragonflyDB**, a Redis-protocol-compatible store.

## Decision

AuthRoute uses a **server-side session store with an opaque session-ID cookie**.

1. **Opaque cookie.** The cross-subdomain cookie (ADR-0003) carries only a
   high-entropy random session ID — no identity or tokens. Attributes: `Secure`,
   `HttpOnly`, `SameSite=Lax` (needed so the OIDC redirect callback carries the
   cookie), `Domain` = the configured parent domain.
2. **Server-side state.** The full session — `Subject { username, groups[], name,
   email }`, OIDC tokens, issued-at/expiry — lives in the store keyed by the
   session ID. OIDC refresh happens server-side; tokens never reach the browser.
3. **Backend = the Redis protocol (RESP), vendor-neutral.** AuthRoute is written
   against Redis-compatible semantics (keyed values with native TTL), not a
   specific product. The default deployment targets the existing on-cluster
   **DragonflyDB**; Valkey/Redis are equally valid. Connection details (address,
   credential Secret, TLS, db index, key prefix) are Helm values.
4. **TTL-native expiry.** Each session is stored with a TTL equal to its lifetime;
   activity may refresh the TTL (sliding window). Expiry is the store's job.
5. **Hot-path mitigation.** AuthRoute keeps a small **in-process TTL cache** (a
   few seconds) of validated sessions so a burst of requests for one user doesn't
   each hit the store. A logout/revocation deletes the store key immediately;
   cached copies drain within the short cache TTL (best-effort, eventually
   consistent).
6. **Revocation / logout.** Deleting the session key kills the session on the next
   uncached request; logout also clears the cookie and optionally calls the OIDC
   end-session endpoint ([ADR-0003](0003-identity-via-oidc-oauth.md)).

## Consequences

- We get **strong revocation and single-logout**, OIDC tokens stay server-side,
  and the cookie stays tiny — the reasons for choosing a store over a stateless
  cookie.
- The store becomes a **required dependency on the request path**. Because
  enforcement is fail-closed, **if the store is unreachable, auth fails** for
  uncached sessions — so the store (DragonflyDB) must be run HA, and AuthRoute
  needs sane connect/read timeouts. The in-process cache softens brief blips for
  already-active sessions.
- AuthRoute replicas stay **stateless apart from the cache** and scale
  horizontally, all sharing the store. (Cache means revocation is not strictly
  instant across replicas — bounded by the cache TTL.)
- Operational surface grows: store address/credentials/TLS in Helm, plus
  monitoring. Backups are unnecessary — sessions are ephemeral; losing the store
  just forces users to re-authenticate.
- Targeting the **RESP protocol** (not a vendor API) keeps Redis/Valkey/Dragonfly
  interchangeable.
- **Open (tuning, not blocking):** in-process cache TTL, whether to use a sliding
  expiry window, and whether to add an integrity MAC to the opaque cookie ID.
