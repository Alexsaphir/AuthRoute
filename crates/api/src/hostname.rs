//! A normalized DNS hostname.

use std::fmt;

use serde::Deserialize;

/// A DNS hostname, normalized to lowercase.
///
/// Hostnames are case-insensitive (RFC 4343), so both the policy-table key and
/// the host of an incoming check are lowercased on construction. This makes
/// `X-Forwarded-Host: App.Example.com` match a policy keyed on
/// `app.example.com`, and lets the type be used directly as a `HashMap` key.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(from = "String")]
pub struct Hostname(String);

impl Hostname {
    /// Build a hostname, lowercasing it.
    pub fn new(raw: impl Into<String>) -> Self {
        let mut s = raw.into();
        s.make_ascii_lowercase();
        Self(s)
    }
}

impl From<String> for Hostname {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for Hostname {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl fmt::Display for Hostname {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case() {
        assert_eq!(
            Hostname::new("App.Example.COM"),
            Hostname::new("app.example.com")
        );
        assert_eq!(
            Hostname::new("App.Example.COM").to_string(),
            "app.example.com"
        );
    }

    #[test]
    fn deserializes_and_normalizes() {
        let h: Hostname = serde_yaml::from_str("App.Example.com").unwrap();
        assert_eq!(h, Hostname::new("app.example.com"));
    }
}
