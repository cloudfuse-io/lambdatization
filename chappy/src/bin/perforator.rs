use chappy::{quic_utils, seed_client, REGISTER_MAGIC_BYTES, REGISTER_MAGIC_LENGTH};
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

async fn register(mut stream: TcpStream, mappings: Mappings) {
    let source_port = stream.read_u16().await.unwrap();
    let target_virtual_ip: Ipv4Addr = stream.read_u32().await.unwrap().into();
    let target_port = stream.read_u16().await.unwrap();
    debug!(
        "Registering source port mapping {}->{}:{}",
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

async fn forward(stream: TcpStream, mappings: Mappings) {
    let src_port = stream.peer_addr().unwrap().port();
    debug!("Forwarding source port {}", src_port);
    let quic_bi;
    loop {
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
        tokio::io::copy(&mut tcp_read, &mut quic_send)
            .await
            .unwrap()
    });
    let in_handle = tokio::spawn(async move {
        tokio::io::copy(&mut quic_recv, &mut tcp_write)
            .await
            .unwrap()
    });
    out_handle.await.unwrap();
    in_handle.await.unwrap();
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:5000").await.unwrap();
    let mappings: Mappings = Arc::new(Mutex::new(HashMap::new()));
    loop {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mappings = Arc::clone(&mappings);
        tokio::spawn(async move {
            let mut buff = [0; REGISTER_MAGIC_LENGTH];
            stream.peek(&mut buff).await.unwrap();
            if buff == REGISTER_MAGIC_BYTES {
                stream.read_exact(&mut buff).await.unwrap();
                register(stream, mappings).await;
            } else {
                forward(stream, mappings).await;
            }
        });
    }
}
