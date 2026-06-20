//! Self-managed serving TLS for the admission webhook (ADR-0008).
//!
//! At startup the webhook mints its own self-signed certificate for the Service
//! DNS names and publishes the matching `caBundle` into the
//! `ValidatingWebhookConfiguration`, so no external cert tooling (cert-manager,
//! Helm cert generation) is required. The cert is ephemeral: a restart mints a
//! fresh one and re-patches the `caBundle`.

use rcgen::{BasicConstraints, CertificateParams, IsCa, KeyPair};

/// A freshly minted serving certificate plus the CA bundle the API server must
/// trust to dial the webhook.
pub struct ServingCert {
    /// PEM-encoded leaf certificate, served to the API server.
    pub cert_pem: String,
    /// PEM-encoded private key for [`Self::cert_pem`].
    pub key_pem: String,
    /// DER-encoded certificate to publish as the webhook `caBundle`. Because the
    /// cert is self-signed, it is its own trust anchor.
    pub ca_der: Vec<u8>,
}

/// Mint a self-signed serving certificate valid for `dns_names`.
///
/// The certificate is marked as a CA so it can act as its own trust anchor when
/// the API server verifies the served leaf against the published `caBundle`.
pub fn generate(dns_names: Vec<String>) -> Result<ServingCert, rcgen::Error> {
    let mut params = CertificateParams::new(dns_names)?;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);

    let key = KeyPair::generate()?;
    let cert = params.self_signed(&key)?;

    Ok(ServingCert {
        cert_pem: cert.pem(),
        key_pem: key.serialize_pem(),
        ca_der: cert.der().to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mints_a_cert_for_the_service_dns() {
        let cert = generate(vec!["authroute-webhook.authroute-system.svc".to_string()]).unwrap();
        assert!(cert.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(cert.key_pem.contains("PRIVATE KEY"));
        assert!(!cert.ca_der.is_empty());
    }
}