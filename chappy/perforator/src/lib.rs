mod conf;
pub mod quic_utils;
pub mod seed_client;

#[macro_use]
extern crate lazy_static;

lazy_static! {
    pub(crate) static ref CHAPPY_CONF: conf::ChappyConf = conf::ChappyConf::load();
}
