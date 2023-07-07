use crate::metrics::meter;
use crate::shutdown::{Cancelled, ShutdownGuard};
use chappy_util::timed_poll::timed_poll;
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
    let timed_poll = timed_poll("spawn", future);
    let cancealable = shutdown_guard.run_cancellable(timed_poll, Duration::from_millis(50));
    let instrumented = cancealable.instrument(span);
    let metered = meter(instrumented);
    tokio::spawn(metered)
}
