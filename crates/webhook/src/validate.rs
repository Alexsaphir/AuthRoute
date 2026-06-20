//! The three admission checks of ADR-0006 §1, expressed against the shared `api`
//! crate so they compile and type-check policy identically to request time.

use authroute_api::{AuthPolicySpec, compile_cel_policy, compile_path_regex};
use kube::Client;
use kube::api::{Api, ApiResource, DynamicObject, GroupVersionKind};

/// Run the *pure* admission checks (ADR-0006 §1.ii–iii): every CEL expression
/// parses, type-checks and yields `bool`, and every `pathRegex` compiles. No
/// cluster access — fully unit-testable. Returns an actionable message on the
/// first failure, naming the offending field.
pub fn validate_spec(spec: &AuthPolicySpec) -> Result<(), String> {
    compile_cel_policy(&spec.default_policy).map_err(|e| format!("spec.defaultPolicy {e}"))?;

    for (i, extra) in spec.extra_policy.iter().enumerate() {
        compile_path_regex(&extra.path_regex)
            .map_err(|e| format!("spec.extraPolicy[{i}].pathRegex {e}"))?;
        compile_cel_policy(&extra.policy)
            .map_err(|e| format!("spec.extraPolicy[{i}].policy {e}"))?;
    }

    Ok(())
}

/// Check that the `HTTPRoute` named by `spec.targetRef` exists in `namespace`
/// (ADR-0006 §1.i, the cross-resource lookup). Point-in-time: the controller
/// owns ongoing `ResolvedRefs` status (ADR-0006 §5).
///
/// `Ok(None)` means the target is present; `Ok(Some(msg))` is a rejection
/// reason; `Err` is an API failure the caller should surface as such.
pub async fn validate_target(
    client: Client,
    namespace: &str,
    spec: &AuthPolicySpec,
) -> Result<Option<String>, kube::Error> {
    let gvk = GroupVersionKind::gvk("gateway.networking.k8s.io", "v1", "HTTPRoute");
    let ar = ApiResource::from_gvk(&gvk);
    let routes: Api<DynamicObject> = Api::namespaced_with(client, namespace, &ar);

    match routes.get_opt(&spec.target_ref.name).await? {
        Some(_) => Ok(None),
        None => Ok(Some(format!(
            "spec.targetRef references HTTPRoute {:?} which does not exist in namespace {:?}",
            spec.target_ref.name, namespace
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use authroute_api::{ExtraPolicy, TargetRef, TargetRefKind};

    fn spec(default_policy: &str, extra: Vec<ExtraPolicy>) -> AuthPolicySpec {
        AuthPolicySpec {
            target_ref: TargetRef {
                group: "gateway.networking.k8s.io".to_string(),
                kind: TargetRefKind::HttpRoute,
                name: "grafana".to_string(),
            },
            default_policy: default_policy.to_string(),
            extra_policy: extra,
        }
    }

    #[test]
    fn accepts_a_well_formed_policy() {
        let s = spec(
            r#""admins" in groups"#,
            vec![ExtraPolicy {
                path_regex: r"^/public(/.*)?$".to_string(),
                policy: "true".to_string(),
            }],
        );
        assert!(validate_spec(&s).is_ok());
    }

    #[test]
    fn rejects_bad_default_cel() {
        let err = validate_spec(&spec("nope == 1", vec![])).unwrap_err();
        assert!(err.starts_with("spec.defaultPolicy"), "got {err}");
    }

    #[test]
    fn rejects_non_boolean_default_cel() {
        let err = validate_spec(&spec(r#""a string""#, vec![])).unwrap_err();
        assert!(err.starts_with("spec.defaultPolicy"), "got {err}");
    }

    #[test]
    fn rejects_bad_extra_regex() {
        let s = spec(
            "true",
            vec![ExtraPolicy {
                path_regex: r"^/api(/.*$".to_string(),
                policy: "true".to_string(),
            }],
        );
        let err = validate_spec(&s).unwrap_err();
        assert!(err.starts_with("spec.extraPolicy[0].pathRegex"), "got {err}");
    }

    #[test]
    fn rejects_bad_extra_cel() {
        let s = spec(
            "true",
            vec![ExtraPolicy {
                path_regex: r"^/api$".to_string(),
                policy: "user ==".to_string(),
            }],
        );
        let err = validate_spec(&s).unwrap_err();
        assert!(err.starts_with("spec.extraPolicy[0].policy"), "got {err}");
    }
}
