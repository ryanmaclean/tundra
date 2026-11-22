//! Outbound LLM API traffic must negotiate post-quantum hybrid key exchange
//! when the server supports it. rustls' aws-lc-rs provider with the
//! `prefer-post-quantum` feature lists X25519MLKEM768 first in `kx_groups`,
//! so it is the key share sent in the ClientHello.

use rustls::crypto::CryptoProvider;
use rustls::NamedGroup;

fn kx_names(provider: &CryptoProvider) -> Vec<NamedGroup> {
    provider.kx_groups.iter().map(|g| g.name()).collect()
}

#[test]
fn process_default_provider_prefers_x25519mlkem768() {
    at_core::tls::install_default_crypto_provider();

    let provider = CryptoProvider::get_default().expect("provider installed");
    let groups = kx_names(provider);
    eprintln!("process-default kx_groups = {groups:?}");
    assert_eq!(
        groups.first(),
        Some(&NamedGroup::X25519MLKEM768),
        "kx_groups = {groups:?}"
    );
    // Classical groups stay available as a fallback for servers without ML-KEM.
    assert!(
        groups.contains(&NamedGroup::X25519),
        "kx_groups = {groups:?}"
    );

    // The reqwest client the LLM providers use builds on top of that provider.
    reqwest::Client::builder()
        .build()
        .expect("reqwest client builds with the installed provider");
}

#[test]
fn crate_feature_default_also_prefers_x25519mlkem768() {
    // What reqwest falls back to when nothing is installed (e.g. library use
    // without our binaries' main): the aws-lc-rs default provider.
    let groups = kx_names(&rustls::crypto::aws_lc_rs::default_provider());
    assert_eq!(
        groups.first(),
        Some(&NamedGroup::X25519MLKEM768),
        "kx_groups = {groups:?}"
    );
}
