use crate::metrics::meter;
use crate::shutdown::{Cancelled, ShutdownGuard};
use std::future::Future;
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::{Instrument, Span};

pub fn spawn_task<T>(
    shutdown_guard: ShutdownGuard,
    span: Span,
    future: T,
) -> JoinHandle<Result<T::Output, Cancelled>>
where
    T: Future + Send + 'static,
    T::Output: Send + 'static,
{
    tokio::spawn(meter(
        shutdown_guard
            .run_cancellable(future, Duration::from_millis(50))
            .instrument(span),
    ))
}
