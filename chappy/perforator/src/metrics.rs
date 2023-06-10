use std::future::Future;

lazy_static! {
    static ref TASK_MONITOR: tokio_metrics::TaskMonitor = tokio_metrics::TaskMonitor::new();
}

pub fn meter<T>(fut: T) -> impl Future<Output = T::Output> + Send + 'static
where
    T: Future + Send + 'static,
    T::Output: Send + 'static,
{
    TASK_MONITOR.instrument(fut)
}

pub fn print_metrics() {
    tracing::info!("Monitor: {:?}", *TASK_MONITOR);
}
