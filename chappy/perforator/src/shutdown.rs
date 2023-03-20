use std::time::Duration;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tonic::async_trait;
use tracing::{info, warn};

pub struct ShutdownGuard {
    _guard: mpsc::Sender<()>,
}

pub struct Shutdown {
    guard: mpsc::Sender<()>,
    waiter: mpsc::Receiver<()>,
}

impl Shutdown {
    pub fn new() -> Self {
        let (guard, waiter) = mpsc::channel(1);
        Self { guard, waiter }
    }

    pub fn create_guard(&self) -> ShutdownGuard {
        ShutdownGuard {
            _guard: self.guard.clone(),
        }
    }

    pub async fn wait(self) {
        let Self { guard, mut waiter } = self;
        drop(guard);
        let _ = waiter.recv().await;
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
