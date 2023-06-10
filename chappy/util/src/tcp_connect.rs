use std::io::{ErrorKind as IoErrorKind, Result as IoResult};
use std::time::{Duration, Instant};
use tokio::net::{TcpStream, ToSocketAddrs};
use tracing::{error, warn};

/// Due to cloud function provisioning and setup times, the target addresses
/// might not be available right way. This helper helps bridge that gap by
/// retrying a TCP connection for the specified duration.
pub async fn connect_retry<A: ToSocketAddrs>(addr: A, timeout: Duration) -> IoResult<TcpStream> {
    let start = Instant::now();
    let mut backoff = 0;
    let mut first = true;
    loop {
        match TcpStream::connect(&addr).await {
            Ok(stream) => return Ok(stream),
            Err(err) if err.kind() == IoErrorKind::ConnectionRefused => {
                if start.elapsed() > timeout {
                    error!("TCP connection and retries refused");
                    return Err(err);
                }
                if first {
                    warn!("TCP connection refused, retrying with linear backoff...");
                    first = false;
                }
                tokio::time::sleep(Duration::from_millis(20 + backoff)).await;
                backoff += 5;
                continue;
            }
            Err(err) => return Err(err),
        }
    }
}
