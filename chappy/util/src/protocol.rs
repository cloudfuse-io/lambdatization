/// Protocol talked between the interceptor and the perforator
use std::fmt::Display;
use std::io::{ErrorKind as IoErrorKind, Result as IoResult};
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, ToSocketAddrs};

const REGISTER_HEADER_LENGTH: usize = 13;
const REGISTER_CLIENT_HEADER_BYTES: [u8; REGISTER_HEADER_LENGTH] = *b"chappy_client";

#[derive(Debug)]
pub enum ParsedTcpStream {
    ClientRegistration {
        source_port: u16,
        target_virtual_ip: Ipv4Addr,
        target_port: u16,
    },
    Raw,
}

impl ParsedTcpStream {
    pub async fn from(stream: &mut TcpStream) -> Self {
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
        } else {
            Self::Raw
        }
    }
}

/// The perforator service might take some time to initialize, so retry the
/// connection for a while
async fn connect_retry<A: ToSocketAddrs + Display>(addr: A) -> IoResult<TcpStream> {
    let start = Instant::now();
    let timeout = Duration::from_secs(3);
    let mut backoff = 0;
    loop {
        match TcpStream::connect(&addr).await {
            Ok(stream) => return Ok(stream),
            Err(err) if err.kind() == IoErrorKind::ConnectionRefused => {
                if start.elapsed() > timeout {
                    return Err(err);
                }
                tokio::time::sleep(Duration::from_millis(20 + backoff)).await;
                backoff += 5;
                continue;
            }
            Err(err) => return Err(err),
        }
    }
}

pub async fn register_client(
    perforator_address: &str,
    source_port: u16,
    target_virtual_ip: Ipv4Addr,
    target_port: u16,
) -> IoResult<()> {
    let mut stream = connect_retry(perforator_address).await?;
    stream.write_all(&REGISTER_CLIENT_HEADER_BYTES).await?;
    stream.write_u16(source_port).await?;
    stream.write_u32(target_virtual_ip.into()).await?;
    stream.write_u16(target_port).await?;
    stream.flush().await?;
    stream
        .read_u8()
        .await
        .expect_err("Connection should have been closed by peer");
    Ok(())
}
