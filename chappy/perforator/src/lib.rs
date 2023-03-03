mod conf;
pub mod forwarder;
pub mod protocol;
pub mod quic_utils;
pub mod seed_client;
pub mod udp_utils;

#[macro_use]
extern crate lazy_static;

lazy_static! {
    pub(crate) static ref CHAPPY_CONF: conf::ChappyConf = conf::ChappyConf::load();
}
