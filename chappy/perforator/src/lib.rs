pub mod binding_service;
mod conf;
pub mod forwarder;
pub mod fwd_protocol;
pub mod perforator;
pub mod quic_utils;
pub mod shutdown;

#[macro_use]
extern crate lazy_static;

/// The name of all certificates are issued for
pub const SERVER_NAME: &str = "chappy";

/// A fictive name to issue punch connections against
pub const PUNCH_SERVER_NAME: &str = "chappy-punch";

use std::future::Future;

lazy_static! {
    pub static ref CHAPPY_CONF: conf::ChappyConf = conf::ChappyConf::load();
    static ref TASK_MONITOR: tokio_metrics::TaskMonitor = tokio_metrics::TaskMonitor::new();
}

pub fn metrics<T>(fut: T) -> impl Future<Output = T::Output> + Send + 'static
where
    T: Future + Send + 'static,
    T::Output: Send + 'static,
{
    TASK_MONITOR.instrument(fut)
}

pub fn print_metrics() {
    tracing::info!("Monitor: {:?}", *TASK_MONITOR);
}
