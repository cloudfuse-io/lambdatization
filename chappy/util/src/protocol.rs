/// Protocol talked between the interceptor and the perforator
use crate::tcp_connect::connect_retry;
use std::io::{Error as IoError, ErrorKind as IoErrorKind, Result as IoResult};
use std::net::Ipv4Addr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const REGISTER_HEADER_LENGTH: usize = 13;
const REGISTER_CLIENT_HEADER_BYTES: [u8; REGISTER_HEADER_LENGTH] = *b"chappy_client";

#[derive(Debug)]
pub enum ParsedTcpStream {
    ClientRegistration {
        source_port: u16,
        target_virtual_ip: Ipv4Addr,
        target_port: u16,
        response_writer: ResponseWriter,
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
                response_writer: ResponseWriter(stream),
            }
        } else {
            Self::Raw(stream)
        }
    }
}

#[derive(Debug)]
pub struct ResponseWriter(TcpStream);

impl ResponseWriter {
    pub async fn write_success(mut self) {
        self.0.write_u8(0).await.unwrap();
        self.0.flush().await.unwrap();
    }

    pub async fn write_failure(mut self) {
        self.0.write_u8(1).await.unwrap();
        self.0.flush().await.unwrap();
    }
}

pub async fn register_client(
    perforator_address: &str,
    source_port: u16,
    target_virtual_ip: Ipv4Addr,
    target_port: u16,
) -> IoResult<()> {
    let mut stream = connect_retry(perforator_address, Duration::from_secs(3)).await?;
    stream.write_all(&REGISTER_CLIENT_HEADER_BYTES).await?;
    stream.write_u16(source_port).await?;
    stream.write_u32(target_virtual_ip.into()).await?;
    stream.write_u16(target_port).await?;
    stream.flush().await?;
    if stream.read_u8().await? > 0 {
        return Err(IoError::new(
            IoErrorKind::AddrNotAvailable,
            anyhow::anyhow!("Perforator could not reach target"),
        ));
    }
    stream
        .read_u8()
        .await
        .expect_err("Connection should have been closed by peer");
    Ok(())
}
