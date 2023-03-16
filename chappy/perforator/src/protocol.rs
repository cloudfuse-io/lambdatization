use std::fmt::Display;
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, ToSocketAddrs};

const REGISTER_HEADER_LENGTH: usize = 13;
const REGISTER_CLIENT_HEADER_BYTES: [u8; REGISTER_HEADER_LENGTH] = *b"chappy_client";
const REGISTER_SERVER_HEADER_BYTES: [u8; REGISTER_HEADER_LENGTH] = *b"chappy_server";

#[derive(Debug)]
pub enum ParsedTcpStream {
    ClientRegistration {
        source_port: u16,
        target_virtual_ip: Ipv4Addr,
        target_port: u16,
    },
    ServerRegistration,
    Raw(TcpStream),
}

impl ParsedTcpStream {
    pub async fn from(mut stream: TcpStream) -> Self {
        let mut buff = [0; REGISTER_HEADER_LENGTH];
        stream.peek(&mut buff).await.unwrap();
        if buff == REGISTER_CLIENT_HEADER_BYTES {
            stream.read_exact(&mut buff).await.unwrap();
            let source_port = stream.read_u16().await.unwrap();
            let target_virtual_ip: Ipv4Addr = stream.read_u32().await.unwrap().into();
            let target_port = stream.read_u16().await.unwrap();
            Self::ClientRegistration {
                source_port,
                target_virtual_ip,
                target_port,
            }
        } else if buff == REGISTER_SERVER_HEADER_BYTES {
            stream.read_exact(&mut buff).await.unwrap();
            Self::ServerRegistration
        } else {
            Self::Raw(stream)
        }
    }
}

/// The perforator service might take some time to initialize, so retry the
/// connection for a while
async fn connect_retry<A: ToSocketAddrs + Display>(addr: A) -> TcpStream {
    let start = Instant::now();
    let timeout = Duration::from_secs(3);
    loop {
        if start.elapsed() > timeout {
            panic!("Connection to {} timed out", addr)
        }
        match TcpStream::connect(&addr).await {
            Ok(stream) => return stream,
            Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => {
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            }
            Err(err) => Err(err).unwrap(),
        }
    }
}

pub async fn register_client(
    perforator_address: &str,
    source_port: u16,
    target_virtual_ip: Ipv4Addr,
    target_port: u16,
) {
    let mut stream = connect_retry(perforator_address).await;
    stream
        .write_all(&REGISTER_CLIENT_HEADER_BYTES)
        .await
        .unwrap();
    stream.write_u16(source_port).await.unwrap();
    stream.write_u32(target_virtual_ip.into()).await.unwrap();
    stream.write_u16(target_port).await.unwrap();
    stream.flush().await.unwrap();
    stream
        .read_u8()
        .await
        .expect_err("Connection should have been closed by peer");
}

pub async fn register_server(perforator_address: &str) {
    let mut stream = connect_retry(perforator_address).await;
    stream
        .write_all(&REGISTER_SERVER_HEADER_BYTES)
        .await
        .unwrap();
    stream.flush().await.unwrap();
    stream
        .read_u8()
        .await
        .expect_err("Connection should have been closed by peer");
}
