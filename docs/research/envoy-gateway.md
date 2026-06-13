# Envoy Gateway (`envoyproxy/gateway`)

Reviewed June 2026. Focus: the `SecurityPolicy` CRD and its `extAuth` / `oidc` /
`authorization` sub-resources — the mechanism AuthRoute plugs into (ADR-0003, ADR-0004).

## Files read

- `api/v1alpha1/securitypolicy_types.go`
- `api/v1alpha1/ext_auth_types.go`
- `api/v1alpha1/oidc_types.go`
- `api/v1alpha1/authorization_types.go`

## Key findings

### `SecurityPolicy` bundles every auth feature, attached via targetRef

`SecurityPolicySpec` inlines `PolicyTargetReferences` and is constrained by CEL to
target `Gateway / HTTPRoute / GRPCRoute / TCPRoute` in group
`gateway.networking.k8s.io` (`targetRef` xor `targetRefs`, or `targetSelectors`).
The spec carries, as optional blocks:

- `basicAuth`, `jwt`, `oidc`, `extAuth`, `authorization`.

So a single Envoy Gateway resource can already do OIDC login, JWT validation,
external authz, and rule-based allow/deny — without any external service.

### `oidc` — full login flow built into Envoy

`oidc_types.go`: `OIDC` has a `provider` (`OIDCProvider.issuer`, with auto-discovery
of `authorizationEndpoint` / `tokenEndpoint` / `endSessionEndpoint` from the
well-known config), `clientID` / `clientIDRef`, `clientSecret` (Secret ref),
`scopes`, `redirectURL`, `logoutPath`, and `cookieNames`. **Envoy itself runs the
OIDC dance and manages the cookie.**

> Crucial limitation for AuthRoute: this is configured **per `SecurityPolicy` /
> per target**. It does not give a single shared session across many subdomains.
> That is exactly why AuthRoute owns OIDC itself instead of delegating here
> (see ADR-0003).

### `extAuth` — external authorization (the forward-auth hook)

`ext_auth_types.go`: `ExtAuth` selects either:

- `grpc` → `GRPCExtAuthService`
- `http` → `HTTPExtAuthService`

both pointing at `backendRefs` (Service / ServiceImport / Backend). Notable fields:

- `failOpen *bool` — if true, allow traffic when the ext-auth service can't be
  reached. Default behavior is fail-closed.
- `headersToExtAuth []string` (on `ExtAuth`) — the **request** headers Envoy
  forwards *to* the auth service. Must include `cookie` so the auth service can
  read the session cookie, plus `x-forwarded-*` so it knows the original
  host/proto/client.
- `HTTPExtAuthService.path` — the path on the auth service that receives the
  check (e.g. Authelia's `/api/authz/ext-authz/`).
- `HTTPExtAuthService.headersToBackend []string` — **response** headers from the
  ext-auth service that Envoy adds to the request before forwarding upstream.
  (This is how identity headers like `Remote-User` reach the app.)

### Real-world wiring: it's typically **gateway-wide**, not per-route

A working Authelia `SecurityPolicy` (provided by the AuthRoute author) targets the
**`Gateway`**, not individual `HTTPRoute`s:

```yaml
spec:
  targetRefs:
    - group: gateway.networking.k8s.io
      kind: Gateway
      name: envoy
  extAuth:
    failOpen: false
    headersToExtAuth: [accept, cookie, location, authorization,
      proxy-authorization, header-authorization, x-forwarded-proto, x-forwarded-for]
    http:
      backendRefs: [{ kind: Service, name: authelia, namespace: kube-auth, port: 80 }]
      path: /api/authz/ext-authz/
      headersToBackend: [Remote-User, Remote-Groups, Remote-Name, Remote-Email]
```

So **one** `SecurityPolicy` sends *all* of the gateway's traffic to the auth
service, and the auth service decides per request from the forwarded
host/path/cookie. Per-route differentiation lives **inside** the auth service,
not in many per-route `SecurityPolicy` resources. This directly shapes AuthRoute's
enforcement model (ADR-0002 D5).

### `authorization` — a built-in rules engine

`authorization_types.go`: `Authorization` has ordered `rules[]` plus a
`defaultAction`. **If neither rules nor defaultAction is set, the default is deny
all.** Each `AuthorizationRule` has an `action: Allow | Deny` and a `Principal`:

- `Principal.clientCIDRs []CIDR`
- `Principal.jwt` (`JWTPrincipal`) — match on `claims[]` (`JWTClaim` with
  name/value/valueType String|StringArray) and `scopes[]`. Requires a `jwt` block
  in the same policy.

This is a static, JWT/CIDR-based matcher. It's *less* expressive than what
AuthRoute wants (dynamic group lookups, session-derived identity), which is one
reason AuthRoute runs as `extAuth` rather than compiling into this engine.

## Implications for AuthRoute (ADR-0004)

- AuthRoute integrates as the **`extAuth.http`** backend of a `SecurityPolicy`
  (forward-auth style), returning allow/deny per request and emitting identity via
  `headersToBackend`. It does **not** use the built-in `oidc` block (per-route, no
  cross-subdomain SSO) and does **not** compile into the `authorization` engine.
- `failOpen` defaults matter: AuthRoute is a security control, so **fail-closed**
  is the intended default — accept the availability cost.
- The `SecurityPolicy` wiring is **gateway-wide** (one resource per Gateway), so
  AuthRoute must match each request to the right route policy *at request time*
  (from forwarded host/path), rather than relying on per-route `SecurityPolicy`
  resources. Resolved (ADR-0002 D5): AuthRoute's **Helm chart** ships that one
  gateway-wide `SecurityPolicy` with `targetRefs` as a chart value — not the
  controller (no runtime RBAC over the Envoy CRD).
