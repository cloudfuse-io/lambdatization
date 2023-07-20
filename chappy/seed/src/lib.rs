mod seed {
    tonic::include_proto!("seed");
}

pub use seed::*;
mod address_stream;
mod cluster_manager;
mod registered_endpoints;
pub mod seed_service;

use std::{net::SocketAddr, str::FromStr};

/// Address conversion newtype
pub struct AddressConv(pub Address);

impl From<AddressConv> for SocketAddr {
    fn from(addr: AddressConv) -> Self {
        SocketAddr::from_str(&format!("{}", addr)).unwrap()
    }
}

impl std::fmt::Display for AddressConv {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}:{}", self.0.ip, self.0.port)
    }
}
