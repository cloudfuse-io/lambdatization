mod seed {
    tonic::include_proto!("seed");
}

pub use seed::*;
mod cluster_manager;
pub mod seed_service;
