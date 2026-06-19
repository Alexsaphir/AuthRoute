//! Runtime configuration, read from the environment at startup.

/// Service configuration. All fields have safe defaults so the binary runs with
/// no environment set (it then default-denies everything — ADR-0002 §D2).
#[derive(Clone, Debug)]
pub struct Config {
    /// TCP port the ext_authz listener binds (`AUTHROUTE_PORT`, default 8080).
    pub port: u16,
    /// Base URL of the auth portal that unauthenticated users are redirected to
    /// (`AUTHROUTE_PORTAL_URL`). The OIDC flow itself lands in M5.
    pub portal_url: String,
    /// Name of the session cookie to read (`AUTHROUTE_COOKIE_NAME`).
    pub cookie_name: String,
    /// Inline policy table as a YAML document (`AUTHROUTE_POLICY`). This is the
    /// M2 stub source; the controller (M3) replaces it. `None` => default-deny.
    pub policy_yaml: Option<String>,
}

impl Config {
    /// Read configuration from the environment, applying defaults.
    pub fn from_env() -> Self {
        Self {
            port: env("AUTHROUTE_PORT")
                .and_then(|v| v.parse().ok())
                .unwrap_or(8080),
            portal_url: env("AUTHROUTE_PORTAL_URL")
                .unwrap_or_else(|| "https://auth.example.com".to_string()),
            cookie_name: env("AUTHROUTE_COOKIE_NAME")
                .unwrap_or_else(|| "authroute_session".to_string()),
            policy_yaml: env("AUTHROUTE_POLICY"),
        }
    }
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}
