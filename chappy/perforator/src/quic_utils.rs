use crate::{CHAPPY_CONF, PUNCH_SERVER_NAME, SERVER_NAME};

use quinn::{ClientConfig, Connection, Endpoint, ServerConfig, TransportConfig};
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tracing::{error, instrument, warn};

/// Returns default server configuration.
pub fn configure_server(certificate_der: Vec<u8>, private_key_der: Vec<u8>) -> ServerConfig {
    let priv_key = rustls::PrivateKey(private_key_der);
    let cert_chain = vec![rustls::Certificate(certificate_der)];

    let mut server_config = ServerConfig::with_single_cert(cert_chain, priv_key).unwrap();
    Arc::get_mut(&mut server_config.transport)
        .unwrap()
        .max_concurrent_uni_streams(0_u8.into())
        .keep_alive_interval(Some(Duration::from_secs(1)))
        .max_idle_timeout(Some(Duration::from_secs(5).try_into().unwrap()));

    server_config
}

/// Builds quinn client config and trusts given certificates.
fn configure_client(server_cert: Vec<u8>) -> ClientConfig {
    let mut certs = rustls::RootCertStore::empty();
    certs.add(&rustls::Certificate(server_cert)).unwrap();

    let mut transport = TransportConfig::default();
    transport.keep_alive_interval(Some(Duration::from_secs(1)));
    transport.max_idle_timeout(Some(Duration::from_secs(5).try_into().unwrap()));

    let mut cli = ClientConfig::with_root_certificates(certs);
    cli.transport_config(Arc::new(transport));
    cli
}

lazy_static! {
    /// Cached client certificate associated with PUNCH_SERVER_NAME. No server
    /// holds the associated private keys.
    pub static ref PUNCH_CERTIFICATE_DER: Vec<u8> =
        rcgen::generate_simple_self_signed(vec![PUNCH_SERVER_NAME.into()])
            .unwrap()
            .serialize_der()
            .unwrap();
}

/// Builds quinn client config and that only trusts a dummy certificate issued for PUNCH_SERVER_NAME. It
/// won't be able to actually connect to any server, but it will still be able
/// to perform hole punching.
pub fn configure_punch_client() -> ClientConfig {
    let mut certs = rustls::RootCertStore::empty();
    let trusted_cert = rustls::Certificate(PUNCH_CERTIFICATE_DER.clone());
    certs.add(&trusted_cert).unwrap();

    ClientConfig::with_root_certificates(certs)
}

#[instrument(name = "quic_conn_creation", skip_all)]
pub async fn connect_with_retry(
    endpoint: &Endpoint,
    target_server_addr: SocketAddr,
    target_server_certificate_der: Vec<u8>,
) -> Option<Connection> {
    let cli_conf = configure_client(target_server_certificate_der);
    let start = Instant::now();
    let quic_con;
    // TODO: investigate whether this retry is necessary or whether
    // QUIC/Quinn is handling retries internally
    loop {
        let endpoint_fut = endpoint
            .connect_with(cli_conf.clone(), target_server_addr, SERVER_NAME)
            .unwrap();
        let timed_endpoint_fut = tokio::time::timeout(Duration::from_millis(500), endpoint_fut);
        if let Ok(endpoint_res) = timed_endpoint_fut.await {
            quic_con = endpoint_res.unwrap();
            break;
        } else if start.elapsed() > Duration::from_millis(CHAPPY_CONF.connection_timeout_ms) {
            error!(
                elapsed=?start.elapsed(),
                timeout=?Duration::from_millis(CHAPPY_CONF.connection_timeout_ms),
                "connection timeout",
            );
            return None;
        } else {
            warn!("timeout, retrying...")
        }
    }
    Some(quic_con)
}
