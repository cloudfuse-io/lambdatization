use crate::address_stream::PunchRequestStream;
use crate::cluster_manager::*;
use crate::registered_endpoints::RegisteredEndpoints;
use crate::{
    seed_server::Seed, Address, ClientBindingRequest, ClientBindingResponse, NodeBindingRequest,
    NodeBindingResponse, ServerBindingRequest, ServerPunchRequest,
};
use futures::stream::{Stream, StreamExt};
use std::{pin::Pin, sync::Arc};
use tokio::sync::mpsc;
use tonic::{Request, Response, Result, Status, Streaming};
use tracing::{debug, error, field::Empty as EmptyField, instrument};

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
                registered_endpoints: Arc::new(RegisteredEndpoints::new()),
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
            clust=%req.get_ref().cluster_id,
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

        let resolved_target = self.registered_endpoints.get(tgt_ip, cluster_id).await?;

        debug!(tgt_nat=%resolved_target.natted_address);
        let punch_req_res = resolved_target.punch_req_stream.send(ServerPunchRequest {
            client_nated_addr: Some(Address {
                ip: src_nated_addr.ip().to_string(),
                port: src_nated_addr.port().try_into().unwrap(),
            }),
            client_virtual_ip: src_ip.clone(),
        });
        let failed_punch_request = if let Err(err) = punch_req_res {
            error!(%err, "failed to send punch request");
            true
        } else {
            false
        };

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
            failed_punch_request,
        }))
    }

    #[instrument(
        name = "bind_srv",
        skip_all,
        fields(clust=%req.get_ref().cluster_id,virt=%req.get_ref().virtual_ip, nat=%req.remote_addr().unwrap())
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

        self.registered_endpoints.insert(
            server_nated_addr,
            req_tx,
            &req.get_ref().server_certificate,
            registered_ip,
            cluster_id,
        );

        debug!("request returning");
        self.cluster_manager.send(
            cluster_id.clone(),
            Message::BindServerResponse {
                virt_ip: registered_ip.clone(),
            },
        );
        Ok(Response::new(
            PunchRequestStream::new(req_rx, tracing::Span::current()).boxed(),
        ))
    }

    #[instrument(
        name = "bind_node",
        skip_all,
        fields(clust = EmptyField,src_nat=%req.remote_addr().unwrap())
    )]
    async fn bind_node(
        &self,
        req: Request<Streaming<NodeBindingRequest>>,
    ) -> Result<Response<NodeBindingResponse>, Status> {
        let mut stream = req.into_inner();
        let bind_req = match stream.next().await {
            Some(Ok(res)) => {
                tracing::Span::current().record("clust", &res.cluster_id);
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
        // debug!("waiting for bind node stream to close");
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
            self.cluster_manager
                .get_summary(bind_req.cluster_id.clone())
                .await
        );
        Ok(Response::new(NodeBindingResponse {}))
    }
}
