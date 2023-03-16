use crate::{
    seed_server::Seed, Address, ClientBindingRequest, ClientBindingResponse, ServerBindingRequest,
    ServerPunchRequest,
};
use futures::stream::{Stream, StreamExt};
use std::{collections::HashMap, net::SocketAddr, pin::Pin, sync::Arc, time::Duration};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::{Request, Response, Result, Status};
use tracing::{debug, info, instrument};

#[derive(PartialEq, Eq, Hash, Debug)]
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
type RegisteredEndpoints = Arc<Mutex<HashMap<VirtualTarget, ResolvedTarget>>>;

pub struct SeedService {
    registered_endpoints: RegisteredEndpoints,
}

impl SeedService {
    pub fn new() -> Self {
        Self {
            registered_endpoints: Arc::new(Mutex::new(HashMap::new())),
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
        let src_nated_addr = req.remote_addr().unwrap().to_string();
        let src_nated_addr: SocketAddr = src_nated_addr.parse().unwrap();
        let resolved_target;
        loop {
            // TODO: add timeout and replace polling with notification mechanism
            let mut guard = self.registered_endpoints.lock().await;
            let key = VirtualTarget {
                ip: tgt_ip.clone(),
                cluster_id: req.get_ref().cluster_id.clone(),
            };
            if let Some(dst) = guard.get(&key) {
                if dst.punch_req_stream.is_closed() {
                    info!("Cleanup closed target {:?}", key);
                    guard.remove(&key);
                } else {
                    resolved_target = dst.clone();
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
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

        self.registered_endpoints.lock().await.insert(
            VirtualTarget {
                ip: registered_ip.clone(),
                cluster_id: req.get_ref().cluster_id.clone(),
            },
            resolved_target,
        );
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
