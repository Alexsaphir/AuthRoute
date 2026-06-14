//! Session resolution: cookie -> [`Subject`].
//!
//! ADR-0005 specifies an opaque session-ID cookie backed by a server-side store
//! (Redis protocol) with a small in-process cache. That store lands in M4. For
//! M2 this is a stub: an in-memory map from cookie value to [`Subject`], so the
//! decision engine can be exercised end-to-end. The [`SessionStore::resolve`]
//! contract — given the `Cookie` header, return the authenticated subject or
//! `None` — is what the real store will implement.

use std::collections::HashMap;

use authroute_api::Subject;

/// Resolves session cookies to identities. Stubbed in M2 (see module docs).
#[derive(Default)]
pub struct SessionStore {
    sessions: HashMap<String, Subject>,
}

impl SessionStore {
    /// A stub store with two dev sessions, so allow / 403 / redirect paths can
    /// all be exercised without OIDC or a backing store:
    /// - `admin-token` -> a member of `admins`
    /// - `user-token`  -> an authenticated non-admin
    pub fn stub() -> Self {
        let mut sessions = HashMap::new();
        sessions.insert(
            "admin-token".to_string(),
            Subject {
                username: "admin@example.com".into(),
                groups: vec!["admins".into(), "users".into()],
                name: "Admin User".into(),
                email: "admin@example.com".into(),
            },
        );
        sessions.insert(
            "user-token".to_string(),
            Subject {
                username: "user@example.com".into(),
                groups: vec!["users".into()],
                name: "Regular User".into(),
                email: "user@example.com".into(),
            },
        );
        Self { sessions }
    }

    /// Resolve the authenticated [`Subject`] from a raw `Cookie` header value,
    /// reading the cookie named `cookie_name`. `None` = no valid session.
    pub fn resolve(&self, cookie_header: Option<&str>, cookie_name: &str) -> Option<Subject> {
        let value = session_cookie(cookie_header?, cookie_name)?;
        self.sessions.get(value).cloned()
    }
}

/// Extract the value of `name` from a `Cookie` header (`a=1; b=2`).
fn session_cookie<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header.split(';').find_map(|pair| {
        let (k, v) = pair.trim().split_once('=')?;
        (k == name).then_some(v)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_known_token() {
        let store = SessionStore::stub();
        let subject = store
            .resolve(Some("foo=bar; authroute_session=admin-token"), "authroute_session")
            .unwrap();
        assert_eq!(subject.username, "admin@example.com");
    }

    #[test]
    fn unknown_or_missing_cookie_is_anonymous() {
        let store = SessionStore::stub();
        assert!(store.resolve(Some("authroute_session=nope"), "authroute_session").is_none());
        assert!(store.resolve(Some("other=1"), "authroute_session").is_none());
        assert!(store.resolve(None, "authroute_session").is_none());
    }
}
