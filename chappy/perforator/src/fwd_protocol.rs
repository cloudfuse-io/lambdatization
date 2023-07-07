use core::panic;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{debug, error, warn};

use chappy_util::timed_poll::timed_poll;

#[derive(Clone, Debug, PartialEq)]
pub struct InitQuery {
    pub target_port: u16,
    pub connect_only: bool,
}

impl InitQuery {
    pub async fn read<R: AsyncRead + Unpin>(recv: &mut R) -> Self {
        let target_port = recv.read_u16().await.unwrap();
        let connect_only = match recv.read_u8().await.unwrap() {
            1 => true,
            0 => false,
            _ => panic!("expect 0 or 1"),
        };
        Self {
            target_port,
            connect_only,
        }
    }

    pub async fn write<W: AsyncWrite + Unpin>(self, send: &mut W) {
        send.write_u16(self.target_port).await.unwrap();
        send.write_u8(u8::from(self.connect_only)).await.unwrap();
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct InitResponse {
    pub code: u8,
}

impl InitResponse {
    pub async fn read<R: AsyncRead + Unpin>(recv: &mut R) -> Self {
        let code = recv.read_u8().await.unwrap();
        InitResponse { code }
    }

    pub async fn write<W: AsyncWrite + Unpin>(self, send: &mut W) {
        send.write_u8(self.code).await.unwrap();
    }
}

/// Async copy then shutdown writer. Silently catch disconnections.
pub async fn copy<R, W>(mut reader: R, mut writer: W) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    timed_poll("copy", async move {
        let mut buf = vec![0u8; 4096].into_boxed_slice();
        let mut bytes_read = 0;
        let mut nb_read = 0;
        // Note: Using tokio::io::copy here was not flushing the stream eagerly
        // enough, which was leaving some application low data volumetry
        // connections hanging.
        loop {
            let read_res = reader.read(&mut buf).await;
            match read_res {
                Ok(0) => {
                    debug!(bytes_read, nb_read, "completed");
                    if let Err(err) = writer.shutdown().await {
                        warn!(%err, "writer shutdown failed");
                    } else {
                        debug!("writer shut down");
                    }
                    break Ok(());
                }
                Ok(b) => match writer.write_all(&buf[0..b]).await {
                    Ok(()) => {
                        bytes_read += b;
                        nb_read += 1;
                        // TODO: this systematic flushing might be inefficient, but
                        // is required to ensure proper forwarding of streams with
                        // small data exchanges. Maybe an improved heuristic could
                        // be applied.
                        if let Err(err) = writer.flush().await {
                            error!(%err, "flush failure");
                            break Err(err);
                        }
                    }
                    Err(err) => {
                        error!(%err, "write failure");
                        break Err(err);
                    }
                },
                Err(err) => {
                    error!(%err, "read failure");
                    if let Err(err) = writer.shutdown().await {
                        warn!(%err, "writer shutdown also failed");
                    } else {
                        debug!("writer shut down");
                    }
                    break Err(err);
                }
            };
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn query_roundtrip() {
        let original = InitQuery {
            target_port: 80,
            connect_only: true,
        };
        let mut buf = vec![];
        original.clone().write(&mut buf).await;
        let result = InitQuery::read(&mut buf.as_slice()).await;
        assert_eq!(original, result);
    }

    #[tokio::test]
    async fn response_roundtrip() {
        let original = InitResponse { code: 1 };
        let mut buf = vec![];
        original.clone().write(&mut buf).await;
        let result = InitResponse::read(&mut buf.as_slice()).await;
        assert_eq!(original, result);
    }
}
