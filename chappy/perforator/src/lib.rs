pub mod binding_service;
mod conf;
pub mod forwarder;
pub mod perforator;
pub mod protocol;
pub mod quic_utils;
pub mod udp_utils;

#[macro_use]
extern crate lazy_static;

lazy_static! {
    pub static ref CHAPPY_CONF: conf::ChappyConf = conf::ChappyConf::load();
}
