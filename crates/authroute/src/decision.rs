//! The authorization decision: turn a check + session + policy table into an
//! allow / deny / redirect outcome (ADR-0002 §D5b, ADR-0004).

use authroute_api::Subject;

use crate::check::CheckRequest;
use crate::policy::PolicyTable;

/// The outcome of an ext_authz check.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// Allow the request. Carries the authenticated subject (if any) so its
    /// identity can be propagated downstream as `Remote-*` headers.
    Allow(Option<Subject>),
    /// Authenticated but not permitted — return `403` (ADR-0004).
    Forbidden,
    /// No (valid) session for a route that requires one — redirect to the auth
    /// portal with a return URL (ADR-0004). Carries the absolute `Location`.
    Redirect(String),
}

/// Decide the outcome for `req`, given the resolved `subject` (or `None` if the
/// request is anonymous) and the policy table.
///
/// Flow (ADR-0002 §D5b): resolve the route's policy expression; a request that
/// matches no route is default-denied (ADR-0002 §D2). Evaluate the expression
/// against the subject (anonymous subjects evaluate against an empty
/// [`Subject`]). `true` allows; `false` (or an evaluation error — fail closed)
/// denies, redirecting anonymous users to log in and returning `403` to
/// authenticated ones.
pub fn decide(
    table: &PolicyTable,
    portal_url: &str,
    req: &CheckRequest,
    subject: Option<Subject>,
) -> Decision {
    let Some(policy) = table.resolve(&req.host, req.path()) else {
        return deny(portal_url, req, subject.is_some());
    };

    let allowed = policy
        .evaluate(subject.as_ref().unwrap_or(&Subject::default()))
        .unwrap_or(false);

    if allowed {
        Decision::Allow(subject)
    } else {
        deny(portal_url, req, subject.is_some())
    }
}

/// A deny outcome: `403` if authenticated, otherwise redirect to the portal.
fn deny(portal_url: &str, req: &CheckRequest, authenticated: bool) -> Decision {
    if authenticated {
        Decision::Forbidden
    } else {
        Decision::Redirect(login_url(portal_url, &req.original_url()))
    }
}

/// Build the portal login URL carrying the original URL as the `rd` parameter.
fn login_url(portal_url: &str, return_to: &str) -> String {
    format!("{portal_url}?rd={}", percent_encode(return_to))
}

/// Minimal percent-encoding for a URL query value (RFC 3986 unreserved set kept
/// verbatim, everything else `%XX`). Avoids pulling in a URL crate for M2.
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(host: &str, uri: &str) -> CheckRequest {
        CheckRequest {
            proto: "https".into(),
            host: host.into(),
            uri: uri.into(),
            method: "GET".into(),
            cookie: None,
        }
    }

    fn admin() -> Subject {
        Subject {
            username: "admin@example.com".into(),
            groups: vec!["admins".into()],
            ..Default::default()
        }
    }

    fn user() -> Subject {
        Subject {
            username: "user@example.com".into(),
            groups: vec!["users".into()],
            ..Default::default()
        }
    }

    fn table() -> PolicyTable {
        let yaml = r#"
routes:
  - host: app.example.com
    defaultPolicy: '"admins" in groups'
    extraPolicy:
      - pathRegex: '^/public(/.*)?$'
        policy: 'true'
"#;
        PolicyTable::from_yaml(yaml).unwrap()
    }

    const PORTAL: &str = "https://auth.example.com";

    #[test]
    fn admin_allowed_with_subject() {
        let d = decide(
            &table(),
            PORTAL,
            &req("app.example.com", "/"),
            Some(admin()),
        );
        assert_eq!(d, Decision::Allow(Some(admin())));
    }

    #[test]
    fn authenticated_non_member_forbidden() {
        let d = decide(&table(), PORTAL, &req("app.example.com", "/"), Some(user()));
        assert_eq!(d, Decision::Forbidden);
    }

    #[test]
    fn anonymous_denied_redirects_to_portal() {
        let d = decide(&table(), PORTAL, &req("app.example.com", "/secret"), None);
        match d {
            Decision::Redirect(url) => {
                assert!(url.starts_with("https://auth.example.com?rd="));
                assert!(url.contains("https%3A%2F%2Fapp.example.com%2Fsecret"));
            }
            other => panic!("expected redirect, got {other:?}"),
        }
    }

    #[test]
    fn public_path_allows_anonymous() {
        let d = decide(
            &table(),
            PORTAL,
            &req("app.example.com", "/public/x?y=1"),
            None,
        );
        assert_eq!(d, Decision::Allow(None));
    }

    #[test]
    fn unknown_route_default_denies() {
        // Anonymous -> redirect; authenticated -> 403. Both are denials.
        assert!(matches!(
            decide(&table(), PORTAL, &req("nope.example.com", "/"), None),
            Decision::Redirect(_)
        ));
        assert_eq!(
            decide(
                &table(),
                PORTAL,
                &req("nope.example.com", "/"),
                Some(admin())
            ),
            Decision::Forbidden
        );
    }
}
