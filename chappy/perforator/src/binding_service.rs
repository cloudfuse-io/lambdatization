use crate::CHAPPY_CONF;
use chappy_seed::{
    seed_client::SeedClient, ClientBindingRequest, ClientBindingResponse, ServerBindingRequest,
    ServerPunchRequest,
};
use log::debug;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::os::fd::AsRawFd;
use tokio::net::TcpSocket;
use tokio::sync::OnceCell;
use tonic::transport::{Channel, Endpoint, Uri};
use tonic::Streaming;
use tower::service_fn;

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

    async fn connect_seed(source_port: u16) -> SeedClient<Channel> {
        let channel = Endpoint::from_static("http://placeholder")
            .connect_with_connector(service_fn(move |_: Uri| {
                // see https://github.com/hyperium/tonic/blob/master/examples/src/uds/client.rs
                let sock = TcpSocket::new_v4().unwrap();
                sock.set_reuseport(true).unwrap();
                debug!("SO_REUSEPORT=true set on {}", sock.as_raw_fd());
                sock.bind(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                    source_port,
                ))
                .unwrap();
                let url = format!("{}:{}", CHAPPY_CONF.seed_hostname, CHAPPY_CONF.seed_port);
                let socket_addr = url
                    .to_socket_addrs()
                    .expect(&format!("Error solving {}:", url))
                    .next()
                    .unwrap();
                debug!(
                    "Opening TCP connection to seed 0.0.0.0:{} -> {}:{}",
                    source_port,
                    socket_addr.ip(),
                    socket_addr.port()
                );
                sock.connect(socket_addr)
            }))
            .await
            .unwrap();
        return SeedClient::new(channel);
    }

    pub async fn bind_client(&self, target_virtual_ip: String) -> ClientBindingResponse {
        debug!(
            "bind client for connection {} -> {}",
            CHAPPY_CONF.virtual_ip, target_virtual_ip
        );
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
        debug!("bind server virtual addr {}", CHAPPY_CONF.virtual_ip);
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
