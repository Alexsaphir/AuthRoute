//! AuthRoute build helper. Generates `deploy/` manifests from the Rust types so
//! the YAML never drifts from the source of truth (ADR-0007).
//!
//! Usage: `cargo run -p xtask -- codegen` (runs every generator), or a single
//! generator by name: `cargo run -p xtask -- crds` / `cargo run -p xtask -- rbac`.

use std::path::{Path, PathBuf};

use authroute_api::AuthPolicy;
use k8s_openapi::Resource;
use k8s_openapi::api::core::v1::ServiceAccount;
use k8s_openapi::api::rbac::v1::{
    ClusterRole, ClusterRoleBinding, PolicyRule, Role, RoleBinding, RoleRef, Subject,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::CustomResourceExt;
use serde::Serialize;

/// Namespace the controller's `ServiceAccount`, `Role`, and bindings install
/// into. A placeholder for the raw manifests; the Helm chart templates the real
/// release namespace (see `controller::consts::OPERATOR_NAMESPACE_ENV`).
const NAMESPACE: &str = "authroute-system";

/// Name shared by the controller's `ServiceAccount` and (cluster) roles/bindings.
const NAME: &str = "authroute-controller";

type DynError = Box<dyn std::error::Error>;

fn main() -> Result<(), DynError> {
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "codegen".into());
    match cmd.as_str() {
        "codegen" => {
            crds()?;
            rbac()
        }
        "crds" => crds(),
        "rbac" => rbac(),
        other => Err(format!(
            "unknown xtask command {other:?}; expected `codegen`, `crds`, or `rbac`"
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
        metadata: namespaced_meta(NAME),
        ..Default::default()
    };

    let cluster_role = ClusterRole {
        metadata: cluster_meta(NAME),
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
        metadata: cluster_meta(NAME),
        role_ref: role_ref("ClusterRole", NAME),
        subjects: Some(vec![sa_subject()]),
    };

    // Leases are namespaced; leader election only needs them in the operator's
    // own namespace, so this is a Role rather than a ClusterRole.
    let role = Role {
        metadata: namespaced_meta(NAME),
        rules: Some(vec![rule(
            &["coordination.k8s.io"],
            &["leases"],
            &[
                "get", "list", "watch", "create", "update", "patch", "delete",
            ],
        )]),
    };

    let role_binding = RoleBinding {
        metadata: namespaced_meta(NAME),
        role_ref: role_ref("Role", NAME),
        subjects: Some(vec![sa_subject()]),
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

fn sa_subject() -> Subject {
    Subject {
        kind: "ServiceAccount".to_string(),
        name: NAME.to_string(),
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
