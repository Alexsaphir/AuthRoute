use kube::Client;

pub mod config;
pub mod consts;

pub async fn run() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = Client::try_default().await?;

    Ok(())
}
