use crate::quic_utils;
use chappy_seed::Address;
use log::debug;
use quinn::Endpoint;
use std::net::{Ipv4Addr, SocketAddrV4};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

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
}

impl Forwarder {
    async fn start_quic_server(server_p2p_port: u16) {
        let (server_config, _server_cert) = quic_utils::configure_server().unwrap();

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
            loop {
                let (mut quic_send, mut quic_recv) = conn.accept_bi().await.unwrap();
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
        }
    }

    fn create_quic_client(client_p2p_port: u16) -> Endpoint {
        let sock = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None).unwrap();
        sock.set_reuse_port(true).unwrap();
        let src_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, client_p2p_port);
        sock.bind(&src_addr.into()).unwrap();

        let mut src_quic_endpoint = quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            None,
            sock.into(),
            quinn::TokioRuntime,
        )
        .unwrap();
        src_quic_endpoint.set_default_client_config(quic_utils::configure_client());
        src_quic_endpoint
    }

    pub fn new(client_p2p_port: u16, server_p2p_port: u16) -> Self {
        tokio::spawn(Self::start_quic_server(server_p2p_port));

        Self {
            src_quic_endpoint: Self::create_quic_client(client_p2p_port),
            client_p2p_port,
            server_p2p_port,
        }
    }

    pub async fn forward(&self, tcp_stream: TcpStream, nated_addr: Address, target_port: u16) {
        // TODO: it is probably not okay to connect multiple time to the same
        // target address from the same Endpoint
        let quic_con = self
            .src_quic_endpoint
            .connect(
                format!("{}:{}", nated_addr.ip, nated_addr.port)
                    .parse()
                    .unwrap(),
                "chappy",
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
}
