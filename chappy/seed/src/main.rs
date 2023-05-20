use chappy_seed::{seed_server::SeedServer, seed_service::SeedService};
use chappy_util::init_tracing;
use std::env;
use std::time::Duration;
use tokio::signal::unix::{signal, SignalKind};
use tokio::time::timeout;
use tonic::{transport::Server, Result};
use tracing::{debug, info, warn};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing("seed");
    let port = env::var("PORT").unwrap();
    debug!("Starting seed on port {}...", port);
    let (service, task) = SeedService::new();
    Server::builder()
        .add_service(SeedServer::new(service))
        .serve_with_shutdown(format!("0.0.0.0:{}", port).parse()?, async {
            let mut sigterm = signal(SignalKind::terminate()).unwrap();
            let mut sigint = signal(SignalKind::interrupt()).unwrap();
            tokio::select! {
                _ = sigint.recv()=> {
                    info!("SIGINT received, exiting gracefully...")
                }
                _ = sigterm.recv()=> {
                    info!("SIGTERM received, exiting gracefully...")
                }
            };
        })
        .await
        .unwrap();

    match timeout(Duration::from_millis(1000), task.wait()).await {
        Ok(_) => info!("Gracefull shutdown completed"),
        Err(_) => warn!("Grace period elapsed, forcefully shutting down"),
    };
    Ok(())
}
