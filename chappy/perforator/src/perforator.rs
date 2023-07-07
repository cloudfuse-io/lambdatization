use crate::binding_service::NodeBindingHandle;
use crate::spawn::spawn_task;
use crate::{
    binding_service::BindingService, forwarder::Forwarder, shutdown::Shutdown,
    shutdown::ShutdownGuard,
};
use chappy_seed::{Address, AddressConv};
use chappy_util::timed_poll::timed_poll;
use chappy_util::{awaitable_map::AwaitableMap, protocol::ParsedTcpStream};
use futures::StreamExt;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tracing::{debug, debug_span, instrument, Instrument};

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
type PortMappings = Arc<AwaitableMap<u16, TargetVirtualAddress>>;

/// Map virtual addresses to resolved ones
type AddressMappings = Arc<AwaitableMap<TargetVirtualAddress, TargetResolvedAddress>>;

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
            port_mappings: Arc::new(AwaitableMap::new()),
            address_mappings: Arc::new(AwaitableMap::new()),
            binding_service,
            forwarder,
            tcp_port,
        }
    }

    #[instrument(name = "reg_cli", skip(self))]
    async fn register_client(
        &self,
        src_port: u16,
        tgt_virt: Ipv4Addr,
        tgt_port: u16,
    ) -> anyhow::Result<()> {
        debug!("starting...");
        let start = Instant::now();
        let virtual_addr = TargetVirtualAddress {
            ip: tgt_virt,
            port: tgt_port,
        };

        self.port_mappings.insert(src_port, virtual_addr.clone());
        // TODO bind only once per target virtual IP
        let punch_resp = timed_poll(
            "bind_client",
            self.binding_service.bind_client(tgt_virt.to_string()),
        )
        .await;
        let natted_addr = punch_resp.target_nated_addr.unwrap();
        self.address_mappings.insert(
            virtual_addr,
            TargetResolvedAddress {
                natted_address: natted_addr.clone(),
                tgt_port,
                certificate_der: punch_resp.server_certificate.clone(),
            },
        );
        timed_poll(
            "try_target",
            self.forwarder.try_target(
                AddressConv(natted_addr).into(),
                tgt_port,
                punch_resp.server_certificate,
            ),
        )
        .await?;
        debug!(duration = ?start.elapsed(), "completed");
        Ok(())
    }

    /// Forward a TCP stream from a registered port
    #[instrument(name = "fwd_conn", skip_all)]
    async fn forward_conn(&self, stream: TcpStream) {
        debug!("starting...");
        // TODO adjust timeout duration
        let src_port = stream.peer_addr().unwrap().port();
        let target_virtual_address = timeout(
            Duration::from_secs(1),
            self.port_mappings.get(src_port, |_| false),
        )
        .await
        .unwrap();
        // TODO adjust timeout duration
        let target_address = timeout(
            Duration::from_secs(3),
            self.address_mappings.get(target_virtual_address, |_| false),
        )
        .await
        .unwrap();
        let target_nated_addr = AddressConv(target_address.natted_address).into();
        debug!(
            tgt_nat = %target_nated_addr,
            tgt_port = target_address.tgt_port,
            "target addr resolved"
        );
        let fwd_fut = self.forwarder.forward(
            stream,
            target_nated_addr,
            target_address.tgt_port,
            target_address.certificate_der,
        );
        fwd_fut.await;
    }

    #[instrument(name = "reg_node", skip_all)]
    pub async fn bind_node(
        &self,
        mut punch_stream_shdn_guard: ShutdownGuard,
        safety_shdn_guard: ShutdownGuard,
    ) -> NodeBindingHandle {
        debug!("starting...");
        let server_certificate = self.forwarder.server_certificate().to_owned();
        let binding_service = Arc::clone(&self.binding_service);
        let fwd_ref = Arc::clone(&self.forwarder);
        let node_binding = binding_service.bind_node().await;
        spawn_task(safety_shdn_guard, tracing::Span::current(), async move {
            let stream = binding_service.bind_server(server_certificate).await;
            // For each incoming server punch request, send a random packet to punch
            // a hole in the NAT
            stream
                .map(|punch_req| {
                    let punch_req = punch_req.unwrap();
                    let client_natted_addr = punch_req.client_nated_addr.unwrap();
                    fwd_ref.punch_hole(
                        AddressConv(client_natted_addr).into(),
                        punch_req.client_virtual_ip,
                    )
                })
                .buffer_unordered(usize::MAX)
                .take_until(punch_stream_shdn_guard.wait_shutdown())
                .for_each(|_| async {})
                .instrument(debug_span!("punch_stream"))
                .await;
        });
        debug!("completed");
        node_binding
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
            let shutdown_guard = shutdown.create_guard();
            spawn_task(
                shutdown_guard,
                debug_span!("tcp_conn", src_port),
                async move {
                    let parsed_stream = ParsedTcpStream::from(stream).await;
                    match parsed_stream {
                        ParsedTcpStream::ClientRegistration {
                            source_port,
                            target_virtual_ip,
                            target_port,
                            response_writer,
                        } => {
                            let reg_fut = timed_poll(
                                "register_client",
                                perforator.register_client(
                                    source_port,
                                    target_virtual_ip,
                                    target_port,
                                ),
                            );
                            match reg_fut.await {
                                Ok(_) => response_writer.write_success().await,
                                Err(_) => response_writer.write_failure().await,
                            };
                        }
                        ParsedTcpStream::Raw(stream) => {
                            timed_poll("forward_conn", perforator.forward_conn(stream)).await;
                        }
                    }
                },
            );
        }
    }
}
