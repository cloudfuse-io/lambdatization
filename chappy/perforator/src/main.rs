use chappy_perforator::{
    binding_service::BindingService,
    forwarder::Forwarder,
    perforator::Perforator,
    protocol::ParsedTcpStream,
    shutdown::{gracefull, GracefullyRunnable, Shutdown},
    CHAPPY_CONF,
};
use chappy_util::init_tracing;
use std::sync::Arc;
use tokio::net::TcpListener;
use tonic::async_trait;
use tracing::{debug_span, info, Instrument};

struct SrvRunnable;

#[async_trait]
impl GracefullyRunnable for SrvRunnable {
    async fn run(&self, shutdown: &Shutdown) {
        let tcp_port = 5000;
        let seed_addr = format!("{}:{}", CHAPPY_CONF.seed_hostname, CHAPPY_CONF.seed_port);
        let (cli_quic_port, srv_quic_port) = (5001, 5002);
        info!(
            perforator_tcp_port = tcp_port,
            perforator_quic_client_port = cli_quic_port,
            perforator_quic_server_port = srv_quic_port,
            seed_address = %seed_addr
        );
        let listener = TcpListener::bind(format!("127.0.0.1:{}", tcp_port))
            .await
            .unwrap();
        let forwarder = Forwarder::new(cli_quic_port, srv_quic_port);
        let binding_service = BindingService::new(cli_quic_port, srv_quic_port);
        let perforator = Arc::new(Perforator::new(forwarder, binding_service));
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let src_port = stream.peer_addr().unwrap().port();
            let perforator = Arc::clone(&perforator);
            let shutdown_guard = shutdown.create_guard();
            tokio::spawn(
                async move {
                    let parsed_stream = ParsedTcpStream::from(stream).await;
                    match parsed_stream {
                        ParsedTcpStream::ClientRegistration {
                            source_port,
                            target_virtual_ip,
                            target_port,
                        } => {
                            perforator.register_client(source_port, target_virtual_ip, target_port)
                        }
                        ParsedTcpStream::ServerRegistration => perforator.register_server(),
                        ParsedTcpStream::Raw(stream) => {
                            perforator.forward_conn(stream, shutdown_guard).await
                        }
                    }
                }
                .instrument(debug_span!("tcp", src_port)),
            );
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    init_tracing();
    gracefull(SrvRunnable).await;
}
