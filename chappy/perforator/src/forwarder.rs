use crate::{quic_utils, shutdown::ShutdownGuard};
use chappy_seed::Address;
use quinn::Endpoint;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, debug_span, info, instrument, warn, Instrument};

const SERVER_NAME: &str = "chappy";

/// A service relays TCP streams through a QUIC tunnel
///
/// The forwarder currently uses two different QUIC connections that match the
/// directions of the multiplexed TCP queries: a server QUIC endpoint is created
/// for TCP connections where the node acts as a server and a client QUIC
/// endpoint is used for TCP connections where the node acts as a client. This
/// could be optimized to use a single QUIC connection, but maintaining a
/// properly synchronized state machine that makes it possible to decide whether
/// the node should be the client or the server for the establishment of the
/// QUIC connection would actually be fairly complex.
#[derive(Debug)]
pub struct Forwarder {
    quic_endpoint: Endpoint,
    port: u16,
    server_certificate_der: Vec<u8>,
}

impl Forwarder {
    fn create_quic_endpoint(
        port: u16,
        server_certificate_der: Vec<u8>,
        private_key_der: Vec<u8>,
    ) -> Endpoint {
        // configure socket
        let sock = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None).unwrap();
        sock.set_reuse_port(true).unwrap();
        let src_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
        sock.bind(&src_addr.into()).unwrap();

        let server_config = quic_utils::configure_server(server_certificate_der, private_key_der);
        quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            Some(server_config),
            sock.into(),
            quinn::TokioRuntime,
        )
        .unwrap()
    }

    pub fn new(port: u16) -> Self {
        let cert = rcgen::generate_simple_self_signed(vec![SERVER_NAME.into()]).unwrap();
        let server_certificate_der = cert.serialize_der().unwrap();
        let private_key_der = cert.serialize_private_key_der();

        Self {
            quic_endpoint: Self::create_quic_endpoint(
                port,
                server_certificate_der.clone(),
                private_key_der,
            ),
            port,
            server_certificate_der,
        }
    }

    /// Run the forwarder p2p server
    ///
    /// Running this multiple times will fail.
    pub async fn run_quic_server(&self) {
        debug!("start QUIC server");
        loop {
            let connecting = self.quic_endpoint.accept().await.unwrap();
            let remote_addr = connecting.remote_address();
            let conn = match connecting.await {
                Ok(conn) => {
                    debug!(nat=%remote_addr, "connection accepted");
                    conn
                }
                Err(err) => {
                    debug!(nat=%remote_addr, err=%err, "failed to accept connection");
                    continue;
                }
            };
            tokio::spawn(
                async move {
                    loop {
                        let (mut quic_send, mut quic_recv) = match conn.accept_bi().await {
                            Ok(streams) => {
                                debug!("new bi accepted");
                                streams
                            }
                            Err(e) => {
                                info!("connection ended: {}", e);
                                break;
                            }
                        };
                        let target_port = quic_recv.read_u16().await.unwrap();

                        // forwarding connection
                        let localhost_url = format!("localhost:{}", target_port);
                        let fwd_stream = TcpStream::connect(localhost_url).await.unwrap();

                        // pipe holepunch connection to forwarding connection
                        let (mut fwd_read, mut fwd_write) = fwd_stream.into_split();
                        let out_fut = async move {
                            let bytes_copied = tokio::io::copy(&mut quic_recv, &mut fwd_write)
                                .await
                                .unwrap();
                            debug!(bytes_copied, "Outbound forwarding completed");
                        }
                        .instrument(debug_span!("cp_out", port = target_port));
                        let in_fut = async move {
                            let bytes_copied = tokio::io::copy(&mut fwd_read, &mut quic_send)
                                .await
                                .unwrap();
                            debug!(bytes_copied, "Inbound forwarding completed");
                        }
                        .instrument(debug_span!("cp_in", port = target_port));
                        tokio::join!(out_fut, in_fut);
                        debug!("closing bi");
                    }
                }
                .instrument(debug_span!("srv_quic_conn", src_nat = %remote_addr)),
            );
        }
    }

    /// Open a QUIC connection on the forwarder endpoint to relay the provided
    /// TcpStream
    ///
    /// Empirically, opening multiple client QUIC connections to the same target
    /// on a given endpoint works. It might be suboptimal as the connection
    /// management might involve some overhead (keepalive...)
    #[instrument(
        name = "cli_quic_conn",
        skip_all,
        fields(
            tgt_nat = %format!("{}:{}", nated_addr.ip, nated_addr.port),
            tgt_port = target_port
        )
    )]
    pub async fn forward(
        &self,
        tcp_stream: TcpStream,
        nated_addr: Address,
        target_port: u16,
        target_server_certificate_der: Vec<u8>,
        mut shdn: ShutdownGuard,
    ) {
        let cli_conf = quic_utils::configure_client(target_server_certificate_der);
        let quic_con;
        loop {
            let remote_addr = format!("{}:{}", nated_addr.ip, nated_addr.port)
                .parse()
                .unwrap();
            let endpoint_fut = self
                .quic_endpoint
                .connect_with(cli_conf.clone(), remote_addr, SERVER_NAME)
                .unwrap();
            let timed_endpoint_fut = tokio::time::timeout(Duration::from_millis(500), endpoint_fut);
            if let Ok(endpoint_res) = timed_endpoint_fut.await {
                quic_con = endpoint_res.unwrap();
                break;
            } else if shdn.is_shutting_down() {
                warn!("forwarding cancelled");
                return;
            } else {
                debug!("timeout, retrying...")
            }
        }
        let (mut quic_send, mut quic_recv) = quic_con.open_bi().await.unwrap();
        debug!("new bi opened");
        let (mut tcp_read, mut tcp_write) = tcp_stream.into_split();
        quic_send.write_u16(target_port).await.unwrap();
        let out_fut = async move {
            let bytes_copied = tokio::io::copy(&mut tcp_read, &mut quic_send)
                .await
                .unwrap();
            debug!(bytes_copied, "Outbound forwarding completed");
        }
        .instrument(debug_span!("cp_out", port = target_port));
        let in_fut = async move {
            let bytes_copied = tokio::io::copy(&mut quic_recv, &mut tcp_write)
                .await
                .unwrap();
            debug!(bytes_copied, "Inbound forwarding completed");
        }
        .instrument(debug_span!("cp_in", port = target_port));
        tokio::join!(out_fut, in_fut);
        debug!("closing bi");
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn server_certificate(&self) -> &[u8] {
        &self.server_certificate_der
    }
}
