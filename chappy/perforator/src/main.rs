use chappy_perforator::{forwarder::Forwarder, protocol::ParsedTcpStream, seed_client, udp_utils};
use chappy_seed::Address;

use futures::StreamExt;
use log::debug;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
struct TargetAddress {
    pub natted_address: Address,
    pub target_port: u16,
}

type Mappings = Arc<Mutex<HashMap<u16, TargetAddress>>>;

async fn register_client(
    mappings: Mappings,
    source_port: u16,
    target_virtual_ip: Ipv4Addr,
    target_port: u16,
    src_p2p_port: u16,
) {
    debug!(
        "Registering client source port mapping {}->{}:{}",
        source_port, target_virtual_ip, target_port
    );
    tokio::spawn(async move {
        let natted_address =
            seed_client::request_punch(src_p2p_port, target_virtual_ip.to_string(), target_port)
                .await
                .target_nated_addr
                .unwrap();
        mappings.lock().await.insert(
            source_port,
            TargetAddress {
                natted_address,
                target_port,
            },
        );
        debug!("QUIC connection registered for source port {}", source_port)
    });
}

async fn forward_client(stream: TcpStream, mappings: Mappings, forwarder: Arc<Forwarder>) {
    let src_port = stream.peer_addr().unwrap().port();
    debug!("Forwarding source port {}", src_port);
    let target_address: TargetAddress;
    loop {
        // TODO: add timeout and replace polling with notification mechanism
        if let Some(addr) = mappings.lock().await.get(&src_port) {
            target_address = addr.clone();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    forwarder
        .forward(
            stream,
            target_address.natted_address,
            target_address.target_port,
        )
        .await;
}

async fn register_server(registered_port: u16, dest_p2p_port: u16) {
    debug!("Registering server port {}", registered_port);

    tokio::spawn(async move {
        let stream = seed_client::register(dest_p2p_port, registered_port).await;
        // For each incoming server punch request, send a random packet to punch
        // a hole in the NAT
        stream
            .map(|punch_req| async {
                let client_natted_addr = punch_req.unwrap().client_nated_addr.unwrap();
                let client_natted_str =
                    format!("{}:{}", client_natted_addr.ip, client_natted_addr.port);
                udp_utils::send_from_reusable_port(
                    dest_p2p_port,
                    &[1, 2, 3, 4],
                    &client_natted_str,
                );
            })
            .buffer_unordered(usize::MAX)
            .for_each(|_| async {})
            .await
    });
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();
    let listener = TcpListener::bind("127.0.0.1:5000").await.unwrap();
    let mappings: Mappings = Arc::new(Mutex::new(HashMap::new()));
    let forwarder = Arc::new(Forwarder::new(5001, 5002));
    loop {
        let (stream, _) = listener.accept().await.unwrap();
        debug!(
            "New connection to perforator TCP server from {}:{}",
            stream.peer_addr().unwrap().ip(),
            stream.peer_addr().unwrap().port()
        );
        let mappings = Arc::clone(&mappings);
        let forwarder = Arc::clone(&forwarder);
        tokio::spawn(async move {
            let parsed_stream = ParsedTcpStream::from(stream).await;
            match parsed_stream {
                ParsedTcpStream::ClientRegistration {
                    source_port,
                    target_virtual_ip,
                    target_port,
                } => {
                    register_client(
                        mappings,
                        source_port,
                        target_virtual_ip,
                        target_port,
                        forwarder.client_p2p_port(),
                    )
                    .await
                }
                ParsedTcpStream::ServerRegistration { registered_port } => {
                    register_server(registered_port, forwarder.server_p2p_port()).await
                }
                ParsedTcpStream::Raw(stream) => {
                    forward_client(stream, mappings, Arc::clone(&forwarder)).await
                }
            }
        });
    }
}
