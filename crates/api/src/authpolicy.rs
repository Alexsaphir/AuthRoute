//! The `AuthPolicy` custom resource and the session [`Subject`] (ADR-0002, ADR-0003).

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `AuthPolicy` — per-route authorization, attached to an `HTTPRoute` (ADR-0002).
///
/// Authorization is expressed as CEL: [`AuthPolicySpec::default_policy`] applies
/// to the whole target, with optional ordered [`ExtraPolicy`] overrides matched
/// by path (first match wins). A route with no `AuthPolicy` is denied
/// (default-deny, ADR-0002 §D2).
#[derive(CustomResource, Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "authroute.dev",
    version = "v1alpha1",
    kind = "AuthPolicy",
    namespaced,
    status = "AuthPolicyStatus",
    shortname = "authpol",
    doc = "Per-route authentication and authorization for Envoy Gateway routes.",
    printcolumn = r#"{"name":"Target","type":"string","jsonPath":".spec.targetRef.name"}"#,
    printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct AuthPolicySpec {
    /// The `HTTPRoute` this policy protects. At most one `AuthPolicy` may target
    /// a given route (ADR-0002 §D6).
    pub target_ref: TargetRef,

    /// CEL boolean expression evaluated against the session [`Subject`]; `true`
    /// allows, `false` denies. Applies to the whole target unless an
    /// [`ExtraPolicy`] matches first (ADR-0002 §D3–D4).
    pub default_policy: String,

    /// Ordered path overrides; the first whose `pathRegex` matches the request
    /// path supplies the policy, otherwise [`Self::default_policy`] is used.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_policy: Vec<ExtraPolicy>,
}

/// A path-scoped policy override within an [`AuthPolicySpec`].
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtraPolicy {
    /// Regex matched against the request path; the same `regex` engine is used
    /// at admission and request time (ADR-0006).
    pub path_regex: String,
    /// CEL boolean expression applied when `pathRegex` matches.
    pub policy: String,
}

/// Reference to the routing resource an [`AuthPolicy`] attaches to.
///
/// Models the Gateway API `LocalPolicyTargetReference` shape, but the kind is a
/// closed enum so an unsupported target is unrepresentable (ADR-0001).
/// Same-namespace only (ADR-0002 §D7).
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TargetRef {
    /// API group of the target. Only `gateway.networking.k8s.io` is meaningful.
    #[serde(default = "TargetRef::default_group")]
    pub group: String,
    /// Kind of the target. Constrained to the kinds AuthRoute can protect.
    pub kind: TargetRefKind,
    /// Name of the target resource in the policy's namespace.
    pub name: String,
}

impl TargetRef {
    fn default_group() -> String {
        "gateway.networking.k8s.io".to_string()
    }
}

/// The kinds an [`AuthPolicy`] may target. Closed so invalid targets cannot be
/// expressed (ADR-0001); v1alpha1 supports `HTTPRoute` only (ADR-0002 §D1).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
pub enum TargetRefKind {
    #[serde(rename = "HTTPRoute")]
    HttpRoute,
}

/// Status reported by the controller via Gateway API `PolicyStatus`
/// conditions (ADR-0002 §D6, ADR-0006). Populated in M3.
#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthPolicyStatus {
    /// Standard Kubernetes conditions (`Accepted`, `ResolvedRefs`, conflict).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<Condition>,
}

/// A single status condition (a trimmed `metav1.Condition`).
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    pub type_: String,
    pub status: String,
    pub reason: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
}

/// The authenticated identity a policy is evaluated against and that is
/// propagated downstream as `Remote-*` headers (ADR-0003).
///
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

#[cfg(test)]
mod tests {
    use super::*;
    use kube::CustomResourceExt;

    #[test]
    fn spec_round_trips_with_camel_case() {
        let json = serde_json::json!({
            "targetRef": {
                "group": "gateway.networking.k8s.io",
                "kind": "HTTPRoute",
                "name": "grafana"
            },
            "defaultPolicy": "\"admins\" in groups",
            "extraPolicy": [
                { "pathRegex": "^/public(/.*)?$", "policy": "true" }
            ]
        });
        let spec: AuthPolicySpec = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(spec.target_ref.kind, TargetRefKind::HttpRoute);
        assert_eq!(spec.target_ref.group, "gateway.networking.k8s.io");
        assert_eq!(spec.extra_policy.len(), 1);
        // Re-serializing yields the same camelCase document.
        assert_eq!(serde_json::to_value(&spec).unwrap(), json);
    }

    #[test]
    fn crd_has_expected_identity() {
        let crd = AuthPolicy::crd();
        assert_eq!(crd.spec.group, "authroute.dev");
        assert_eq!(crd.spec.names.kind, "AuthPolicy");
        assert_eq!(crd.spec.versions[0].name, "v1alpha1");
        assert!(crd.spec.scope == "Namespaced");
    }
}
