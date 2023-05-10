use crate::{
    seed_server::Seed, Address, ClientBindingRequest, ClientBindingResponse, ServerBindingRequest,
    ServerPunchRequest,
};
use chappy_util::awaitable_map::AwaitableMap;
use futures::stream::{Stream, StreamExt};
use std::{net::SocketAddr, pin::Pin, sync::Arc, time::Duration};
use tokio::{sync::mpsc, time::timeout};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::{Request, Response, Result, Status};
use tracing::{debug, error, info, instrument};

#[derive(PartialEq, Eq, Hash, Clone)]
struct VirtualTarget {
    cluster_id: String,
    ip: String,
}

#[derive(Clone)]
struct ResolvedTarget {
    pub natted_address: SocketAddr,
    pub punch_req_stream: mpsc::UnboundedSender<Address>,
    pub server_certificate: Vec<u8>,
}

/// Map virtual addresses to the NATed endpoint and punch request stream
type RegisteredEndpoints = Arc<AwaitableMap<VirtualTarget, ResolvedTarget>>;

pub struct SeedService {
    registered_endpoints: RegisteredEndpoints,
}

#[allow(clippy::new_without_default)]
impl SeedService {
    pub fn new() -> Self {
        Self {
            registered_endpoints: Arc::new(AwaitableMap::new()),
        }
    }
}

#[tonic::async_trait]
impl Seed for SeedService {
    type BindServerStream = Pin<Box<dyn Stream<Item = Result<ServerPunchRequest, Status>> + Send>>;

    #[instrument(
        level = "debug",
        name = "bind_cli",
        target = "",
        skip_all,
        fields(
            src_virt=%req.get_ref().source_virtual_ip,
            src_nat=%req.remote_addr().unwrap(),
            tgt_virt=%req.get_ref().target_virtual_ip
        )
    )]
    async fn bind_client(
        &self,
        req: Request<ClientBindingRequest>,
    ) -> Result<Response<ClientBindingResponse>, Status> {
        debug!("new request");
        let tgt_ip = &req.get_ref().target_virtual_ip;
        let src_nated_addr = req.remote_addr().unwrap();

        let virtual_target_key = VirtualTarget {
            ip: tgt_ip.clone(),
            cluster_id: req.get_ref().cluster_id.clone(),
        };

        // TODO adjust timout duration
        let resolved_target_timeout = timeout(
            Duration::from_secs(10),
            self.registered_endpoints
                .get(virtual_target_key, |prev_tgt| {
                    if prev_tgt.punch_req_stream.is_closed() {
                        info!(
                            ip = tgt_ip,
                            cluster = req.get_ref().cluster_id,
                            "replace closed target"
                        );
                        true
                    } else {
                        false
                    }
                }),
        )
        .await;

        let resolved_target = if let Ok(target) = resolved_target_timeout {
            target
        } else {
            let msg = "Target ip could not be resolved";
            error!(msg);
            return Err(Status::not_found(msg));
        };

        debug!(tgt_nat=%resolved_target.natted_address);
        resolved_target
            .punch_req_stream
            .send(Address {
                ip: src_nated_addr.ip().to_string(),
                port: src_nated_addr.port().try_into().unwrap(),
            })
            .unwrap();
        debug!("request returning");
        Ok(Response::new(ClientBindingResponse {
            target_nated_addr: Some(Address {
                ip: resolved_target.natted_address.ip().to_string(),
                port: resolved_target.natted_address.port().try_into().unwrap(),
            }),
            server_certificate: resolved_target.server_certificate,
        }))
    }

    #[instrument(
        level = "debug",
        name = "bind_srv",
        target = "",
        skip_all,
        fields(virt=%req.get_ref().virtual_ip, nat=%req.remote_addr().unwrap())
    )]
    async fn bind_server(
        &self,
        req: Request<ServerBindingRequest>,
    ) -> Result<Response<Self::BindServerStream>, Status> {
        debug!("new request");
        let server_nated_addr = req.remote_addr().unwrap();
        let registered_ip = req.get_ref().virtual_ip.clone();

        let (req_tx, req_rx) = mpsc::unbounded_channel();

        let resolved_target = ResolvedTarget {
            natted_address: server_nated_addr,
            punch_req_stream: req_tx,
            server_certificate: req.get_ref().server_certificate.clone(),
        };
        let virtual_target_key = VirtualTarget {
            ip: registered_ip,
            cluster_id: req.get_ref().cluster_id.clone(),
        };

        // replace the new target in the registered endpoint map
        if let Some(prev_tgt) = self
            .registered_endpoints
            .insert(virtual_target_key.clone(), resolved_target)
        {
            if prev_tgt.punch_req_stream.is_closed() {
                info!(
                    ip = virtual_target_key.ip,
                    cluster = virtual_target_key.cluster_id,
                    "replaced closed target"
                );
            } else {
                error!(
                    ip = virtual_target_key.ip,
                    cluster = virtual_target_key.cluster_id,
                    "replaced unclosed target"
                );
            }
        }

        let span = tracing::Span::current();
        let stream = UnboundedReceiverStream::new(req_rx).map(move |addr| {
            debug!(parent: &span, tgt_nat=%format!("{}:{}", addr.ip, addr.port), "forwarding punch request");
            Ok(ServerPunchRequest {
                client_nated_addr: Some(addr),
            })
        });
        debug!("request returning");
        Ok(Response::new(Box::pin(stream)))
    }
}
