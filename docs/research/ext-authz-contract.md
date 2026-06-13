# The ext_authz (HTTP forward-auth) contract

Reviewed June 2026 against Envoy's HTTP `ext_authz` filter and Authelia's
implementation (`authelia/authelia`, `internal/handlers/handler_authz*.go`). This
is the spec AuthRoute's decision endpoint is coded against — the integration
chosen in [ADR-0004](../adr/0004-envoy-gateway-integration-mechanism.md).

## The round trip

Envoy calls the auth service on **every** request to a protected route. The auth
service returns **only a decision** — it never proxies the request body. Envoy
forwards the original request upstream (or not) based on the response.

```
browser ──req──▶ Envoy ──"check" (subset of headers)──▶ AuthRoute /authz
                   ◀── 200 + Remote-* headers ──────────┘  (allow)
        ◀─ upstream response ─ Envoy ──req + Remote-*──▶ app

                   ◀── 302 Location: auth portal ───────┘  (deny → login)
        ◀─ 302 relayed to browser ─┘
```

## 1. Request: what Envoy sends to the auth service

- A request to the configured **`path`** (Authelia uses `/api/authz/ext-authz/`).
- Only the headers in **`headersToExtAuth`** (set on the `SecurityPolicy`):
  notably `cookie` (the session), `x-forwarded-proto`, `x-forwarded-for`, plus the
  forwarded method/host/URI. Envoy injects the `X-Forwarded-*` set.

The original target is reconstructed from forwarded headers
(`handler_authz_impl_extauthz.go`, `handler_authz_common.go`):

```
Object{ proto, host, uri, method }
  = X-Forwarded-Proto + "://" + Host + X-Forwarded-URI ; X-Forwarded-Method
```

Authelia **rejects a non-https target** (so the session cookie is only ever
transmitted securely) → `400`.

## 2. Decision logic (`handler_authz.go::Handler`)

1. Build the `Object` (host/path/method) from forwarded headers; require https.
2. Read the **session cookie**, resolve it to a `Subject { username, groups[] }`.
   *(AuthRoute: look the opaque session ID up in the store — [ADR-0005](../adr/0005-session-storage.md).)*
3. Determine the **required level** for `(Subject, Object)`. **This is the only
   place AuthRoute diverges from Authelia:** Authelia matches a central ACL;
   AuthRoute matches the request host/path against its runtime CRD table
   ([ADR-0002](../adr/0002-per-route-authorization-crd.md) D5b) to find the
   `RouteAuthPolicy` and its `rule` (`public` / `authenticated` /
   `restricted{allow}`), falling back to the `GatewayAuthPolicy` default or
   default-deny.
4. Compare the authenticated level against the required level → Authorized /
   Unauthorized / Forbidden.

An auth error must **never** yield an authenticated user — Authelia resets the
subject to anonymous / `NotAuthenticated` on any strategy error before deciding.

## 3. Response contract (the whole integration)

| Outcome | Auth service returns | What Envoy does |
| --- | --- | --- |
| **Authorized** | `200` + `Remote-User`, `Remote-Groups` (comma-joined), `Remote-Name`, `Remote-Email` | Adds those (the `headersToBackend` allow-list) to the request, forwards upstream |
| **Forbidden** (authenticated, not allowed) | `403` | Relays `403` to the client |
| **Unauthorized** (no/invalid session, auth required) | `302`/`303` redirect to portal, **or** `401` | Relays it to the client → user lands on login |
| **Bad request** (missing/insecure target) | `400` | Relays `400` |

**Load-bearing fact:** on any **non-2xx**, Envoy returns the auth server's response
— status, headers (incl. `Location`), and body — directly to the downstream
client. This is what lets the redirect-to-login work without the auth service
being in the response path. On `200`, only the headers named in `headersToBackend`
are copied onto the upstream request.

Identity headers are set from the user details on allow
(`handler_authz_common.go::handleAuthzAuthorizedStandard`): username, groups
joined with `,`, display name, and the first email (empty string when none).

## 4. Redirect nuance (`handleAuthzUnauthorizedExtAuthz`)

When unauthenticated and auth is required, the status code is chosen by request
shape:

- **XHR, or the client does not accept `text/html`** → `401` (never redirect an
  API/`fetch` call).
- Browser **GET / HEAD / OPTIONS** → `302 Found`.
- Browser other methods (POST, …) → `303 See Other` (so the post-login replay
  degrades to a safe GET).
- `HEAD` → redirect with no body.

The redirect URL is `portal + ?rd=<original-URL>&rm=<method>`
(`handler_authz.go::getRedirectionURL`). After the OIDC flow completes and the SSO
cookie is set, the portal uses `rd` to send the user back.

## 5. failOpen / fail-closed

If the auth service is unreachable, Envoy applies `failOpen`
(Envoy's `failure_mode_allow`). AuthRoute runs with **`failOpen: false`** → deny on
outage ([ADR-0004](../adr/0004-envoy-gateway-integration-mechanism.md)). Hence
AuthRoute *and* its session store must be highly available.

## What AuthRoute must expose

- **The decision endpoint** (e.g. `/api/authz/ext-authz/` or `/authz`) — the
  per-request check above: stateless logic plus one session lookup. Backed by the
  gateway-wide `SecurityPolicy` the Helm chart ships (ADR-0002 D5).
- **The portal** (separate, user-facing, on e.g. `auth.example.com`): `GET /`
  (login → OIDC redirect), the **OIDC callback**, `GET /logout`. These set/clear
  the SSO cookie ([ADR-0003](../adr/0003-identity-via-oidc-oauth.md),
  [ADR-0005](../adr/0005-session-storage.md)). Envoy only routes to it; it is not
  in the ext_authz path.

## Reference

Authelia's `handler_authz.go` (core flow), `handler_authz_common.go` (identity
headers, URL reconstruction), and `handler_authz_impl_extauthz.go` (status-code
selection) are the concrete model to mirror. AuthRoute reuses the request/response
contract verbatim and replaces only the policy source (step 2.3).
