use chappy_perforator::{
    binding_service::BindingService, forwarder::Forwarder, perforator::Perforator,
    protocol::ParsedTcpStream, CHAPPY_CONF,
};
use chappy_util::CustomTime;
use tracing::{debug_span, info, Instrument};
use tracing_subscriber::EnvFilter;

use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .with_timer(CustomTime)
        .init();
    let tcp_port = 5000;
    let tcp_addr = format!("127.0.0.1:{}", tcp_port);
    let seed_addr = format!("{}:{}", CHAPPY_CONF.seed_hostname, CHAPPY_CONF.seed_port);
    let (cli_quic_port, srv_quic_port) = (5001, 5002);
    info!(
        perforator_tcp_address = %tcp_addr,
        perforator_quic_client_port = cli_quic_port,
        perforator_quic_server_port = srv_quic_port,
        seed_address = %seed_addr
    );
    let listener = TcpListener::bind(tcp_addr).await.unwrap();
    let forwarder = Forwarder::new(cli_quic_port, srv_quic_port);
    let binding_service = BindingService::new(cli_quic_port, srv_quic_port);
    let perforator = Arc::new(Perforator::new(forwarder, binding_service));
    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let src_port = stream.peer_addr().unwrap().port();
        let perforator = Arc::clone(&perforator);
        tokio::spawn(
            async move {
                let parsed_stream = ParsedTcpStream::from(stream).await;
                match parsed_stream {
                    ParsedTcpStream::ClientRegistration {
                        source_port,
                        target_virtual_ip,
                        target_port,
                    } => perforator.register_client(source_port, target_virtual_ip, target_port),
                    ParsedTcpStream::ServerRegistration => perforator.register_server(),
                    ParsedTcpStream::Raw(stream) => perforator.forward_conn(stream).await,
                }
            }
            .instrument(debug_span!("tcp", src_port)),
        );
    }
}
