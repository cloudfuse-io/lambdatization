use chappy_seed::{
    seed_server::{Seed, SeedServer},
    Address, ClientPunchRequest, ClientPunchResponse, RegisterRequest, ServerPunchRequest,
};
use env_logger;
use futures::stream::StreamExt;
use futures::Stream;
use log::debug;
use std::{collections::HashMap, env, net::SocketAddr, pin::Pin, sync::Arc, time::Duration};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::{transport::Server, Request, Response, Result, Status};

#[derive(PartialEq, Eq, Hash)]
struct VirtualEndpoint {
    cluster_id: String,
    address: Address,
}

#[derive(Clone)]
struct ResolvedEndpoint {
    pub natted_address: SocketAddr,
    pub punch_req_stream: mpsc::UnboundedSender<Address>,
}

/// Map virtual addresses to the NATed endpoint and punch request stream
type RegisteredEndpoints = Arc<Mutex<HashMap<VirtualEndpoint, ResolvedEndpoint>>>;

struct SeedService {
    // req_rx: Mutex<Option<mpsc::UnboundedReceiver<Address>>>,
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
    type RegisterStream = Pin<Box<dyn Stream<Item = Result<ServerPunchRequest, Status>> + Send>>;

    async fn punch(
        &self,
        req: Request<ClientPunchRequest>,
    ) -> Result<Response<ClientPunchResponse>, Status> {
        let src_addr = req.get_ref().source_virtual_addr.as_ref().unwrap();
        let tgt_addr = req.get_ref().target_virtual_addr.as_ref().unwrap();
        debug!(
            "received punch request, virtual {}:{} -> {}:{}",
            src_addr.ip, src_addr.port, tgt_addr.ip, tgt_addr.port,
        );
        let src_nated_addr: SocketAddr = req.remote_addr().unwrap().to_string().parse().unwrap();
        let endpoint;
        loop {
            // TODO: add timeout and replace polling with notification mechanism
            let guard = self.registered_endpoints.lock().await;
            if let Some(dst) = guard.get(&VirtualEndpoint {
                address: tgt_addr.clone(),
                cluster_id: req.get_ref().cluster_id.clone(),
            }) {
                endpoint = dst.clone();
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        debug!(
            "corresponding NATed tuple {} -> {}",
            src_nated_addr, endpoint.natted_address
        );
        endpoint
            .punch_req_stream
            .send(Address {
                ip: src_nated_addr.ip().to_string(),
                port: src_nated_addr.port().try_into().unwrap(),
            })
            .unwrap();
        Ok(Response::new(ClientPunchResponse {
            target_nated_addr: Some(Address {
                ip: endpoint.natted_address.ip().to_string(),
                port: endpoint.natted_address.port().try_into().unwrap(),
            }),
        }))
    }

    async fn register(
        &self,
        req: Request<RegisterRequest>,
    ) -> Result<Response<Self::RegisterStream>, Status> {
        let server_nated_addr = req.remote_addr().unwrap();
        let srv_nated_ip = server_nated_addr.ip().to_string();
        let srv_nated_port = server_nated_addr.port();
        let registered_addr = req.get_ref().virtual_addr.as_ref().unwrap().clone();
        debug!(
            "received register request of virtual addr {}:{} to NATed addr {}:{}",
            registered_addr.ip, registered_addr.port, srv_nated_ip, srv_nated_port,
        );

        let (req_tx, req_rx) = mpsc::unbounded_channel();

        let endpoint = ResolvedEndpoint {
            natted_address: server_nated_addr,
            punch_req_stream: req_tx,
        };

        self.registered_endpoints.lock().await.insert(
            VirtualEndpoint {
                address: registered_addr,
                cluster_id: req.get_ref().cluster_id.clone(),
            },
            endpoint,
        );

        let stream = UnboundedReceiverStream::new(req_rx).map(move |addr| {
            debug!(
                "forwarding punch request to srv {}:{} for client {}:{} (NATed addresses)",
                srv_nated_ip, srv_nated_port, addr.ip, addr.port,
            );
            Ok(ServerPunchRequest {
                client_nated_addr: Some(addr),
            })
        });
        Ok(Response::new(Box::pin(stream)))
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();
    let port = env::var("PORT").unwrap();
    debug!("Starting seed on port {}...", port);
    let service = SeedService::new();
    Server::builder()
        .add_service(SeedServer::new(service))
        .serve(format!("0.0.0.0:{}", port).parse()?)
        .await
        .unwrap();

    Ok(())
}
