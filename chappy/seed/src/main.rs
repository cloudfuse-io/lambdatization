use chappy_seed::{seed_server::SeedServer, seed_service::SeedService};
use env_logger;
use log::debug;
use std::env;
use tonic::{transport::Server, Result};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
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
