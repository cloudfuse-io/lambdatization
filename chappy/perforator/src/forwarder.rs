use crate::quic_utils;
use chappy_seed::Address;
use log::{debug, info};
use quinn::Endpoint;
use std::net::{Ipv4Addr, SocketAddrV4};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

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
    src_quic_endpoint: Endpoint,
    client_p2p_port: u16,
    server_p2p_port: u16,
    server_certificate_der: Vec<u8>,
}

impl Forwarder {
    async fn start_quic_server(
        server_p2p_port: u16,
        private_key_der: Vec<u8>,
        server_certificate_der: Vec<u8>,
    ) {
        let server_config =
            quic_utils::configure_server(server_certificate_der, private_key_der).unwrap();

        let sock = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None).unwrap();
        sock.set_reuse_port(true).unwrap();
        let src_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, server_p2p_port);
        sock.bind(&src_addr.into()).unwrap();

        let endpoint = quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            Some(server_config),
            sock.into(),
            quinn::TokioRuntime,
        )
        .unwrap();

        loop {
            let conn = endpoint.accept().await.unwrap().await.unwrap();
            tokio::spawn(async move {
                loop {
                    let (mut quic_send, mut quic_recv) = match conn.accept_bi().await {
                        Ok(streams) => streams,
                        Err(e) => {
                            info!("Connection with {} ended: {}", conn.remote_address(), e);
                            break;
                        }
                    };
                    let target_port = quic_recv.read_u16().await.unwrap();
                    // forwarding connection
                    let localhost_url = format!("localhost:{}", target_port);
                    let fwd_stream = TcpStream::connect(localhost_url).await.unwrap();

                    // pipe holepunch connection to forwarding connection
                    let (mut fwd_read, mut fwd_write) = fwd_stream.into_split();
                    let out_handle = tokio::spawn(async move {
                        debug!("Outbound forwarding started");
                        let bytes_copied = tokio::io::copy(&mut quic_recv, &mut fwd_write)
                            .await
                            .unwrap();
                        debug!("Outbound forwarding of {} bytes completed", bytes_copied);
                    });
                    let in_handle = tokio::spawn(async move {
                        debug!("Inbound forwarding started");
                        let bytes_copied = tokio::io::copy(&mut fwd_read, &mut quic_send)
                            .await
                            .unwrap();
                        debug!("Inbound forwarding of {} bytes completed", bytes_copied);
                    });
                    out_handle.await.unwrap();
                    in_handle.await.unwrap();
                }
            });
        }
    }

    fn create_quic_client(client_p2p_port: u16) -> Endpoint {
        let sock = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None).unwrap();
        sock.set_reuse_port(true).unwrap();
        let src_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, client_p2p_port);
        sock.bind(&src_addr.into()).unwrap();

        quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            None,
            sock.into(),
            quinn::TokioRuntime,
        )
        .unwrap()
    }

    pub fn new(client_p2p_port: u16, server_p2p_port: u16) -> Self {
        let cert = rcgen::generate_simple_self_signed(vec![SERVER_NAME.into()]).unwrap();
        let server_certificate_der = cert.serialize_der().unwrap();
        let private_key_der = cert.serialize_private_key_der();

        tokio::spawn(Self::start_quic_server(
            server_p2p_port,
            private_key_der,
            server_certificate_der.clone(),
        ));

        Self {
            src_quic_endpoint: Self::create_quic_client(client_p2p_port),
            client_p2p_port,
            server_p2p_port,
            server_certificate_der,
        }
    }

    /// Open a QUIC connection on the forwarder client endpoint to relay the
    /// provided TcpStream
    ///
    /// Empirically, opening multiple QUIC connections to the same target on a
    /// given endpoint works. It might be suboptimal as the connection
    /// management might involve some overhead (keepalive...)
    pub async fn forward(
        &self,
        tcp_stream: TcpStream,
        nated_addr: Address,
        target_port: u16,
        target_server_certificate_der: Vec<u8>,
    ) {
        let quic_con = self
            .src_quic_endpoint
            .connect_with(
                quic_utils::configure_client(target_server_certificate_der),
                format!("{}:{}", nated_addr.ip, nated_addr.port)
                    .parse()
                    .unwrap(),
                SERVER_NAME,
            )
            .unwrap()
            .await
            .unwrap();
        let (mut quic_send, mut quic_recv) = quic_con.open_bi().await.unwrap();
        let (mut tcp_read, mut tcp_write) = tcp_stream.into_split();
        quic_send.write_u16(target_port).await.unwrap();
        let out_handle = tokio::spawn(async move {
            debug!("Outbound forwarding started");
            let bytes_copied = tokio::io::copy(&mut tcp_read, &mut quic_send)
                .await
                .unwrap();
            debug!("Outbound forwarding of {} bytes completed", bytes_copied);
        });
        let in_handle = tokio::spawn(async move {
            debug!("Inbound forwarding started");
            let bytes_copied = tokio::io::copy(&mut quic_recv, &mut tcp_write)
                .await
                .unwrap();
            debug!("Inbound forwarding of {} bytes completed", bytes_copied);
        });
        out_handle.await.unwrap();
        in_handle.await.unwrap();
    }

    pub fn client_p2p_port(&self) -> u16 {
        self.client_p2p_port
    }

    pub fn server_p2p_port(&self) -> u16 {
        self.server_p2p_port
    }

    pub fn server_certificate(&self) -> &[u8] {
        &self.server_certificate_der
    }
}
