use env_logger;
use futures::stream::StreamExt;
use futures::Stream;
use log::debug;
use seed::{
    seed_server::{Seed, SeedServer},
    Address, ClientPunchRequest, ClientPunchResponse, RegisterRequest, ServerPunchRequest,
};
use std::{env, net::SocketAddr, pin::Pin};
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
        let guard = self.server_nated_addr.lock().await;
        let dst_nated_addr = guard.as_ref().unwrap();
        debug!(
            "corresponding natted tuple {} -> {}",
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
        let stream = UnboundedReceiverStream::new(req_rx).map(|addr| {
            Ok(ServerPunchRequest {
                client_nated_addr: Some(addr),
            })
        });
        Ok(Response::new(Box::pin(stream)))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    debug!("Starting seed...");
    let service = SeedService::new();
    let port = env::var("PORT").unwrap();
    Server::builder()
        .add_service(SeedServer::new(service))
        .serve(format!("0.0.0.0:{}", port).parse()?)
        .await
        .unwrap();

    Ok(())
}
