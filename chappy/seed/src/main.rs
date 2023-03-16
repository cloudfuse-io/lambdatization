use chappy_seed::{seed_server::SeedServer, seed_service::SeedService};
use chappy_util::CustomTime;
use std::env;
use tonic::{transport::Server, Result};
use tracing::debug;
use tracing_subscriber::EnvFilter;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .with_timer(CustomTime)
        .init();
    let port = env::var("PORT").unwrap();
    debug!("Starting seed on port {}...", port);
    let service = SeedService::new();
    Server::builder()
        .add_service(SeedServer::new(service))
        .serve(format!("0.0.0.0:{}", port).parse()?)
        .await
        .unwrap();

    Ok(())
}
