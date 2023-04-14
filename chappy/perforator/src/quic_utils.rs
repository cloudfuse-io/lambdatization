use quinn::{ClientConfig, ServerConfig, TransportConfig};
use std::{sync::Arc, time::Duration};

/// Returns default server configuration.
pub fn configure_server(certificate_der: Vec<u8>, private_key_der: Vec<u8>) -> ServerConfig {
    let priv_key = rustls::PrivateKey(private_key_der);
    let cert_chain = vec![rustls::Certificate(certificate_der)];

    let mut server_config = ServerConfig::with_single_cert(cert_chain, priv_key).unwrap();
    Arc::get_mut(&mut server_config.transport)
        .unwrap()
        .max_concurrent_uni_streams(0_u8.into())
        .keep_alive_interval(Some(Duration::from_secs(1)))
        // NOTE: removing this timeout might leave lingering connections around
        .max_idle_timeout(None);

    server_config
}

/// Builds quinn client config and trusts given certificates.
pub fn configure_client(server_cert: Vec<u8>) -> ClientConfig {
    let mut certs = rustls::RootCertStore::empty();
    certs.add(&rustls::Certificate(server_cert)).unwrap();

    let mut transport = TransportConfig::default();
    transport.keep_alive_interval(Some(Duration::from_secs(1)));
    // NOTE: removing this timeout might leave lingering connections around
    transport.max_idle_timeout(None);

    let mut cli = ClientConfig::with_root_certificates(certs);
    cli.transport_config(Arc::new(transport));
    cli
}
