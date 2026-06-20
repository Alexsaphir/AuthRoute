//! AuthRoute admission webhook — a `ValidatingAdmissionWebhook` that fail-fast
//! rejects malformed `AuthPolicy` writes (ADR-0006). It runs the three checks of
//! ADR-0006 §1 (CEL valid, regex valid, target exists) through the shared `api`
//! crate, and serves over self-managed TLS (ADR-0008).

pub mod config;
pub mod tls;
pub mod validate;

use std::net::SocketAddr;

use authroute_api::AuthPolicy;
use axum::Router;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, response::IntoResponse};
use axum_server::tls_rustls::RustlsConfig;
use k8s_openapi::ByteString;
use k8s_openapi::api::admissionregistration::v1::ValidatingWebhookConfiguration;
use kube::Client;
use kube::api::{Api, PostParams};
use kube::core::admission::{AdmissionRequest, AdmissionResponse, AdmissionReview};

/// Build the client and TLS, publish the `caBundle`, and serve until shutdown.
pub async fn run() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    init_tracing();

    let client = Client::try_default().await?;
    let namespace = config::namespace();

    // Mint the serving cert and publish its CA into the webhook configuration so
    // the API server trusts us (ADR-0008). Best-effort: a missing configuration
    // (e.g. not yet installed) is logged, not fatal.
    let serving = tls::generate(config::serving_dns_names(&namespace))?;
    publish_ca_bundle(&client, &serving.ca_der).await;

    let tls = RustlsConfig::from_pem(serving.cert_pem.into_bytes(), serving.key_pem.into_bytes())
        .await?;

    let app = Router::new()
        .route(config::VALIDATE_PATH, post(handle_validate))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(client);

    let addr: SocketAddr = config::listen_addr().parse()?;
    tracing::info!(%addr, path = config::VALIDATE_PATH, "admission webhook listening");
    axum_server::bind_rustls(addr, tls)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

/// Admission entry point: validate one `AuthPolicy` create/update and answer
/// allow/deny. The webhook is scoped to `authpolicies` only, so every request
/// carries an `AuthPolicy` object.
async fn handle_validate(
    State(client): State<Client>,
    Json(review): Json<AdmissionReview<AuthPolicy>>,
) -> impl IntoResponse {
    let req: AdmissionRequest<AuthPolicy> = match review.try_into() {
        Ok(req) => req,
        Err(err) => {
            tracing::warn!(%err, "malformed AdmissionReview");
            return Json(AdmissionResponse::invalid(err.to_string()).into_review());
        }
    };

    let resp = AdmissionResponse::from(&req);
    let resp = match req.object.as_ref() {
        // No object (e.g. a DELETE review): nothing to validate, admit.
        None => resp,
        Some(policy) => review_policy(client, &req, policy, resp).await,
    };
    Json(resp.into_review())
}

/// Apply the ADR-0006 §1 checks to one policy, denying with an actionable
/// message on the first failure. Fail-closed: an API error verifying the target
/// is a denial, consistent with `failurePolicy: Fail` (ADR-0006 §4).
async fn review_policy(
    client: Client,
    req: &AdmissionRequest<AuthPolicy>,
    policy: &AuthPolicy,
    resp: AdmissionResponse,
) -> AdmissionResponse {
    if let Err(message) = validate::validate_spec(&policy.spec) {
        return resp.deny(message);
    }

    let namespace = req.namespace.as_deref().unwrap_or("default");
    match validate::validate_target(client, namespace, &policy.spec).await {
        Ok(None) => resp,
        Ok(Some(message)) => resp.deny(message),
        Err(err) => resp.deny(format!("could not verify spec.targetRef: {err}")),
    }
}

/// Patch the published `caBundle` of the `ValidatingWebhookConfiguration` named
/// [`config::NAME`] to the freshly minted CA. Best-effort and idempotent.
async fn publish_ca_bundle(client: &Client, ca_der: &[u8]) {
    let api: Api<ValidatingWebhookConfiguration> = Api::all(client.clone());
    let mut cfg = match api.get(config::NAME).await {
        Ok(cfg) => cfg,
        Err(err) => {
            tracing::warn!(%err, name = config::NAME,
                "no ValidatingWebhookConfiguration to patch; serving anyway");
            return;
        }
    };

    if let Some(webhooks) = cfg.webhooks.as_mut() {
        for webhook in webhooks.iter_mut() {
            webhook.client_config.ca_bundle = Some(ByteString(ca_der.to_vec()));
        }
    }

    match api.replace(config::NAME, &PostParams::default(), &cfg).await {
        Ok(_) => tracing::info!(name = config::NAME, "published webhook caBundle"),
        Err(err) => tracing::warn!(%err, name = config::NAME, "failed to publish caBundle"),
    }
}

/// Initialize JSON tracing from `RUST_LOG`, defaulting to `info`. Idempotent
/// enough for `main`; ignores a double-init in tests.
fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .try_init();
}