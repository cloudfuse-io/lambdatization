mod seed {
    tonic::include_proto!("seed");
}

pub use seed::*;
pub mod seed_service;
