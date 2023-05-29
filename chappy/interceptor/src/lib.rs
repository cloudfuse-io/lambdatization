mod bindings;
mod conf;
mod debug_fmt;
mod utils;

#[macro_use]
extern crate lazy_static;

pub use bindings::connect;

lazy_static! {
    pub(crate) static ref RUNTIME: tokio::runtime::Runtime =
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
    pub(crate) static ref LIBC_LOADED: libloading::Library =
        unsafe { libloading::Library::new("/lib/x86_64-linux-gnu/libc.so.6").unwrap() };
}
