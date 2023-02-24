mod bindings;
mod conf;
mod debug_fmt;
pub mod quic_utils;
pub mod seed_client;
mod utils;

pub const REGISTER_HEADER_LENGTH: usize = 13;
pub const REGISTER_CLIENT_HEADER_BYTES: [u8; REGISTER_HEADER_LENGTH] = *b"chappy_client";
pub const REGISTER_SERVER_HEADER_BYTES: [u8; REGISTER_HEADER_LENGTH] = *b"chappy_server";

#[macro_use]
extern crate lazy_static;

pub use bindings::{bind, connect};

lazy_static! {
    pub(crate) static ref RUNTIME: tokio::runtime::Runtime =
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
    pub(crate) static ref CHAPPY_CONF: conf::ChappyConf = conf::ChappyConf::load();
    pub(crate) static ref VIRTUAL_NET: ipnet::Ipv4Net = CHAPPY_CONF.virtual_subnet.parse().unwrap();
    pub(crate) static ref LIBC_LOADED: libloading::Library =
        unsafe { libloading::Library::new("/lib/x86_64-linux-gnu/libc.so.6").unwrap() };
}
