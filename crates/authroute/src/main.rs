//! AuthRoute — Envoy Gateway forward-auth (ext_authz) service.
//!
//! Envoy's `extAuth.http` hook calls this service on every request to a
//! protected route. It reconstructs the original request from the
//! `X-Forwarded-*` headers, resolves the session cookie to a [`Subject`],
//! evaluates the per-route CEL policy, and answers allow (`200` + `Remote-*`
//! headers), deny (`403`), or redirect-to-login (`302`). See
//! `docs/research/ext-authz-contract.md` and ADR-0002/0003/0004.
//!
//! Session resolution and the policy table are stubbed in M2 (in-memory store,
//! policy file); the controller (M3) and Redis store (M4) replace those seams
//! without changing the decision logic.

mod check;
mod config;
mod decision;
mod policy;
mod session;

use std::net::SocketAddr;
use std::sync::Arc;

use authroute_api::Subject;
use axum::{
    extract::State,
    http::{header::LOCATION, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, get},
    Router,
};

use crate::check::CheckRequest;
use crate::config::Config;
use crate::decision::{decide, Decision};
use crate::policy::PolicyTable;
use crate::session::SessionStore;

/// Shared, read-only state handed to every request.
struct AppState {
    config: Config,
    table: PolicyTable,
    sessions: SessionStore,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "authroute=debug,info".into()),
        )
        .init();

    let config = Config::from_env();

    let table = match &config.policy_yaml {
        Some(raw) => match PolicyTable::from_yaml(raw) {
            Ok(table) => {
                tracing::info!("loaded policy from AUTHROUTE_POLICY");
                table
            }
            // A bad policy config is a fatal misconfiguration: fail closed loudly
            // rather than silently default-denying every route.
            Err(e) => {
                tracing::error!(error = %e, "failed to parse AUTHROUTE_POLICY");
                std::process::exit(1);
            }
        },
        None => {
            tracing::warn!("no AUTHROUTE_POLICY set; all routes default-deny");
            PolicyTable::empty()
        }
    };

    let state = Arc::new(AppState {
        table,
        sessions: SessionStore::stub(),
        config,
    });

    let app = Router::new()
        .route("/healthz", get(healthz))
        // Envoy calls the ext_authz HTTP service at the configured `path`
        // (`/api/authz/ext-authz/`, matching Authelia) and appends the original
        // request path to it — so we register the base path plus a catch-all for
        // the appended segments, on any method. The authoritative target is
        // always read from the X-Forwarded-* headers, not from this path.
        .route("/api/authz/ext-authz", any(authz))
        .route("/api/authz/ext-authz/", any(authz))
        .route("/api/authz/ext-authz/{*rest}", any(authz))
        .with_state(state.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], state.config.port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind ext_authz listener");
    tracing::info!(%addr, "AuthRoute ext_authz listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

/// Liveness/readiness probe target.
async fn healthz() -> &'static str {
    "ok"
}

/// The ext_authz decision endpoint.
async fn authz(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let req = CheckRequest::from_headers(&headers);
    let subject = state
        .sessions
        .resolve(req.cookie.as_deref(), &state.config.cookie_name);

    let decision = decide(&state.table, &state.config.portal_url, &req, subject);

    tracing::info!(
        host = %req.host,
        path = %req.path(),
        method = %req.method,
        outcome = decision_label(&decision),
        "ext_authz check"
    );

    match decision {
        Decision::Allow(subject) => allow_response(subject),
        Decision::Forbidden => StatusCode::FORBIDDEN.into_response(),
        Decision::Redirect(location) => redirect_response(&location),
    }
}

/// `200` with `Remote-*` identity headers for an authenticated subject
/// (ADR-0003); a bare `200` for an allowed anonymous (public) request.
fn allow_response(subject: Option<Subject>) -> Response {
    let mut resp = StatusCode::OK.into_response();
    if let Some(subject) = subject {
        let headers = resp.headers_mut();
        set_header(headers, "Remote-User", &subject.username);
        set_header(headers, "Remote-Groups", &subject.groups.join(","));
        set_header(headers, "Remote-Name", &subject.name);
        set_header(headers, "Remote-Email", &subject.email);
    }
    resp
}

/// `302` to the auth portal with the original URL as the return target.
fn redirect_response(location: &str) -> Response {
    let mut resp = StatusCode::FOUND.into_response();
    if let Ok(value) = HeaderValue::from_str(location) {
        resp.headers_mut().insert(LOCATION, value);
    }
    resp
}

/// Set a header, silently skipping values that aren't valid header content.
fn set_header(headers: &mut HeaderMap, name: &'static str, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        headers.insert(name, value);
    }
}

fn decision_label(decision: &Decision) -> &'static str {
    match decision {
        Decision::Allow(_) => "allow",
        Decision::Forbidden => "forbidden",
        Decision::Redirect(_) => "redirect",
    }
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}
