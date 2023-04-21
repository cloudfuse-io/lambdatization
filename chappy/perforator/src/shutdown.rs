use futures::{Future, FutureExt};
use std::fmt;
use std::time::Duration;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::watch;
use tokio::time::{sleep, timeout};
use tonic::async_trait;
use tracing::{info, warn};

pub struct ShutdownGuard {
    is_shutdown: watch::Receiver<bool>,
}

impl ShutdownGuard {
    /// Wait for the shutdown signal
    pub async fn wait_shutdown(&mut self) {
        while self.is_shutdown.changed().await.is_ok() {
            if *self.is_shutdown.borrow() {
                break;
            }
        }
    }

    /// Run the provided future until completion or a shutdown notification is
    /// received
    ///
    /// # Arguments
    ///
    /// * `fut` - A future that should be cancelled upon shutdown
    /// * `grace_period` - A grace period to let the future complete
    pub async fn run_cancellable<T>(
        mut self,
        fut: impl Future<Output = T>,
        grace_period: Duration,
    ) -> Result<T, Cancelled> {
        tokio::select! {
            res = fut => Ok(res),
            _ = self.wait_shutdown().then(|_| sleep(grace_period)) => {
                warn!("cancelled by shutdown");
                Err(Cancelled)
            },
        }
    }
}

pub struct Shutdown {
    waiter: watch::Sender<bool>,
    guard: watch::Receiver<bool>,
}

#[allow(clippy::new_without_default)]
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

/// Listen for termination signals and abort the provided runnable
///
/// The future returned by GracefullyRunnable.run is cancelled right away, and
/// we await the shutdown guards to be dropped for the provided grace period.
pub async fn gracefull(runnable: impl GracefullyRunnable, grace_period: Duration) {
    let shutdown = Shutdown::new();
    let mut sigterm = signal(SignalKind::terminate()).unwrap();
    let mut sigint = signal(SignalKind::interrupt()).unwrap();
    // Abort GracefullyRunnable.run right away
    tokio::select! {
        _ = runnable.run(&shutdown) => {}
        _ = sigint.recv()=> {
            info!("SIGINT received, exiting gracefully...")
        }
        _ = sigterm.recv()=> {
            info!("SIGTERM received, exiting gracefully...")
        }
    }
    // Wait for the spawned tasks to release the shutdown guards
    match timeout(grace_period, shutdown.wait()).await {
        Ok(_) => info!("Gracefull shutdown completed"),
        Err(_) => warn!("Grace period elapsed, forcefully shutting down"),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Cancelled;

impl std::error::Error for Cancelled {}

impl fmt::Display for Cancelled {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        "Cancelled by shutdown".fmt(fmt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering::SeqCst},
        Arc,
    };

    #[tokio::test]
    async fn wait_shutdown() {
        let shutdown = Shutdown::new();
        let completed = Arc::new(AtomicBool::new(false));
        let mut guard = shutdown.create_guard();
        let completed_ref = Arc::clone(&completed);
        tokio::spawn(async move {
            guard.wait_shutdown().await;
            completed_ref.store(true, SeqCst)
        });
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(!completed.load(SeqCst));
        shutdown.wait().await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(completed.load(SeqCst));
    }

    #[tokio::test]
    async fn run_cancellable_gracefull() {
        let shutdown = Shutdown::new();
        let completed = Arc::new(AtomicBool::new(false));
        let guard = shutdown.create_guard();
        let completed_ref = Arc::clone(&completed);
        let handle = tokio::spawn(guard.run_cancellable(
            async move {
                tokio::time::sleep(Duration::from_millis(5)).await;
                completed_ref.store(true, SeqCst)
            },
            Duration::from_millis(10),
        ));
        assert!(!completed.load(SeqCst));
        shutdown.wait().await;
        assert!(completed.load(SeqCst));
        handle
            .await
            .unwrap()
            .expect("task expected to complete within grace period");
    }

    #[tokio::test]
    async fn run_cancellable_cancelled() {
        let shutdown = Shutdown::new();
        let completed = Arc::new(AtomicBool::new(false));
        let guard = shutdown.create_guard();
        let completed_ref = Arc::clone(&completed);
        let handle = tokio::spawn(guard.run_cancellable(
            async move {
                tokio::time::sleep(Duration::from_millis(5)).await;
                completed_ref.store(true, SeqCst)
            },
            Duration::ZERO,
        ));
        assert!(!completed.load(SeqCst));
        shutdown.wait().await;
        assert!(!completed.load(SeqCst));
        handle
            .await
            .unwrap()
            .expect_err("task not expected to complete within grace period");
    }
}
