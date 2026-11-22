//! Process-wide TLS crypto provider selection.
//!
//! Outbound HTTPS (reqwest 0.13, octocrab/hyper-rustls) runs on rustls with
//! the aws-lc-rs provider. The workspace enables rustls' `prefer-post-quantum`
//! feature, so the provider offers the hybrid X25519MLKEM768 key exchange
//! first. Installing it explicitly makes the choice deterministic: if any
//! dependency ever enables rustls' `ring` feature as well, rustls can no
//! longer infer a default and `ClientConfig::builder()` would panic.

use std::sync::Arc;

use rustls::crypto::CryptoProvider;

/// Install aws-lc-rs as the process-default rustls [`CryptoProvider`].
///
/// Call this first thing in every binary `main`, and from tests that build a
/// rustls `ClientConfig` (directly or through reqwest/octocrab). Idempotent:
/// if a provider is already installed it is left in place and returned.
pub fn install_default_crypto_provider() -> Arc<CryptoProvider> {
    // install_default() only fails when another provider won the race; the
    // installed one is returned by get_default() either way.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    CryptoProvider::get_default()
        .cloned()
        .expect("a rustls CryptoProvider is installed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_aws_lc_rs_with_post_quantum_first() {
        let provider = install_default_crypto_provider();
        assert_eq!(
            provider.kx_groups.first().map(|g| g.name()),
            Some(rustls::NamedGroup::X25519MLKEM768)
        );
        // Idempotent.
        let again = install_default_crypto_provider();
        assert!(Arc::ptr_eq(&provider, &again));
    }
}
