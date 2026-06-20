use kube::Client;

pub mod config;
pub mod consts;

pub async fn run() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    // Bound in M3 when the reconcile loop lands; constructed now to verify
    // in-cluster config/credentials resolve at boot.
    let _client = Client::try_default().await?;

    Ok(())
}
