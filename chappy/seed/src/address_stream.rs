use crate::{Address, ServerPunchRequest};
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{debug, Span};

type PunchRequestResult = Result<ServerPunchRequest, tonic::Status>;

pub struct PunchRequestStream {
    // TODO: Avoid boxing here (nit)
    inner: Pin<Box<dyn Stream<Item = PunchRequestResult> + Send>>,
    parent_span: Span,
}

impl PunchRequestStream {
    pub fn new(recv: UnboundedReceiver<Address>, parent_span: Span) -> Self {
        let span = parent_span.clone();
        let inner = UnboundedReceiverStream::new(recv).map(move |addr| {
            debug!(parent: &span, tgt_nat=%format!("{}:{}", addr.ip, addr.port), "forwarding punch request");
            Ok(ServerPunchRequest {
                client_nated_addr: Some(addr),
            })
        }).boxed();
        Self { inner, parent_span }
    }
}

impl Stream for PunchRequestStream {
    type Item = PunchRequestResult;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(cx)
    }
}

impl Drop for PunchRequestStream {
    fn drop(&mut self) {
        debug!(parent: &self.parent_span, "PunchRequestStream dropped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_manager() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        let _stream = UnboundedReceiverStream::new(rx);
        assert!(!tx.is_closed());
    }
}
