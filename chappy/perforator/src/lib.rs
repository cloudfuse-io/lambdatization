pub mod binding_service;
mod conf;
pub mod forwarder;
pub mod fwd_protocol;
pub mod metrics;
pub mod perforator;
pub mod quic_utils;
pub mod shutdown;

#[macro_use]
extern crate lazy_static;

/// The name of all certificates are issued for
pub const SERVER_NAME: &str = "chappy";

/// A fictive name to issue punch connections against
pub const PUNCH_SERVER_NAME: &str = "chappy-punch";

lazy_static! {
    pub static ref CHAPPY_CONF: conf::ChappyConf = conf::ChappyConf::load();
}
