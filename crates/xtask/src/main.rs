//! AuthRoute build helper. Generates `deploy/` manifests from the Rust types so
//! the YAML never drifts from the source of truth (ADR-0007).
//!
//! Usage: `cargo run -p xtask -- codegen` (runs every generator), or a single
//! generator by name: `cargo run -p xtask -- crds` / `cargo run -p xtask -- rbac`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use authroute_api::AuthPolicy;
use k8s_openapi::Resource;
use k8s_openapi::api::admissionregistration::v1::{
    RuleWithOperations, ServiceReference, ValidatingWebhook, ValidatingWebhookConfiguration,
    WebhookClientConfig,
};
use k8s_openapi::api::core::v1::{Service, ServiceAccount, ServicePort, ServiceSpec};
use k8s_openapi::api::rbac::v1::{
    ClusterRole, ClusterRoleBinding, PolicyRule, Role, RoleBinding, RoleRef, Subject,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::CustomResourceExt;
use serde::Serialize;

/// Namespace the controller's `ServiceAccount`, `Role`, and bindings install
/// into. A placeholder for the raw manifests; the Helm chart templates the real
/// release namespace (see `controller::consts::OPERATOR_NAMESPACE_ENV`).
const NAMESPACE: &str = "authroute-system";

/// Name shared by the controller's `ServiceAccount` and (cluster) roles/bindings.
const CONTROLLER_NAME: &str = "authroute-controller";

/// Name shared by the webhook's `ServiceAccount`, `Service`, RBAC, and the
/// `ValidatingWebhookConfiguration` whose `caBundle` it self-patches (ADR-0008).
/// Must match `webhook::config::NAME`.
const WEBHOOK_NAME: &str = "authroute-webhook";

type DynError = Box<dyn std::error::Error>;

fn main() -> Result<(), DynError> {
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "codegen".into());
    match cmd.as_str() {
        "codegen" => {
            crds()?;
            rbac()?;
            webhook()
        }
        "crds" => crds(),
        "rbac" => rbac(),
        "webhook" => webhook(),
        other => Err(format!(
            "unknown xtask command {other:?}; expected `codegen`, `crds`, `rbac`, or `webhook`"
        )
        .into()),
    }
}

/// Write `deploy/crds/` from the `AuthPolicy` Rust type.
fn crds() -> Result<(), DynError> {
    let dir = deploy_dir("crds")?;
    let path = dir.join("authroute.dev_authpolicies.yaml");
    write_manifest(&path, &serde_yaml::to_string(&AuthPolicy::crd())?)?;
    Ok(())
}

/// Write `deploy/rbac/controller.yaml` — the least-privilege permissions the
/// controller needs, derived from the watches the ADRs settle:
///   - `authpolicies` + `authpolicies/status` — the primary watch and the
///     `PolicyStatus` it patches (ADR-0002 §D6).
///   - `httproutes` — `ResolvedRefs` validation of `spec.targetRef` (ADR-0002 §D1).
///   - `events` — surfacing reconcile outcomes.
///   - `leases` — leader election for the HA requirement (ADR-0004); namespaced,
///     so it lives in a `Role`, not the `ClusterRole`.
///
/// AuthRoute does not reconcile `SecurityPolicy` (the chart owns the single
/// gateway-wide one, ADR-0002 §D5), so no write grant on Envoy Gateway resources.
fn rbac() -> Result<(), DynError> {
    let service_account = ServiceAccount {
        metadata: namespaced_meta(CONTROLLER_NAME),
        ..Default::default()
    };

    let cluster_role = ClusterRole {
        metadata: cluster_meta(CONTROLLER_NAME),
        rules: Some(vec![
            rule(
                &["authroute.dev"],
                &["authpolicies"],
                &["get", "list", "watch"],
            ),
            rule(
                &["authroute.dev"],
                &["authpolicies/status"],
                &["get", "patch", "update"],
            ),
            rule(
                &["gateway.networking.k8s.io"],
                &["httproutes"],
                &["get", "list", "watch"],
            ),
            rule(&[""], &["events"], &["create", "patch"]),
        ]),
        ..Default::default()
    };

    let cluster_role_binding = ClusterRoleBinding {
        metadata: cluster_meta(CONTROLLER_NAME),
        role_ref: role_ref("ClusterRole", CONTROLLER_NAME),
        subjects: Some(vec![sa_subject(CONTROLLER_NAME)]),
    };

    // Leases are namespaced; leader election only needs them in the operator's
    // own namespace, so this is a Role rather than a ClusterRole.
    let role = Role {
        metadata: namespaced_meta(CONTROLLER_NAME),
        rules: Some(vec![rule(
            &["coordination.k8s.io"],
            &["leases"],
            &[
                "get", "list", "watch", "create", "update", "patch", "delete",
            ],
        )]),
    };

    let role_binding = RoleBinding {
        metadata: namespaced_meta(CONTROLLER_NAME),
        role_ref: role_ref("Role", CONTROLLER_NAME),
        subjects: Some(vec![sa_subject(CONTROLLER_NAME)]),
    };

    let docs = [
        document(&service_account)?,
        document(&cluster_role)?,
        document(&cluster_role_binding)?,
        document(&role)?,
        document(&role_binding)?,
    ];

    let path = deploy_dir("rbac")?.join("controller.yaml");
    write_manifest(&path, &docs.join("---\n"))?;
    Ok(())
}

/// Write `deploy/webhook/webhook.yaml` — everything the admission webhook needs
/// that is generated from the Rust types (ADR-0006, ADR-0008):
///   - a `ServiceAccount` and least-privilege RBAC: `get` on `httproutes` (the
///     §1.i target-exists check, cluster-wide since targets are namespace-local
///     but policies live anywhere) and `get`/`update` on
///     `validatingwebhookconfigurations` (to self-publish its `caBundle`).
///   - a `Service` fronting the pod, and the `ValidatingWebhookConfiguration`
///     itself, scoped to `authpolicies` create/update with `failurePolicy: Fail`
///     (ADR-0006 §4). The `caBundle` is left empty; the running webhook patches
///     it at startup (ADR-0008).
fn webhook() -> Result<(), DynError> {
    let service_account = ServiceAccount {
        metadata: namespaced_meta(WEBHOOK_NAME),
        ..Default::default()
    };

    let cluster_role = ClusterRole {
        metadata: cluster_meta(WEBHOOK_NAME),
        rules: Some(vec![
            rule(
                &["gateway.networking.k8s.io"],
                &["httproutes"],
                &["get", "list", "watch"],
            ),
            rule(
                &["admissionregistration.k8s.io"],
                &["validatingwebhookconfigurations"],
                &["get", "list", "watch", "update", "patch"],
            ),
        ]),
        ..Default::default()
    };

    let cluster_role_binding = ClusterRoleBinding {
        metadata: cluster_meta(WEBHOOK_NAME),
        role_ref: role_ref("ClusterRole", WEBHOOK_NAME),
        subjects: Some(vec![sa_subject(WEBHOOK_NAME)]),
    };

    let service = webhook_service();
    let configuration = webhook_configuration();

    let docs = [
        document(&service_account)?,
        document(&cluster_role)?,
        document(&cluster_role_binding)?,
        document(&service)?,
        document(&configuration)?,
    ];

    let path = deploy_dir("webhook")?.join("webhook.yaml");
    write_manifest(&path, &docs.join("---\n"))?;
    Ok(())
}

/// The `Service` the API server dials; port 443 → the webhook's 8443 (matches
/// `webhook::config::DEFAULT_LISTEN_ADDR`).
fn webhook_service() -> Service {
    Service {
        metadata: namespaced_meta(WEBHOOK_NAME),
        spec: Some(ServiceSpec {
            selector: Some(BTreeMap::from([(
                "app.kubernetes.io/name".to_string(),
                WEBHOOK_NAME.to_string(),
            )])),
            ports: Some(vec![ServicePort {
                port: 443,
                target_port: Some(IntOrString::Int(8443)),
                ..Default::default()
            }]),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// The `ValidatingWebhookConfiguration`, scoped to `authpolicies` create/update
/// and fail-closed (ADR-0006 §1, §4). `caBundle` is empty and self-patched at
/// runtime (ADR-0008).
fn webhook_configuration() -> ValidatingWebhookConfiguration {
    ValidatingWebhookConfiguration {
        metadata: cluster_meta(WEBHOOK_NAME),
        webhooks: Some(vec![ValidatingWebhook {
            name: "vauthpolicy.authroute.dev".to_string(),
            admission_review_versions: vec!["v1".to_string()],
            side_effects: "None".to_string(),
            failure_policy: Some("Fail".to_string()),
            client_config: WebhookClientConfig {
                service: Some(ServiceReference {
                    name: WEBHOOK_NAME.to_string(),
                    namespace: NAMESPACE.to_string(),
                    path: Some("/validate".to_string()),
                    port: Some(443),
                }),
                ..Default::default()
            },
            rules: Some(vec![RuleWithOperations {
                api_groups: Some(vec!["authroute.dev".to_string()]),
                api_versions: Some(vec!["v1alpha1".to_string()]),
                operations: Some(vec!["CREATE".to_string(), "UPDATE".to_string()]),
                resources: Some(vec!["authpolicies".to_string()]),
                scope: Some("Namespaced".to_string()),
            }]),
            ..Default::default()
        }]),
    }
}

/// A single `PolicyRule` over one API group.
fn rule(api_groups: &[&str], resources: &[&str], verbs: &[&str]) -> PolicyRule {
    PolicyRule {
        api_groups: Some(api_groups.iter().map(|s| s.to_string()).collect()),
        resources: Some(resources.iter().map(|s| s.to_string()).collect()),
        verbs: verbs.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

fn role_ref(kind: &str, name: &str) -> RoleRef {
    RoleRef {
        api_group: "rbac.authorization.k8s.io".to_string(),
        kind: kind.to_string(),
        name: name.to_string(),
    }
}

fn sa_subject(name: &str) -> Subject {
    Subject {
        kind: "ServiceAccount".to_string(),
        name: name.to_string(),
        namespace: Some(NAMESPACE.to_string()),
        ..Default::default()
    }
}

fn cluster_meta(name: &str) -> ObjectMeta {
    ObjectMeta {
        name: Some(name.to_string()),
        ..Default::default()
    }
}

fn namespaced_meta(name: &str) -> ObjectMeta {
    ObjectMeta {
        name: Some(name.to_string()),
        namespace: Some(NAMESPACE.to_string()),
        ..Default::default()
    }
}

/// Serialize a `k8s-openapi` resource to a YAML document. These structs omit
/// `apiVersion`/`kind` (they live in the [`Resource`] trait), so inject them up
/// front, otherwise `kubectl apply` rejects the document.
fn document<K: Resource + Serialize>(obj: &K) -> Result<String, DynError> {
    let mut value = serde_json::to_value(obj)?;
    let map = value
        .as_object_mut()
        .ok_or("k8s resource did not serialize to a JSON object")?;
    map.insert("apiVersion".into(), K::API_VERSION.into());
    map.insert("kind".into(), K::KIND.into());
    Ok(serde_yaml::to_string(&value)?)
}

/// Ensure `deploy/<sub>/` exists and return it.
fn deploy_dir(sub: &str) -> Result<PathBuf, DynError> {
    let dir = workspace_root().join("deploy").join(sub);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Write `body` under the generated-file header and report the path.
fn write_manifest(path: &Path, body: &str) -> Result<(), DynError> {
    let header = "# Generated by `cargo run -p xtask -- codegen`. Do not edit by hand.\n";
    std::fs::write(path, format!("{header}{body}"))?;
    println!("wrote {}", path.display());
    Ok(())
}

/// The workspace root, derived from this crate's manifest dir (`<root>/crates/xtask`).
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("xtask lives at <root>/crates/xtask")
        .to_path_buf()
}
