use crate::CHAPPY_CONF;
use chappy_seed::{
    seed_client::SeedClient, ClientBindingRequest, ClientBindingResponse, ServerBindingRequest,
    ServerPunchRequest,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::os::fd::AsRawFd;
use std::time::Duration;
use tokio::net::TcpSocket;
use tokio::sync::OnceCell;
use tonic::transport::{Channel, Endpoint, Uri};
use tonic::Streaming;
use tower::service_fn;
use tracing::{debug, instrument};

pub struct BindingService {
    client_p2p_port: u16,
    server_p2p_port: u16,
    client_side: OnceCell<SeedClient<Channel>>,
    server_side: OnceCell<SeedClient<Channel>>,
}

impl BindingService {
    pub fn new(client_p2p_port: u16, server_p2p_port: u16) -> Self {
        assert_ne!(client_p2p_port, server_p2p_port);
        Self {
            client_p2p_port,
            server_p2p_port,
            client_side: OnceCell::new(),
            server_side: OnceCell::new(),
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
                .expect(&format!("Error solving {}:", uri))
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
        .unwrap();
        return SeedClient::new(channel);
    }

    pub async fn bind_client(&self, target_virtual_ip: String) -> ClientBindingResponse {
        debug!(virt = CHAPPY_CONF.virtual_ip, "call seed to bind client");
        self.client_side
            .get_or_init(|| Self::connect_seed(self.client_p2p_port))
            .await
            .clone()
            .bind_client(ClientBindingRequest {
                cluster_id: String::from("test"),
                source_virtual_ip: CHAPPY_CONF.virtual_ip.clone(),
                target_virtual_ip: target_virtual_ip,
            })
            .await
            .unwrap()
            .into_inner()
    }

    pub async fn bind_server(&self, server_certificate: Vec<u8>) -> Streaming<ServerPunchRequest> {
        debug!(virt = CHAPPY_CONF.virtual_ip, "call seed to bind server");
        self.server_side
            .get_or_init(|| Self::connect_seed(self.server_p2p_port))
            .await
            .clone()
            .bind_server(ServerBindingRequest {
                cluster_id: String::from("test"),
                virtual_ip: CHAPPY_CONF.virtual_ip.clone(),
                server_certificate,
            })
            .await
            .unwrap()
            .into_inner()
    }
}
