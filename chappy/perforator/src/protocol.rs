use std::net::Ipv4Addr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

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
    ServerRegistration {
        registered_port: u16,
    },
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
            let registered_port = stream.read_u16().await.unwrap();
            Self::ServerRegistration { registered_port }
        } else {
            Self::Raw(stream)
        }
    }
}

pub async fn register_client(
    perforator_address: &str,
    source_port: u16,
    target_virtual_ip: Ipv4Addr,
    target_port: u16,
) {
    let mut stream = TcpStream::connect(perforator_address).await.unwrap();
    stream
        .write_all(&REGISTER_CLIENT_HEADER_BYTES)
        .await
        .unwrap();
    stream.write_u16(source_port).await.unwrap();
    stream.write_u32(target_virtual_ip.into()).await.unwrap();
    stream.write_u16(target_port).await.unwrap();
    stream.flush().await.unwrap();
    stream.read_u8().await.unwrap_err();
}

pub async fn register_server(perforator_address: &str, registered_port: u16) {
    let mut stream = TcpStream::connect(perforator_address).await.unwrap();
    stream
        .write_all(&REGISTER_SERVER_HEADER_BYTES)
        .await
        .unwrap();
    stream.write_u16(registered_port).await.unwrap();
    stream.flush().await.unwrap();
    stream.read_u8().await.unwrap_err();
}
