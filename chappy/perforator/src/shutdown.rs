use std::time::Duration;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::broadcast;
use tokio::time::timeout;
use tonic::async_trait;
use tracing::{info, warn};

pub struct ShutdownGuard {
    _guard: broadcast::Sender<()>,
    shutdown_notif: broadcast::Receiver<()>,
    is_shutting_down: bool,
}

impl ShutdownGuard {
    /// Check whether the shutdown signal was emitted
    ///
    /// This can be used to check whether a retry loop should be escaped or
    /// continued.
    pub fn is_shutting_down(&mut self) -> bool {
        if self.is_shutting_down {
            return true;
        }
        match self.shutdown_notif.try_recv() {
            Err(broadcast::error::TryRecvError::Empty) => false,
            _ => {
                self.is_shutting_down = true;
                true
            }
        }
    }
}

pub struct Shutdown {
    guard: broadcast::Sender<()>,
    waiter: broadcast::Receiver<()>,
}

impl Shutdown {
    pub fn new() -> Self {
        let (guard, waiter) = broadcast::channel(16);
        Self { guard, waiter }
    }

    /// Emit a guard that will prevent `wait()` to complete until being dropped.
    pub fn create_guard(&self) -> ShutdownGuard {
        ShutdownGuard {
            _guard: self.guard.clone(),
            shutdown_notif: self.guard.subscribe(),
            is_shutting_down: false,
        }
    }

    /// Wait for all `ShutdownGuard`s to be dropped
    ///
    /// Also sends a notification that a shutdown is in progress to all
    /// `ShutdownGuard`s, enabling them to be used to escape retry loops.
    pub async fn wait(self) {
        let Self { guard, mut waiter } = self;
        guard.send(()).unwrap();
        drop(guard);
        while let Ok(_) = waiter.recv().await {}
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
