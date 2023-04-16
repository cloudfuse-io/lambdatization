use crate::{
    binding_service::BindingService, forwarder::Forwarder, protocol::ParsedTcpStream,
    shutdown::Shutdown, shutdown::ShutdownGuard, udp_utils,
};
use chappy_seed::Address;

use futures::StreamExt;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, debug_span, instrument, warn, Instrument};

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

#[derive(Clone)]
pub struct Perforator {
    port_mappings: PortMappings,
    address_mappings: AddressMappings,
    forwarder: Arc<Forwarder>,
    binding_service: Arc<BindingService>,
    tcp_port: u16,
}

impl Perforator {
    pub fn new(
        forwarder: Arc<Forwarder>,
        binding_service: Arc<BindingService>,
        tcp_port: u16,
    ) -> Self {
        Self {
            port_mappings: Arc::new(Mutex::new(HashMap::new())),
            address_mappings: Arc::new(Mutex::new(HashMap::new())),
            binding_service,
            forwarder,
            tcp_port,
        }
    }

    #[instrument(name = "reg_cli", skip(self))]
    fn register_client(&self, src_port: u16, tgt_virt: Ipv4Addr, tgt_port: u16) {
        debug!("starting...");
        let virtual_addr = TargetVirtualAddress {
            ip: tgt_virt,
            port: tgt_port,
        };
        {
            let mut guard = self.port_mappings.lock().unwrap();
            let already_registered = guard.values().any(|val| *val == virtual_addr);
            guard.insert(src_port, virtual_addr.clone());
            if already_registered {
                debug!("target virtual IP already registered");
                return;
            }
        }
        // if the client being registered is the first to use this target
        // virtual IP, emit a binding request to the Seed
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
                debug!("client registration completed")
            }
            .instrument(tracing::Span::current()),
        );
        debug!("completed");
    }

    /// Forward a TCP stream from a registered port
    #[instrument(name = "fwd_conn", skip_all)]
    async fn forward_conn(&self, stream: TcpStream, shdn: ShutdownGuard) {
        debug!("starting...");
        let src_port = stream.peer_addr().unwrap().port();
        let target_virtual_address = self
            .port_mappings
            .lock()
            .unwrap()
            .get(&src_port)
            .unwrap_or_else(|| panic!("Source port {} was not registered", src_port))
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
        debug!(
            tgt_nat = format!(
                "{}:{}",
                target_address.natted_address.ip, target_address.natted_address.port
            ),
            tgt_port = target_address.tgt_port,
            "target addr resolved"
        );
        let fwd_fut = self.forwarder.forward(
            stream,
            target_address.natted_address,
            target_address.tgt_port,
            target_address.certificate_der,
        );
        shdn.run_cancellable(fwd_fut, Duration::from_millis(50))
            .await
            .map(|_| debug!("completed"))
            .ok();
    }

    #[instrument(name = "reg_srv", skip_all)]
    fn register_server(&self, shdn: ShutdownGuard) {
        debug!("starting...");
        let p2p_port = self.forwarder.port();
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
                        let client_natted_addr = punch_req.unwrap().client_nated_addr.unwrap();
                        let client_natted_str =
                            format!("{}:{}", client_natted_addr.ip, client_natted_addr.port);
                        udp_utils::send_from_reusable_port(
                            p2p_port,
                            &[1, 2, 3, 4],
                            client_natted_str.clone(),
                        )
                        .instrument(debug_span!("punch hole", tgt_nat = client_natted_str))
                    })
                    .buffer_unordered(usize::MAX)
                    .take_until(shdn.wait_shutdown())
                    .for_each(|_| async {})
                    .await;
                debug!("subscription to hole punching requests closed");
            }
            .instrument(tracing::Span::current()),
        );
        debug!("completed");
    }

    #[instrument(name = "tcp_srv", skip_all)]
    pub async fn run_tcp_server(&self, shutdown: &Shutdown) {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.tcp_port))
            .await
            .unwrap();
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let src_port = stream.peer_addr().unwrap().port();
            let perforator = self.clone();
            let fwd_conn_shdwn_guard = shutdown.create_guard();
            let holepunch_shdwn_guard = shutdown.create_guard();
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
                        ParsedTcpStream::ServerRegistration => {
                            perforator.register_server(holepunch_shdwn_guard)
                        }
                        ParsedTcpStream::Raw(stream) => {
                            perforator.forward_conn(stream, fwd_conn_shdwn_guard).await
                        }
                    }
                }
                .instrument(debug_span!("tcp_conn", src_port)),
            );
        }
    }
}
