use futures::Future;
use std::fmt;
use std::time::Duration;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::watch;
use tokio::time::timeout;
use tonic::async_trait;
use tracing::{info, warn};

pub struct ShutdownGuard {
    is_shutdown: watch::Receiver<bool>,
}

impl ShutdownGuard {
    /// Wait for the shutdown signal then drop the guard
    pub async fn wait_shutdown(mut self) {
        while self.is_shutdown.changed().await.is_ok() {
            if *self.is_shutdown.borrow() {
                break;
            }
        }
    }

    /// Run the provided future until completion or a shutdown notification is received
    pub async fn run_cancellable<T>(self, fut: impl Future<Output = T>) -> Result<T, Interrupted> {
        tokio::select! {
            res = fut => Ok(res),
            _ = self.wait_shutdown() => {
                warn!("interrupted by shutdown");
                Err(Interrupted)
            },
        }
    }
}

pub struct Shutdown {
    waiter: watch::Sender<bool>,
    guard: watch::Receiver<bool>,
}

impl Shutdown {
    pub fn new() -> Self {
        let (waiter, guard) = watch::channel(false);
        Self { guard, waiter }
    }

    /// Emit a guard that will prevent `wait()` to complete until being dropped.
    pub fn create_guard(&self) -> ShutdownGuard {
        ShutdownGuard {
            is_shutdown: self.guard.clone(),
        }
    }

    /// Wait for all `ShutdownGuard`s to be dropped
    ///
    /// Also sends a notification that a shutdown is in progress to all
    /// `ShutdownGuard`s, enabling them to be awaitable.
    pub async fn wait(self) {
        let Self { guard, waiter } = self;
        waiter.send(true).unwrap();
        drop(guard);

        waiter.closed().await
    }
}

#[async_trait]
pub trait GracefullyRunnable {
    async fn run(&self, shutdown: &Shutdown);
}

pub async fn gracefull(runnable: impl GracefullyRunnable) {
    let shutdown = Shutdown::new();
    let mut sigterm = signal(SignalKind::terminate()).unwrap();
    let mut sigint = signal(SignalKind::interrupt()).unwrap();
    tokio::select! {
        _ = runnable.run(&shutdown) => {}
        _ = sigint.recv()=> {
            info!("SIGINT received, exiting gracefully...")
        }
        _ = sigterm.recv()=> {
            info!("SIGTERM received, exiting gracefully...")
        }
    }

    match timeout(Duration::from_secs(1), shutdown.wait()).await {
        Ok(_) => info!("Gracefull shutdown completed"),
        Err(_) => warn!("Grace period elapsed, forcefully shutting down"),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Interrupted;

impl std::error::Error for Interrupted {}

impl fmt::Display for Interrupted {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        "interrupted by shutdown".fmt(fmt)
    }
}
