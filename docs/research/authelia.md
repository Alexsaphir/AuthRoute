# Authelia (`authelia/authelia`)

Reviewed June 2026. Focus: the access-control model, the session/cookie SSO
mechanism, and the forward-auth / ext_authz request contract — the behavior
AuthRoute reimplements in a Kubernetes-native way (ADR-0002, ADR-0003, ADR-0004).

AuthRoute is positioned as an alternative to Authelia: same enforcement model, but
**dynamically configured from Kubernetes resources** instead of a central config
file, and identity from **OIDC** instead of an LLDAP directory.

## Files read

- `internal/authorization/types.go`, `const.go` — the ACL data model.
- `internal/authorization/` (`access_control_rule.go`, `access_control_subjects.go`,
  `authorizer.go`, …) — rule matching.
- `internal/handlers/handler_authz_impl_extauthz.go`,
  `handler_authz_impl_forwardauth.go`, `handler_authz_impl_authrequest.go` — the
  three proxy-integration contracts.
- `internal/session/`, `internal/oidc/` — session and OIDC packages (surveyed).

## Key findings

### Access-control data model

- **`Subject`** = `{ Username, Groups []string, ClientID, IP }`. `IsAnonymous()`
  is true when username, groups, and clientID are all empty.
- **`Object`** = `{ URL, Domain, Path, Method }`, built from the forwarded request.
- A **rule** matches on domain / resources (path regex) / query / methods /
  networks / subjects, and yields a **`Level`**: `Bypass | OneFactor | TwoFactor`
  (`const.go`).
- Rules are evaluated in order; `RuleMatchResult` tracks which criteria matched.

This `Subject × Object → decision` shape is essentially what AuthRoute's per-route
CRD encodes — but Authelia keeps all rules in one central config file, which is
the static-configuration pain point AuthRoute is reacting to.

### Speaks every proxy's auth contract — including Envoy ext_authz

`internal/handlers/` has parallel implementations:

- `handler_authz_impl_extauthz.go` — **Envoy external authorization** (gRPC/HTTP
  ext_authz). Direct reference for AuthRoute's chosen integration (ADR-0004).
- `handler_authz_impl_forwardauth.go` — Traefik/Caddy forward-auth.
- `handler_authz_impl_authrequest.go` — nginx `auth_request`.

All share `handler_authz_common.go` / `handler_authz_builder.go`: build a
`Subject` + `Object` from forwarded request metadata, run the authorizer, return
allow/deny (or a redirect to the portal for unauthenticated browsers).

### Identity forwarding headers

On allow, Authelia injects `Remote-User`, `Remote-Groups`, `Remote-Name`,
`Remote-Email` for the upstream app. Inputs come from `X-Forwarded-Host`,
`X-Forwarded-Method`, etc. AuthRoute should adopt the same `Remote-*` names for
ecosystem compatibility (delivered via Envoy's `headersToBackend`).

### Session / cookie SSO across subdomains

`internal/session/` manages a session cookie scoped to a parent domain so a single
login is valid across all subdomains (the portal lives on e.g. `auth.example.com`;
the cookie covers `*.example.com`). Multiple cookie domains can be configured.
This is the model AuthRoute adopts (ADR-0003) — and the reason Envoy's per-route
built-in OIDC is unsuitable.

## Implications for AuthRoute

- **ADR-0002:** borrow the `Subject`/`Object`/rule semantics, but express rules as
  per-route Kubernetes resources rather than central config. (AuthRoute's auth
  "levels" will differ — likely public vs. require-auth-and-these-principals — not
  Authelia's 1FA/2FA ladder.)
- **ADR-0003:** adopt the domain-wide session-cookie SSO model and the `Remote-*`
  header convention; map OIDC claims → `Subject{Username, Groups, …}`.
- **ADR-0004:** `handler_authz_impl_extauthz.go` is the concrete reference for the
  ext_authz decision endpoint AuthRoute exposes.
