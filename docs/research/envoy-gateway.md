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
- `HTTPExtAuthService.headersToBackend []string` — response headers from the
  ext-auth service that Envoy adds to the request before forwarding upstream.
  (This is how identity headers like `Remote-User` reach the app.)

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
- Open (ADR-0002/0004): does AuthRoute *generate* the `SecurityPolicy` that wires
  `extAuth` to a route, or do users author it and reference AuthRoute's Service?
