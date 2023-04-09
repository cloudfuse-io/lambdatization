use chappy_seed::{seed_server::SeedServer, seed_service::SeedService};
use chappy_util::init_tracing;
use std::env;
use tonic::{transport::Server, Result};
use tracing::debug;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing("chappy_seed");
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
