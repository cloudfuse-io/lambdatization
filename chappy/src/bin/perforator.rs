use chappy::{
    quic_utils, seed_client, REGISTER_CLIENT_HEADER_BYTES, REGISTER_HEADER_LENGTH,
    REGISTER_SERVER_HEADER_BYTES,
};
use futures::StreamExt;
use log::debug;
use nix::sys::socket::{self, sockopt};
use quinn::Connection;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::os::fd::AsRawFd;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

type Mappings = Arc<Mutex<HashMap<u16, Connection>>>;

async fn register_client(mut stream: TcpStream, mappings: Mappings) {
    let source_port = stream.read_u16().await.unwrap();
    let target_virtual_ip: Ipv4Addr = stream.read_u32().await.unwrap().into();
    let target_port = stream.read_u16().await.unwrap();
    debug!(
        "Registering client source port mapping {}->{}:{}",
        source_port, target_virtual_ip, target_port
    );
    stream.write_u8(1).await.unwrap();
    let src_p2p_port = 5001;
    tokio::spawn(async move {
        let addr =
            seed_client::request_punch(src_p2p_port, target_virtual_ip.to_string(), target_port)
                .await
                .target_nated_addr
                .unwrap();
        let socket = std::net::UdpSocket::bind(format!("0.0.0.0:{}", src_p2p_port)).unwrap();
        socket::setsockopt(socket.as_raw_fd(), sockopt::ReusePort, &true).unwrap();
        let mut quic_endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        quic_endpoint.rebind(socket).unwrap();
        quic_endpoint.set_default_client_config(quic_utils::configure_client());
        let quic_con = quic_endpoint
            .connect(
                format!("{}:{}", addr.ip, addr.port).parse().unwrap(),
                "chappy",
            )
            .unwrap()
            .await
            .unwrap();
        mappings.lock().await.insert(source_port, quic_con);
        debug!("QUIC connection registered for source port {}", source_port)
    });
}

async fn forward_client(stream: TcpStream, mappings: Mappings) {
    let src_port = stream.peer_addr().unwrap().port();
    debug!("Forwarding source port {}", src_port);
    let quic_bi;
    loop {
        // TODO: add timeout and replace polling with notification mechanism
        if let Some(conn) = mappings.lock().await.get(&src_port) {
            quic_bi = Some(conn.open_bi().await.unwrap());
            break;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    debug!("QUIC bidirectional stream created");
    let (mut quic_send, mut quic_recv) = quic_bi.unwrap();
    let (mut tcp_read, mut tcp_write) = stream.into_split();
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

async fn register_server(mut stream: TcpStream) {
    let registered_port = stream.read_u16().await.unwrap();
    debug!("Registering server port {}", registered_port);
    stream.write_u8(1).await.unwrap();
    let dest_p2p_port = 5002;

    tokio::spawn(async move {
        let stream = seed_client::register(dest_p2p_port, registered_port).await;
        // For each incoming server punch request, we create a hole punched connection.
        // We then forward that connection to the local listening server.
        stream
            .map(|punch_req| async {
                // holepunch connection
                let socket =
                    std::net::UdpSocket::bind(format!("0.0.0.0:{}", dest_p2p_port)).unwrap();
                socket::setsockopt(socket.as_raw_fd(), sockopt::ReusePort, &true).unwrap();
                let client_nated_addr = punch_req.unwrap().client_nated_addr.unwrap();
                let client_nated_url =
                    format!("{}:{}", client_nated_addr.ip, client_nated_addr.port);
                socket.send_to(&[1, 2, 3, 4], client_nated_url).unwrap();

                // quic server
                let (server_config, _server_cert) = quic_utils::configure_server().unwrap();
                let endpoint =
                    quinn::Endpoint::server(server_config, "0.0.0.0:0".parse().unwrap()).unwrap();
                endpoint.rebind(socket).unwrap();
                let conn = endpoint.accept().await.unwrap().await.unwrap();
                let (mut quic_send, mut quic_recv) = conn.accept_bi().await.unwrap();

                // forwarding connection
                let localhost_url = format!("localhost:{}", registered_port);
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
            })
            .buffer_unordered(usize::MAX)
            .for_each(|_| async {})
            .await
    });
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:5000").await.unwrap();
    let mappings: Mappings = Arc::new(Mutex::new(HashMap::new()));
    loop {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mappings = Arc::clone(&mappings);
        tokio::spawn(async move {
            let mut buff = [0; REGISTER_HEADER_LENGTH];
            stream.peek(&mut buff).await.unwrap();
            if buff == REGISTER_CLIENT_HEADER_BYTES {
                stream.read_exact(&mut buff).await.unwrap();
                register_client(stream, mappings).await;
            } else if buff == REGISTER_SERVER_HEADER_BYTES {
                stream.read_exact(&mut buff).await.unwrap();
                register_server(stream).await;
            } else {
                forward_client(stream, mappings).await;
            }
        });
    }
}
