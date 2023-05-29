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
        let addr_str = format!("{}:{}", addr.0.ip, addr.0.port);
        SocketAddr::from_str(&addr_str).unwrap()
    }
}
