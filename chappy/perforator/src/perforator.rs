use crate::{forwarder::Forwarder, seed_client, udp_utils};
use chappy_seed::Address;

use futures::StreamExt;
use log::debug;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
struct TargetAddress {
    pub natted_address: Address,
    pub target_port: u16,
}

/// Map source ports to target addresses
type Mappings = Arc<Mutex<HashMap<u16, TargetAddress>>>;

pub struct Perforator {
    mappings: Mappings,
    forwarder: Arc<Forwarder>,
}

impl Perforator {
    pub fn new(forwarder: Forwarder) -> Self {
        Self {
            mappings: Arc::new(Mutex::new(HashMap::new())),
            forwarder: Arc::new(forwarder),
        }
    }

    pub async fn register_client(
        &self,
        source_port: u16,
        target_virtual_ip: Ipv4Addr,
        target_port: u16,
    ) {
        debug!(
            "Registering client source port mapping {}->{}:{}",
            source_port, target_virtual_ip, target_port
        );
        let mappings = Arc::clone(&self.mappings);
        let client_p2p_port = self.forwarder.client_p2p_port();
        tokio::spawn(async move {
            let natted_address = seed_client::request_punch(
                client_p2p_port,
                target_virtual_ip.to_string(),
                target_port,
            )
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

    pub async fn forward_client(&self, stream: TcpStream) {
        let src_port = stream.peer_addr().unwrap().port();
        debug!("Forwarding source port {}", src_port);
        let target_address: TargetAddress;
        loop {
            // TODO: add timeout and replace polling with notification mechanism
            if let Some(addr) = self.mappings.lock().await.get(&src_port) {
                target_address = addr.clone();
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        self.forwarder
            .forward(
                stream,
                target_address.natted_address,
                target_address.target_port,
            )
            .await;
    }

    pub async fn register_server(&self, registered_port: u16) {
        debug!("Registering server port {}", registered_port);
        let server_p2p_port = self.forwarder.server_p2p_port();
        tokio::spawn(async move {
            let stream = seed_client::register(server_p2p_port, registered_port).await;
            // For each incoming server punch request, send a random packet to punch
            // a hole in the NAT
            stream
                .map(|punch_req| async {
                    let client_natted_addr = punch_req.unwrap().client_nated_addr.unwrap();
                    let client_natted_str =
                        format!("{}:{}", client_natted_addr.ip, client_natted_addr.port);
                    udp_utils::send_from_reusable_port(
                        server_p2p_port,
                        &[1, 2, 3, 4],
                        &client_natted_str,
                    );
                })
                .buffer_unordered(usize::MAX)
                .for_each(|_| async {})
                .await
        });
    }
}
