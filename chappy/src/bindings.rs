use crate::utils;
use env_logger;
use log::debug;
use nix::{
    libc::{c_int, sockaddr, socklen_t},
    sys::socket::SockaddrLike,
};
use std::net::Ipv4Addr;
use std::ptr;
use utils::{parse_virtual, register, request_punch};

fn init_logger() {
    env_logger::try_init().ok();
}

lazy_static! {
    static ref LIBC_LOADED: libloading::Library =
        unsafe { libloading::Library::new("/lib/x86_64-linux-gnu/libc.so.6").unwrap() };
}

type ConnectSymbol<'a> =
    libloading::Symbol<'a, unsafe extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int>;

#[no_mangle]
pub unsafe extern "C" fn connect(sockfd: c_int, addr: *const sockaddr, len: socklen_t) -> c_int {
    init_logger();
    let libc_connect: ConnectSymbol = LIBC_LOADED.get(b"connect").unwrap();
    debug!("Entering interception connect({})", sockfd);
    match parse_virtual(addr, len) {
        Some(addr_in) => {
            let new_addr = request_punch(sockfd, addr_in);
            debug!(
                "Calling libc.connect({}, {}, {}) instead of ({}, {}, {})",
                sockfd,
                Ipv4Addr::from(new_addr.ip()),
                new_addr.port(),
                sockfd,
                Ipv4Addr::from(addr_in.ip()),
                addr_in.port()
            );
            let code = libc_connect(sockfd, ptr::addr_of!(new_addr).cast(), new_addr.len());
            if code == -1 {
                debug!("errno for libc.connect({})", nix::errno::errno())
            }
            code
        }
        None => libc_connect(sockfd, addr, len),
    }
}

type BindSymbol<'a> =
    libloading::Symbol<'a, unsafe extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int>;

#[no_mangle]
pub unsafe extern "C" fn bind(sockfd: c_int, addr: *const sockaddr, len: socklen_t) -> c_int {
    init_logger();
    debug!("Entering interception bind({})", sockfd);
    let libc_bind: BindSymbol = LIBC_LOADED.get(b"bind").unwrap();

    match parse_virtual(addr, len) {
        Some(addr_in) => {
            let new_addr = register(sockfd, addr_in);
            debug!(
                "Calling libc.bind({}, {}, {}) instead of ({}, {}, {})",
                sockfd,
                Ipv4Addr::from(new_addr.ip()),
                new_addr.port(),
                sockfd,
                Ipv4Addr::from(addr_in.ip()),
                addr_in.port()
            );
            let code = libc_bind(sockfd, ptr::addr_of!(new_addr).cast(), new_addr.len());
            if code == -1 {
                debug!("errno for libc.bind({})", nix::errno::errno())
            }
            code
        }
        None => libc_bind(sockfd, addr, len),
    }
}
