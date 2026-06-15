use serde::{Deserialize, Serialize};

/// Derived from OIDC claims via configurable mapping (M5). It is bound into the
/// CEL activation as `user`, `groups`, and `claims`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Subject {
    /// `Remote-User` — the canonical username.
    pub username: String,
    /// `Remote-Groups` — group memberships.
    pub groups: Vec<String>,
    /// `Remote-Name` — human-readable display name.
    pub name: String,
    /// `Remote-Email` — email address.
    pub email: String,
}
