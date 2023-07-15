use core::panic;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{debug, error};

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

/// Async copy then shutdown writer.
pub async fn copy<R, W>(mut reader: R, mut writer: W) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    match tokio::io::copy(&mut reader, &mut writer).await {
        Ok(bytes_read) => {
            debug!(bytes_read, "completed");
            writer.shutdown().await?;
            Ok(())
        }
        Err(err) => {
            error!(%err, "error while copying");
            Err(err)
        }
    }
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
