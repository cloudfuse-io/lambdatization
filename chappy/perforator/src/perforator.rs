use crate::{
    binding_service::BindingService, forwarder::Forwarder, shutdown::ShutdownGuard, udp_utils,
};
use chappy_seed::Address;

use futures::StreamExt;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;
use tracing::{debug, instrument, Instrument};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct TargetVirtualAddress {
    pub ip: Ipv4Addr,
    pub port: u16,
}

#[derive(Debug, Clone)]
struct TargetResolvedAddress {
    pub natted_address: Address,
    pub tgt_port: u16,
    pub certificate_der: Vec<u8>,
}

/// Map source ports to target virtual addresses
type PortMappings = Arc<Mutex<HashMap<u16, TargetVirtualAddress>>>;

/// Map virtual addresses to resolved ones
type AddressMappings = Arc<Mutex<HashMap<TargetVirtualAddress, TargetResolvedAddress>>>;

pub struct Perforator {
    port_mappings: PortMappings,
    address_mappings: AddressMappings,
    forwarder: Arc<Forwarder>,
    binding_service: Arc<BindingService>,
}

impl Perforator {
    pub fn new(forwarder: Forwarder, binding_service: BindingService) -> Self {
        Self {
            port_mappings: Arc::new(Mutex::new(HashMap::new())),
            address_mappings: Arc::new(Mutex::new(HashMap::new())),
            binding_service: Arc::new(binding_service),
            forwarder: Arc::new(forwarder),
        }
    }

    #[instrument(name = "reg_cli", skip(self))]
    pub fn register_client(&self, src_port: u16, tgt_virt: Ipv4Addr, tgt_port: u16) {
        debug!("new request");
        let virtual_addr = TargetVirtualAddress {
            ip: tgt_virt,
            port: tgt_port,
        };
        {
            let mut guard = self.port_mappings.lock().unwrap();
            let already_registered = guard.values().any(|val| *val == virtual_addr);
            guard.insert(src_port, virtual_addr.clone());
            if already_registered {
                debug!("source port mapping already registered");
                return;
            }
        }
        let address_mappings = Arc::clone(&self.address_mappings);
        let binding_service = Arc::clone(&self.binding_service);
        tokio::spawn(
            async move {
                let punch_resp = binding_service.bind_client(tgt_virt.to_string()).await;
                address_mappings.lock().unwrap().insert(
                    virtual_addr,
                    TargetResolvedAddress {
                        natted_address: punch_resp.target_nated_addr.unwrap(),
                        tgt_port,
                        certificate_der: punch_resp.server_certificate,
                    },
                );
                debug!("QUIC connection registered")
            }
            .instrument(tracing::Span::current()),
        );
    }

    /// Forward a TCP stream from a registered port
    #[instrument(name = "fwd_conn", skip_all)]
    pub async fn forward_conn(&self, stream: TcpStream, _shdn: ShutdownGuard) {
        debug!("starting...");
        let src_port = stream.peer_addr().unwrap().port();
        let target_virtual_address = self
            .port_mappings
            .lock()
            .unwrap()
            .get(&src_port)
            .expect(&format!("Source port {} was not registered", src_port))
            .clone();
        let target_address: TargetResolvedAddress;
        loop {
            // TODO: add timeout and replace polling with notification mechanism
            if let Some(addr) = self
                .address_mappings
                .lock()
                .unwrap()
                .get(&target_virtual_address)
            {
                target_address = addr.clone();
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        self.forwarder
            .forward(
                stream,
                target_address.natted_address,
                target_address.tgt_port,
                target_address.certificate_der,
            )
            .await;
        debug!("completed");
    }

    #[instrument(name = "reg_srv", skip_all)]
    pub fn register_server(&self) {
        debug!("new request");
        let server_p2p_port = self.forwarder.server_p2p_port();
        let server_certificate = self.forwarder.server_certificate().to_owned();
        let binding_service = Arc::clone(&self.binding_service);
        tokio::spawn(
            async move {
                let stream = binding_service.bind_server(server_certificate).await;
                // For each incoming server punch request, send a random packet to punch
                // a hole in the NAT
                debug!("subscribe to hole punching requests");
                stream
                    .map(|punch_req| {
                        async {
                            let client_natted_addr = punch_req.unwrap().client_nated_addr.unwrap();
                            let client_natted_str =
                                format!("{}:{}", client_natted_addr.ip, client_natted_addr.port);
                            udp_utils::send_from_reusable_port(
                                server_p2p_port,
                                &[1, 2, 3, 4],
                                &client_natted_str,
                            );
                            debug!("hole punching to {} performed!", client_natted_str);
                        }
                        .instrument(tracing::Span::current())
                    })
                    .buffer_unordered(usize::MAX)
                    .for_each(|_| async {})
                    .await;
                debug!("subscription to hole punching requests closed");
            }
            .instrument(tracing::Span::current()),
        );
    }
}
