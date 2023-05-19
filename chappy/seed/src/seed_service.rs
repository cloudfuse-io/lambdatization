use crate::cluster_manager::*;
use crate::{
    seed_server::Seed, Address, ClientBindingRequest, ClientBindingResponse, NodeBindingRequest,
    NodeBindingResponse, ServerBindingRequest, ServerPunchRequest,
};
use chappy_util::awaitable_map::AwaitableMap;
use futures::stream::{Stream, StreamExt};
use std::{net::SocketAddr, pin::Pin, sync::Arc, time::Duration};
use tokio::{sync::mpsc, time::timeout};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::{Request, Response, Result, Status, Streaming};
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
type RegisteredEndpoints = AwaitableMap<VirtualTarget, ResolvedTarget>;

pub struct SeedService {
    registered_endpoints: Arc<RegisteredEndpoints>,
    cluster_manager: Arc<ClusterManager>,
}

#[allow(clippy::new_without_default)]
impl SeedService {
    pub fn new() -> (Self, ClusterManagerTask) {
        let (cluster_manager, task) = ClusterManager::new();
        (
            Self {
                registered_endpoints: Arc::new(AwaitableMap::new()),
                cluster_manager: Arc::new(cluster_manager),
            },
            task,
        )
    }
}

#[tonic::async_trait]
impl Seed for SeedService {
    type BindServerStream = Pin<Box<dyn Stream<Item = Result<ServerPunchRequest, Status>> + Send>>;

    #[instrument(
        name = "bind_cli",
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
        let src_ip = &req.get_ref().source_virtual_ip;
        let cluster_id = &req.get_ref().cluster_id;
        let src_nated_addr = req.remote_addr().unwrap();

        self.cluster_manager.send(
            cluster_id.clone(),
            Message::BindClientStart {
                src_virt_ip: src_ip.clone(),
                tgt_virt_ip: tgt_ip.clone(),
            },
        );

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
                        info!(ip = tgt_ip, cluster_id, "replace closed target");
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

        self.cluster_manager.send(
            cluster_id.clone(),
            Message::BindClientEnd {
                src_virt_ip: src_ip.clone(),
                tgt_virt_ip: tgt_ip.clone(),
            },
        );

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
        name = "bind_srv",
        skip_all,
        fields(virt=%req.get_ref().virtual_ip, nat=%req.remote_addr().unwrap())
    )]
    async fn bind_server(
        &self,
        req: Request<ServerBindingRequest>,
    ) -> Result<Response<Self::BindServerStream>, Status> {
        debug!("new request");
        let server_nated_addr = req.remote_addr().unwrap();
        let registered_ip = &req.get_ref().virtual_ip;
        let cluster_id = &req.get_ref().cluster_id;
        self.cluster_manager.send(
            cluster_id.clone(),
            Message::BindServerStart {
                virt_ip: registered_ip.clone(),
            },
        );

        let (req_tx, req_rx) = mpsc::unbounded_channel();

        let resolved_target = ResolvedTarget {
            natted_address: server_nated_addr,
            punch_req_stream: req_tx,
            server_certificate: req.get_ref().server_certificate.clone(),
        };
        let virtual_target_key = VirtualTarget {
            ip: registered_ip.clone(),
            cluster_id: cluster_id.clone(),
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
        self.cluster_manager.send(
            cluster_id.clone(),
            Message::BindServerResponse {
                virt_ip: registered_ip.clone(),
            },
        );
        Ok(Response::new(Box::pin(stream)))
    }

    #[instrument(
        name = "bind_node",
        skip_all,
        fields(src_nat=%req.remote_addr().unwrap())
    )]
    async fn bind_node(
        &self,
        req: Request<Streaming<NodeBindingRequest>>,
    ) -> Result<Response<NodeBindingResponse>, Status> {
        let mut stream = req.into_inner();
        let bind_req = match stream.next().await {
            Some(Ok(res)) => {
                debug!(virt=%res.source_virtual_ip, "new request");
                res
            }
            Some(Err(err)) => {
                let msg = "Unexpected error in stream";
                error!(%err, msg);
                return Err(Status::invalid_argument(msg));
            }
            None => {
                let msg = "Expected one binding request";
                error!(msg);
                return Err(Status::invalid_argument(msg));
            }
        };
        self.cluster_manager.send(
            bind_req.cluster_id.clone(),
            Message::BindNodeStart {
                cluster_size: bind_req.cluster_size,
                virt_ip: bind_req.source_virtual_ip.clone(),
                time: Message::now(),
            },
        );
        if stream.next().await.is_some() {
            return Err(Status::invalid_argument(
                "Expected only one binding request",
            ));
        }
        // debug!("bind node stream closed, recording node end");
        self.cluster_manager.send(
            bind_req.cluster_id.clone(),
            Message::BindNodeEnd {
                virt_ip: bind_req.source_virtual_ip,
                time: Message::now(),
            },
        );
        debug!(
            "{:?}",
            self.cluster_manager.get_summary(bind_req.cluster_id).await
        );
        // debug!("bind node completed");
        Ok(Response::new(NodeBindingResponse {}))
    }
}
