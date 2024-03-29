use crate::fwd_protocol::{copy, InitQuery, InitResponse};
use crate::spawn::spawn_task;
use crate::{quic_utils, shutdown::Shutdown, PUNCH_SERVER_NAME, SERVER_NAME};
use anyhow::{anyhow, Result};
use chappy_util::tcp_connect::connect_retry;
use quinn::{Connection, ConnectionError, Endpoint};
use quinn_proto::{TransportError, TransportErrorCode};
use rustls::AlertDescription::UnknownCA;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tracing::{debug, debug_span, error, info, instrument, trace, Instrument};

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
            Arc::new(quinn::TokioRuntime),
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

    /// Accept one bi QUIC stream, decode the target_port and forward the rest
    /// of the stream to localhost:target_port
    ///
    /// Panic if receives a second bi on the connection
    async fn handle_srv_conn(conn: Connection) {
        let (mut quic_send, mut quic_recv) = match conn.accept_bi().await {
            Ok(streams) => {
                trace!("new bi accepted");
                streams
            }
            Err(e) => {
                info!("connection ended: {}", e);
                return;
            }
        };
        let query = InitQuery::read(&mut quic_recv).await;
        debug!(?query, "init query read");

        // forwarding connection

        // TODO: make timeout configurable according to expected target startup
        // duration
        let fwd_stream = match connect_retry(
            (Ipv4Addr::LOCALHOST, query.target_port),
            Duration::from_millis(500),
        )
        .await
        {
            Ok(stream) => {
                InitResponse { code: 0 }.write(&mut quic_send).await;
                stream
            }
            Err(err) => {
                error!(err=%err, "connection to target failed");
                InitResponse { code: 1 }.write(&mut quic_send).await;
                quic_send.finish().await.unwrap();
                return;
            }
        };

        if query.connect_only {
            quic_send.finish().await.unwrap();
            return;
        }

        // pipe holepunch connection to forwarding connection
        let (fwd_read, fwd_write) = fwd_stream.into_split();
        let out_fut = copy(quic_recv, fwd_write)
            .instrument(debug_span!("cp_quic_tcp", port = query.target_port));
        let in_fut = copy(fwd_read, quic_send)
            .instrument(debug_span!("cp_tcp_quic", port = query.target_port));
        tokio::try_join!(out_fut, in_fut).ok();
        trace!("closing bi");
        // expect the connection to be closed by caller
        conn.accept_bi()
            .await
            .expect_err("Unexpected second bi on connection");
    }

    /// Run the forwarder p2p server
    ///
    /// Running this multiple times will fail.
    #[instrument(name = "quic_srv", skip_all)]
    pub async fn run_quic_server(&self, shutdown: &Shutdown) {
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
            let shdwn_guard = shutdown.create_guard();
            spawn_task(
                shdwn_guard,
                debug_span!("srv_quic_conn", src_nat = %remote_addr),
                Self::handle_srv_conn(conn),
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
            tgt_nat = %nated_addr,
            tgt_port = target_port
        )
    )]
    pub async fn forward(
        &self,
        tcp_stream: TcpStream,
        nated_addr: SocketAddr,
        target_port: u16,
        target_server_certificate_der: Vec<u8>,
    ) {
        let conn_result = quic_utils::connect_with_retry(
            &self.quic_endpoint,
            nated_addr,
            target_server_certificate_der,
        )
        .await;
        if conn_result.is_none() {
            error!("QUIC conn failed, dropping upstream connection");
            tcp_stream.set_linger(None).unwrap();
            return;
        }
        let quic_conn = conn_result.unwrap();
        let (mut quic_send, mut quic_recv) = quic_conn.open_bi().await.unwrap();
        trace!("new bi opened");
        let query = InitQuery {
            target_port,
            connect_only: false,
        };
        query.write(&mut quic_send).await;
        let InitResponse { code } = InitResponse::read(&mut quic_recv).await.unwrap();
        match code {
            0 => debug!("target conn successful"),
            err_code => {
                // at this point the clients already think they are connected,
                // so we are converting a connection establishment error into a
                // lost connection error
                error!(err_code, "target conn failed, dropping upstream connection");
                quic_send.finish().await.unwrap();
                tcp_stream.set_linger(None).unwrap();
                return;
            }
        }
        let (tcp_read, tcp_write) = tcp_stream.into_split();
        let out_fut =
            copy(tcp_read, quic_send).instrument(debug_span!("cp_tcp_quic", port = target_port));
        let in_fut =
            copy(quic_recv, tcp_write).instrument(debug_span!("cp_quic_tcp", port = target_port));
        tokio::try_join!(out_fut, in_fut).ok();
        trace!("closing bi");
    }

    #[instrument(
        name = "cli_try_tgt",
        skip_all,
        fields(
            tgt_nat = %nated_addr,
            tgt_port = target_port
        )
    )]
    pub async fn try_target(
        &self,
        nated_addr: SocketAddr,
        target_port: u16,
        target_server_certificate_der: Vec<u8>,
    ) -> Result<()> {
        let conn_result = quic_utils::connect_with_retry(
            &self.quic_endpoint,
            nated_addr,
            target_server_certificate_der,
        )
        .await;
        if conn_result.is_none() {
            return Err(anyhow!("quic conn failed"));
        }
        let quic_conn = conn_result.unwrap();

        // bi opening timeout means an unexpected QUIC flow control kicked in
        let (mut quic_send, mut quic_recv) =
            tokio::time::timeout(Duration::from_millis(50), quic_conn.open_bi())
                .await
                .unwrap()
                .unwrap();

        trace!("new bi opened");
        let query = InitQuery {
            target_port,
            connect_only: true,
        };
        query.write(&mut quic_send).await;
        let InitResponse { code } = match InitResponse::read(&mut quic_recv).await {
            Ok(r) => r,
            Err(err) => {
                error!(%err, "proxy conn failed");
                return Err(err.into());
            }
        };
        match code {
            0 => debug!("target conn successful"),
            err_code => {
                error!(err_code, "target conn failed");
                return Err(anyhow!("target conn failed with code {}", err_code));
            }
        }
        trace!("closing bi");
        Ok(())
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn server_certificate(&self) -> &[u8] {
        &self.server_certificate_der
    }

    #[instrument(skip(self))]
    pub async fn punch_hole(&self, nat: SocketAddr, virt: String) -> Result<()> {
        debug!("make punch conn to client");
        let connecting = self
            .quic_endpoint
            .connect_with(quic_utils::configure_punch_client(), nat, PUNCH_SERVER_NAME)
            .unwrap();
        // we expect the connection establishment mechanism to handle retries
        // until the hole is actually punched
        match connecting.await {
            Ok(_) => Err(anyhow!("Connection unexpectedly successful")),
            Err(ConnectionError::TransportError(TransportError { code: c, .. }))
                if c == TransportErrorCode::crypto(UnknownCA.get_u8()) =>
            {
                // Got expected certificate error
                Ok(())
            }
            Err(e) => Err(anyhow!("Unexpected error {:?}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chappy_util::test;
    use futures::StreamExt;
    use rand::seq::SliceRandom;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;

    /// Create a TCP server on the specified port and connect to it, then
    /// forward the server side stream using the provided forwarder and target
    /// port
    async fn simulate_proxied_connect(
        port: u16,
        fwd: &Arc<Forwarder>,
        target_port: u16,
    ) -> (TcpStream, JoinHandle<()>) {
        let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

        let accept_handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            (stream, listener)
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        let cli_stream = TcpStream::connect(addr).await.unwrap();
        let (proxied_stream, listener) = accept_handle.await.unwrap();

        let fwd = Arc::clone(fwd);
        let fwd_handle = tokio::spawn(async move {
            fwd.forward(
                proxied_stream,
                SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, fwd.port().into())),
                target_port,
                fwd.server_certificate().to_owned(),
            )
            .await;
            debug!("dropping moved listener {}", listener.local_addr().unwrap());
        });
        (cli_stream, fwd_handle)
    }

    /// Start a TCP echo server that serves only one request then stops
    async fn echo_server(port: u16) {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        let (mut socket, _) = listener.accept().await.unwrap();
        let (mut r, mut w) = socket.split();
        tokio::io::copy(&mut r, &mut w).await.unwrap();
        w.flush().await.unwrap();
    }

    async fn create_and_start_forwarder(port: u16) -> (Arc<Forwarder>, JoinHandle<()>) {
        let fwd = Arc::new(Forwarder::new(port));

        let srv_handle = {
            let fwd = Arc::clone(&fwd);
            tokio::spawn(async move {
                fwd.run_quic_server(&Shutdown::new()).await;
            })
        };
        (fwd, srv_handle)
    }

    /// Assert that the bytes written to the stream are echoed back
    async fn assert_echo(stream: &mut TcpStream, length: usize) {
        let bytes = &(0..length).map(|v| (v % 255) as u8).collect::<Vec<u8>>();
        let mut read_buf = vec![0; length];
        stream.write_all(bytes).await.unwrap();
        stream.read_exact(&mut read_buf).await.unwrap();
        assert_eq!(bytes, read_buf.as_slice());
    }

    #[tokio::test]
    async fn test_single_target() {
        let avail_ports = test::available_ports(3).await;
        let echo_srv_port = avail_ports[0];
        let fwd_quic_port = avail_ports[1];
        let cli_proxy_port = avail_ports[2];
        let echo_srv_handle = tokio::spawn(echo_server(echo_srv_port));
        let (fwd, fwd_srv_handle) = create_and_start_forwarder(fwd_quic_port).await;
        let (mut cli_stream, fwd_handle) =
            simulate_proxied_connect(cli_proxy_port, &fwd, echo_srv_port).await;

        // Try big and small writes to check whether bytes are properly flushed
        // through the proxies
        assert_echo(&mut cli_stream, 4).await;
        assert_echo(&mut cli_stream, 10000).await;
        assert_echo(&mut cli_stream, 4).await;

        // cleanup
        echo_srv_handle.abort();
        fwd_srv_handle.abort();
        fwd_handle.abort();
    }

    #[tokio::test]
    async fn test_multiple_targets() {
        chappy_util::init_tracing("test");
        let nb_target = 20;
        let avail_ports = test::available_ports(2 * nb_target + 1).await;
        let echo_srv_ports = &avail_ports[0..nb_target];
        let cli_proxy_ports = &avail_ports[nb_target..2 * nb_target];
        let fwd_quic_port = avail_ports[2 * nb_target];
        let (fwd, fwd_srv_handle) = create_and_start_forwarder(fwd_quic_port).await;

        let mut cli_streams = futures::stream::iter(0..nb_target)
            .then(|t| {
                let echo_srv_port = echo_srv_ports[t];
                let cli_proxy_port = cli_proxy_ports[t];
                let fwd = Arc::clone(&fwd);
                async move {
                    let echo_srv_handle = tokio::spawn(echo_server(echo_srv_port));
                    let (cli_stream, fwd_handle) =
                        simulate_proxied_connect(cli_proxy_port, &fwd, echo_srv_port).await;
                    (cli_stream, echo_srv_handle, fwd_handle)
                }
            })
            .collect::<Vec<_>>()
            .await;

        // Mix writes of different sizes to different targets
        let mut targets: Vec<usize> = (1..nb_target * 3).map(|t| t % nb_target).collect();
        targets.shuffle(&mut rand::thread_rng());
        for target in targets {
            let length = rand::random::<usize>() % 12000;
            info!("writing {} bytes to target {}", length, target);
            assert_echo(&mut cli_streams[target].0, length).await;
        }

        // cleanup
        fwd_srv_handle.abort();
        for i in 0..nb_target {
            cli_streams[i].1.abort();
            cli_streams[i].2.abort();
        }
    }

    #[tokio::test]
    async fn test_target_dropped() {
        // chappy_util::init_tracing("test");
        let avail_ports = test::available_ports(3).await;
        let echo_srv_port = avail_ports[0];
        let fwd_quic_port = avail_ports[1];
        let cli_proxy_port = avail_ports[2];
        let echo_srv_handle = tokio::spawn(echo_server(echo_srv_port));
        let (fwd, fwd_srv_handle) = create_and_start_forwarder(fwd_quic_port).await;
        let (mut cli_stream, fwd_handle) =
            simulate_proxied_connect(cli_proxy_port, &fwd, echo_srv_port).await;

        // Interrupt the target server in the middle of the communication
        assert_echo(&mut cli_stream, 10).await;
        echo_srv_handle.abort();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let bytes = &[1u8, 2, 3, 4];
        let mut read_buf = vec![0; bytes.len()];
        // the first write is expected to succeed because it is aknowledged by
        // the first proxy that cannot know yet that the target is disconnected.
        cli_stream.write_all(bytes).await.unwrap();
        cli_stream.flush().await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        // this write succeeds also, but ideally by this time the stream should
        // know that the connection is broken
        cli_stream.write_all(bytes).await.unwrap();
        cli_stream
            .read_exact(&mut read_buf)
            .await
            .expect_err("read from aborted target should not succeed");

        // cleanup
        fwd_srv_handle.abort();
        fwd_handle.abort();
    }

    #[tokio::test]
    async fn test_try_target_existing() {
        let avail_ports = test::available_ports(2).await;
        let echo_srv_port = avail_ports[0];
        let fwd_quic_port = avail_ports[1];
        let echo_srv_handle = tokio::spawn(echo_server(echo_srv_port));
        let (fwd, fwd_srv_handle) = create_and_start_forwarder(fwd_quic_port).await;
        let tgt_fwd_addr =
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, fwd.port().into()));
        fwd.try_target(
            tgt_fwd_addr,
            echo_srv_port,
            fwd.server_certificate().to_owned(),
        )
        .await
        .unwrap();
        fwd_srv_handle.abort();
        echo_srv_handle.abort();
    }

    #[tokio::test]
    async fn test_try_target_missing() {
        let avail_ports = test::available_ports(2).await;
        let echo_srv_port = avail_ports[0];
        let fwd_quic_port = avail_ports[1];
        // here the echo server is not started
        let (fwd, fwd_srv_handle) = create_and_start_forwarder(fwd_quic_port).await;
        let tgt_fwd_addr =
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, fwd.port().into()));
        fwd.try_target(
            tgt_fwd_addr,
            echo_srv_port,
            fwd.server_certificate().to_owned(),
        )
        .await
        .expect_err("should detect that target isn't running");
        fwd_srv_handle.abort();
    }
}
