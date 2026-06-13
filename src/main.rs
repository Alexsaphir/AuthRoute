//! AuthRoute — Envoy Gateway forward-auth (ext_authz) service.
//!
//! First milestone: stand up the HTTP server that Envoy's `extAuth.http` hook
//! calls on every request to a protected route. For now it parses what Envoy
//! sends — the original target reconstructed from `X-Forwarded-*` headers, plus
//! the session cookie — logs it, and allows the request (`200`).
//!
//! The real decision logic (session lookup -> Subject, per-route policy match,
//! allow / 403 / redirect-to-login) comes next. See
//! `docs/research/ext-authz-contract.md` and ADR-0002/0003/0004.

use std::net::SocketAddr;

use axum::{
    Router,
    extract::OriginalUri,
    http::{HeaderMap, StatusCode},
    routing::{any, get},
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "authroute=debug,info".into()),
        )
        .init();

    let app = Router::new()
        .route("/healthz", get(healthz))
        // Envoy's ext_authz check can arrive on any path (it carries the real
        // target in X-Forwarded-* headers), so every other path is a check.
        .fallback(any(authz));

    let port: u16 = std::env::var("AUTHROUTE_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

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
///
/// Reconstructs the original request from the `X-Forwarded-*` headers Envoy
/// forwards (per `headersToExtAuth`) and notes whether a session cookie is
/// present. Currently allows everything; this is where policy evaluation lands.
async fn authz(OriginalUri(check_path): OriginalUri, headers: HeaderMap) -> StatusCode {
    let proto = header(&headers, "x-forwarded-proto");
    let host = header(&headers, "x-forwarded-host").or_else(|| header(&headers, "host"));
    let uri = header(&headers, "x-forwarded-uri");
    let method = header(&headers, "x-forwarded-method");
    let has_cookie = headers.contains_key("cookie");

    tracing::info!(
        proto = proto.unwrap_or("-"),
        host = host.unwrap_or("-"),
        uri = uri.unwrap_or("-"),
        method = method.unwrap_or("-"),
        cookie = has_cookie,
        check_path = %check_path,
        "ext_authz check (allow-all)"
    );

    // TODO(ADR-0002/0003): resolve session -> Subject, match the per-route
    // policy, and return 200 (+Remote-* headers) / 403 / redirect-to-login.
    StatusCode::OK
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}
