use env_logger;
use futures::stream::StreamExt;
use futures::Stream;
use log::debug;
use seed::{
    seed_server::{Seed, SeedServer},
    Address, ClientPunchRequest, ClientPunchResponse, RegisterRequest, ServerPunchRequest,
};
use std::{env, net::SocketAddr, pin::Pin, time::Duration};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::{transport::Server, Request, Response, Result, Status};

mod seed {
    use tonic;
    tonic::include_proto!("seed");
}

struct SeedService {
    req_rx: Mutex<Option<mpsc::UnboundedReceiver<Address>>>,
    server_nated_addr: Mutex<Option<SocketAddr>>,
    req_tx: mpsc::UnboundedSender<Address>,
}

impl SeedService {
    pub fn new() -> Self {
        let (req_tx, req_rx) = mpsc::unbounded_channel();
        Self {
            req_rx: Mutex::new(Some(req_rx)),
            server_nated_addr: Mutex::new(None),
            req_tx,
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
        let dst_nated_addr;
        loop {
            // TODO: add timeout and replace polling with notification mechanism
            let guard = self.server_nated_addr.lock().await;
            if guard.is_some() {
                dst_nated_addr = guard.as_ref().unwrap().clone();
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        debug!(
            "corresponding NATed tuple {} -> {}",
            src_nated_addr, dst_nated_addr
        );
        self.req_tx
            .send(Address {
                ip: src_nated_addr.ip().to_string(),
                port: src_nated_addr.port().try_into().unwrap(),
            })
            .unwrap();
        Ok(Response::new(ClientPunchResponse {
            target_nated_addr: Some(Address {
                ip: dst_nated_addr.ip().to_string(),
                port: dst_nated_addr.port().try_into().unwrap(),
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
        self.server_nated_addr
            .lock()
            .await
            .replace(server_nated_addr);
        let req_rx = self
            .req_rx
            .lock()
            .await
            .take()
            .expect("Receiver already used");
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

#[tokio::main]
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
