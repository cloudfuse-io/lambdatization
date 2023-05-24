mod seed {
    tonic::include_proto!("seed");
}

pub use seed::*;
mod address_stream;
mod cluster_manager;
mod registered_endpoints;
pub mod seed_service;
