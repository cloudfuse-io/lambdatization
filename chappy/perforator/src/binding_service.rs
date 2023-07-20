use crate::CHAPPY_CONF;
use chappy_seed::NodeBindingResponse;
use chappy_seed::{
    seed_client::SeedClient, ClientBindingRequest, ClientBindingResponse, NodeBindingRequest,
    ServerBindingRequest, ServerPunchRequest,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::os::fd::AsRawFd;
use std::time::Duration;
use tokio::net::TcpSocket;
use tokio::sync::mpsc;
use tokio::sync::OnceCell;
use tokio::task::JoinHandle;
use tonic::transport::{Channel, Endpoint, Uri};
use tonic::{Response, Status, Streaming};
use tower::service_fn;
use tracing::{debug, error, instrument, Instrument};

pub struct BindingService {
    p2p_port: u16,
    client_cell: OnceCell<SeedClient<Channel>>,
}

pub struct NodeBindingHandle(
    JoinHandle<Result<Response<NodeBindingResponse>, Status>>,
    mpsc::Sender<NodeBindingRequest>,
);

impl NodeBindingHandle {
    pub async fn close(self) {
        let NodeBindingHandle(handle, sender) = self;
        drop(sender);
        match handle.await {
            Ok(Ok(_)) => debug!("node binding closed"),
            err => error!(?err, "node binding failed to close"),
        }
    }
}

impl BindingService {
    pub fn new(p2p_port: u16) -> Self {
        Self {
            p2p_port,
            client_cell: OnceCell::new(),
        }
    }

    #[instrument(name = "conn_seed")]
    async fn connect_seed(src_port: u16) -> SeedClient<Channel> {
        let channel = Endpoint::from_shared(format!(
            "http://{}:{}",
            CHAPPY_CONF.seed_hostname, CHAPPY_CONF.seed_port
        ))
        .unwrap()
        .connect_timeout(Duration::from_secs(1))
        .connect_with_connector(service_fn(move |uri: Uri| {
            // see https://github.com/hyperium/tonic/blob/master/examples/src/uds/client.rs
            let sock = TcpSocket::new_v4().unwrap();
            debug!("created socket {}", sock.as_raw_fd());
            sock.set_reuseport(true).unwrap();
            sock.bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), src_port))
                .unwrap();
            let socket_addr = uri
                .authority()
                .unwrap()
                .as_str()
                .to_socket_addrs()
                .unwrap_or_else(|_| panic!("Error solving {}:", uri))
                .next()
                .unwrap();
            debug!(
                "connecting sock {} to {}",
                sock.as_raw_fd(),
                socket_addr.ip()
            );
            sock.connect(socket_addr)
        }))
        .await
        .expect("Connection to seed failed");
        SeedClient::new(channel)
    }

    async fn client(&self) -> SeedClient<Channel> {
        self.client_cell
            .get_or_init(|| Self::connect_seed(self.p2p_port))
            .await
            .clone()
    }

    pub async fn bind_node(&self) -> NodeBindingHandle {
        debug!("call seed to bind node");
        let (tx, rx) = mpsc::channel::<NodeBindingRequest>(1);

        let mut client = self.client().await;
        // don't use a gracefull spawn here as we manually close the handle
        let handle = tokio::spawn(
            async move {
                client
                    .bind_node(tokio_stream::wrappers::ReceiverStream::new(rx))
                    .await
            }
            .instrument(tracing::Span::current()),
        );
        tx.send(NodeBindingRequest {
            cluster_id: CHAPPY_CONF.cluster_id.clone(),
            source_virtual_ip: CHAPPY_CONF.virtual_ip.clone(),
            cluster_size: CHAPPY_CONF.cluster_size,
        })
        .await
        .unwrap();
        NodeBindingHandle(handle, tx)
    }

    pub async fn bind_client(&self, target_virtual_ip: String) -> ClientBindingResponse {
        debug!("call seed to bind client");
        let resp = self
            .client()
            .await
            .bind_client(ClientBindingRequest {
                cluster_id: CHAPPY_CONF.cluster_id.clone(),
                source_virtual_ip: CHAPPY_CONF.virtual_ip.clone(),
                target_virtual_ip,
            })
            .await;
        if let Err(err) = &resp {
            error!(%err,"cli binding failed");
        }
        resp.unwrap().into_inner()
    }

    pub async fn bind_server(&self, server_certificate: Vec<u8>) -> Streaming<ServerPunchRequest> {
        debug!("call seed to bind server");
        self.client()
            .await
            .bind_server(ServerBindingRequest {
                cluster_id: CHAPPY_CONF.cluster_id.clone(),
                virtual_ip: CHAPPY_CONF.virtual_ip.clone(),
                server_certificate,
            })
            .await
            .unwrap()
            .into_inner()
    }
}
