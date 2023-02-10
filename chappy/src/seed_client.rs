use log::debug;
use seed::{seed_client::SeedClient, Address, ClientPunchRequest, RegisterRequest};
use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::os::fd::AsRawFd;
use tokio::net::TcpSocket;
use tonic::transport::{Channel, Endpoint, Uri};
use tonic::Streaming;
use tower::service_fn;

use self::seed::{ClientPunchResponse, ServerPunchRequest};

mod seed {
    use tonic;
    tonic::include_proto!("seed");
}

pub(crate) async fn connect_seed(source_port: u16) -> SeedClient<Channel> {
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
            let url = format!(
                "{}:{}",
                env::var("SEED_HOSTNAME").unwrap(),
                env::var("SEED_PORT").unwrap()
            );
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
        // .connect()
        .await
        .unwrap();
    return SeedClient::new(channel);
}

pub(crate) async fn request_punch(
    source_port: u16,
    target_virtual_ip: String,
    target_port: u16,
) -> ClientPunchResponse {
    let source_virtual_ip = env::var("CLIENT_VIRTUAL_IP").unwrap();
    debug!(
        "request punch to enable {}:{} -> {}:{}",
        source_virtual_ip, source_port, target_virtual_ip, target_port
    );
    let mut client = connect_seed(source_port).await;
    let resp = client
        .punch(ClientPunchRequest {
            cluster_id: String::from("test"),
            source_virtual_addr: Some(Address {
                ip: source_virtual_ip,
                port: source_port.try_into().unwrap(),
            }),
            target_virtual_addr: Some(Address {
                ip: target_virtual_ip,
                port: target_port.try_into().unwrap(),
            }),
        })
        .await;
    let success = resp.unwrap();
    success.into_inner()
}

pub(crate) async fn register(port: u16) -> Streaming<ServerPunchRequest> {
    let virtual_ip = env::var("SERVER_VIRTUAL_IP").unwrap();
    debug!("register {}:{}", virtual_ip, port);
    let mut client = connect_seed(port).await;
    let resp = client
        .register(RegisterRequest {
            cluster_id: String::from("test"),
            virtual_addr: Some(Address {
                ip: virtual_ip,
                port: port.try_into().unwrap(),
            }),
        })
        .await;
    resp.unwrap().into_inner()
}
