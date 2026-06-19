//! The incoming ext_authz check, reconstructed from Envoy's forwarded headers.
//!
//! Envoy forwards the original request context as `X-Forwarded-*` headers (per
//! `extAuth.headersToExtAuth`); the authoritative target is always read from
//! these, never from the path Envoy calls us on. See
//! `docs/research/ext-authz-contract.md` and ADR-0004.

use axum::http::HeaderMap;

use authroute_api::hostname::Hostname;

/// The original request being authorized.
#[derive(Clone, Debug)]
pub struct CheckRequest {
    /// Original scheme (`https` unless told otherwise).
    pub proto: String,
    /// Original host (`X-Forwarded-Host`, falling back to `Host`).
    pub host: Hostname,
    /// Original request target including any query string.
    pub uri: String,
    /// HTTP method of the original request.
    pub method: String,
    /// Raw `Cookie` header value, if present.
    pub cookie: Option<String>,
}

impl CheckRequest {
    /// Reconstruct the check from request headers.
    pub fn from_headers(headers: &HeaderMap) -> Self {
        Self {
            proto: header(headers, "x-forwarded-proto")
                .unwrap_or("https")
                .to_string(),
            host: Hostname::new(
                header(headers, "x-forwarded-host")
                    .or_else(|| header(headers, "host"))
                    .unwrap_or_default(),
            ),
            uri: header(headers, "x-forwarded-uri")
                .unwrap_or("/")
                .to_string(),
            method: header(headers, "x-forwarded-method")
                .unwrap_or("GET")
                .to_string(),
            cookie: header(headers, "cookie").map(str::to_string),
        }
    }

    /// The request path, without any query string — what `pathRegex` matches.
    pub fn path(&self) -> &str {
        self.uri.split('?').next().unwrap_or(&self.uri)
    }

    /// The full original URL, used as the return target on a login redirect.
    pub fn original_url(&self) -> String {
        format!("{}://{}{}", self.proto, self.host, self.uri)
    }
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}
