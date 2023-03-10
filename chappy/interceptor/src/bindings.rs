use crate::{
    debug_fmt,
    utils::{
        self,
        ParsedAddress::{LocalVirtual, NotVirtual, RemoteVirtual},
    },
    LIBC_LOADED,
};
use env_logger;
use log::error;
use nix::{
    libc::{__errno_location, c_int, sockaddr, socklen_t, EADDRNOTAVAIL},
    sys::socket::{SockaddrIn, SockaddrLike},
};
use std::ptr;

use utils::{parse_virtual, register, request_punch};

fn init_logger() {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .try_init()
        .ok();
}

type ConnectSymbol<'a> =
    libloading::Symbol<'a, unsafe extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int>;

#[no_mangle]
pub unsafe extern "C" fn connect(sockfd: c_int, addr: *const sockaddr, len: socklen_t) -> c_int {
    init_logger();
    let libc_connect: ConnectSymbol = LIBC_LOADED.get(b"connect").unwrap();
    // debug!("Entering interception connect({})", sockfd);
    let code = match parse_virtual(addr, len) {
        RemoteVirtual(addr_in) => {
            let new_addr = request_punch(sockfd, addr_in);
            debug_fmt::dst_rewrite("connect", sockfd, &new_addr, &addr_in);
            libc_connect(sockfd, ptr::addr_of!(new_addr).cast(), new_addr.len())
        }
        LocalVirtual(addr_in) => {
            let local = SockaddrIn::new(127, 0, 0, 1, addr_in.port());
            debug_fmt::dst_rewrite("connect", sockfd, &local, &addr_in);
            libc_connect(sockfd, ptr::addr_of!(local).cast(), local.len())
        }
        NotVirtual => {
            debug_fmt::dst("connect", sockfd, addr, len);
            libc_connect(sockfd, addr, len)
        }
    };
    debug_fmt::return_code("connect", sockfd, code);
    code
}

type BindSymbol<'a> =
    libloading::Symbol<'a, unsafe extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int>;

#[no_mangle]
pub unsafe extern "C" fn bind(sockfd: c_int, addr: *const sockaddr, len: socklen_t) -> c_int {
    init_logger();
    // debug!("Entering interception bind({})", sockfd);
    let libc_bind: BindSymbol = LIBC_LOADED.get(b"bind").unwrap();

    let code = match parse_virtual(addr, len) {
        LocalVirtual(addr_in) => {
            let new_addr = register(addr_in);
            debug_fmt::dst_rewrite("bind", sockfd, &new_addr, &addr_in);
            libc_bind(sockfd, ptr::addr_of!(new_addr).cast(), new_addr.len())
        }
        RemoteVirtual(_) => {
            error!("Binding to remote virtual address");
            *__errno_location() = EADDRNOTAVAIL;
            -1
        }
        NotVirtual => {
            debug_fmt::dst("bind", sockfd, addr, len);
            libc_bind(sockfd, addr, len)
        }
    };
    debug_fmt::return_code("bind", sockfd, code);
    code
}