//! The in-memory policy table the request path evaluates against.
//!
//! In M2 this is seeded from a stub YAML file ([`Config::policy_file`]). In M3
//! the controller will populate it by reconciling `HTTPRoute`s and
//! `AuthPolicy`s; the lookup contract here (resolve a host+path to the policy
//! expression that governs it, ADR-0002 §D5b) stays the same.
//!
//! [`Config::policy_file`]: crate::config::Config::policy_file

use std::collections::HashMap;
use std::path::Path;

use authroute_api::{CompiledPolicy, compile_path_regex, compile_policy};
use regex::Regex;
use serde::Deserialize;

/// All routes AuthRoute knows about, keyed by host.
#[derive(Default)]
pub struct PolicyTable {
    routes: HashMap<String, Route>,
}

/// The compiled policies governing one protected host. The host itself is the
/// `PolicyTable::routes` key.
struct Route {
    default_policy: CompiledPolicy,
    extra: Vec<(Regex, CompiledPolicy)>,
}

impl PolicyTable {
    /// An empty table: every request matches no route and is default-denied
    /// (ADR-0002 §D2).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Load and compile a stub policy file. Every expression and regex is
    /// compiled up front through the shared `authroute-api` path, so the hot
    /// path never parses (ADR-0006).
    pub fn from_file(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        Self::from_yaml(&std::fs::read_to_string(path)?)
    }

    /// Compile a policy table from a YAML string. The file-backed [`from_file`]
    /// delegates here; tests can build a table without touching the filesystem.
    ///
    /// [`from_file`]: Self::from_file
    pub fn from_yaml(raw: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let file: StubPolicyFile = serde_yaml::from_str(raw)?;
        let routes = file
            .routes
            .into_iter()
            .map(Route::compile)
            .collect::<Result<HashMap<_, _>, _>>()?;
        Ok(Self { routes })
    }

    /// Resolve the policy expression governing `host` + `path`: the first
    /// `extraPolicy` whose regex matches `path`, otherwise the host's
    /// `defaultPolicy`. `None` means no route matched (default-deny).
    pub fn resolve(&self, host: &str, path: &str) -> Option<&CompiledPolicy> {
        let route = self.routes.get(host)?;
        let matched = route
            .extra
            .iter()
            .find(|(re, _)| re.is_match(path))
            .map(|(_, policy)| policy);
        Some(matched.unwrap_or(&route.default_policy))
    }
}

impl Route {
    fn compile(raw: StubRoute) -> Result<(String, Self), Box<dyn std::error::Error>> {
        let extra = raw
            .extra_policy
            .into_iter()
            .map(|e| Ok((compile_path_regex(&e.path_regex)?, compile_policy(&e.policy)?)))
            .collect::<Result<Vec<_>, authroute_api::PolicyError>>()?;
        let route = Self {
            default_policy: compile_policy(&raw.default_policy)?,
            extra,
        };
        Ok((raw.host, route))
    }
}

/// Stub policy file schema. This is a temporary M2 input; the real source is the
/// `HTTPRoute` + `AuthPolicy` pair resolved by the controller (M3), so it does
/// not carry the CRD `targetRef` — only the host the route serves plus its CEL.
#[derive(Deserialize)]
struct StubPolicyFile {
    #[serde(default)]
    routes: Vec<StubRoute>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StubRoute {
    host: String,
    default_policy: String,
    #[serde(default)]
    extra_policy: Vec<StubExtra>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StubExtra {
    path_regex: String,
    policy: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use authroute_api::Subject;

    fn table() -> PolicyTable {
        let yaml = r#"
routes:
  - host: grafana.example.com
    defaultPolicy: '"admins" in groups'
    extraPolicy:
      - pathRegex: '^/public(/.*)?$'
        policy: 'true'
      - pathRegex: '^/api(/.*)?$'
        policy: 'user != ""'
"#;
        PolicyTable::from_yaml(yaml).unwrap()
    }

    fn admin() -> Subject {
        Subject {
            username: "a@example.com".into(),
            groups: vec!["admins".into()],
            ..Default::default()
        }
    }

    #[test]
    fn unknown_host_resolves_to_none() {
        assert!(table().resolve("other.example.com", "/").is_none());
    }

    #[test]
    fn default_policy_applies_without_match() {
        let t = table();
        let p = t.resolve("grafana.example.com", "/dashboards").unwrap();
        assert!(p.evaluate(&admin()).unwrap());
        assert!(!p.evaluate(&Subject::default()).unwrap());
    }

    #[test]
    fn first_matching_extra_policy_wins() {
        let t = table();
        // /public is open to everyone, even the anonymous Subject.
        let public = t.resolve("grafana.example.com", "/public/img.png").unwrap();
        assert!(public.evaluate(&Subject::default()).unwrap());
        // /api requires any authenticated user.
        let api = t.resolve("grafana.example.com", "/api/v1").unwrap();
        assert!(!api.evaluate(&Subject::default()).unwrap());
        assert!(api.evaluate(&admin()).unwrap());
    }
}
