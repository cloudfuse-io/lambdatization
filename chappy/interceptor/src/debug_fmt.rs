use nix::{
    libc::{c_int, sockaddr, socklen_t},
    sys::socket::{SockaddrIn, SockaddrLike},
};
use std::net::Ipv4Addr;
use tracing::trace;

pub(crate) fn dst_rewrite(func: &str, fd: c_int, new_addr: &SockaddrIn, old_addr: &SockaddrIn) {
    trace!(
        "Calling libc.{}({}, {}:{}) instead of ({}, {}:{})",
        func,
        fd,
        Ipv4Addr::from(new_addr.ip()),
        new_addr.port(),
        fd,
        Ipv4Addr::from(old_addr.ip()),
        old_addr.port()
    );
}

pub(crate) unsafe fn dst(func: &str, fd: c_int, addr: *const sockaddr, len: socklen_t) {
    let addr_stor = nix::sys::socket::SockaddrStorage::from_raw(addr, Some(len)).unwrap();
    let addr = if let Some(addr) = addr_stor.as_sockaddr_in() {
        format!("{}:{}", Ipv4Addr::from(addr.ip()), addr.port())
    } else {
        String::from("not-ipv4")
    };
    trace!("Calling libc.{}({}, {})", func, fd, addr);
}

pub(crate) fn return_code(func: &str, fd: c_int, code: c_int) {
    if code == -1 {
        trace!("libc.{}({}): errno {}", func, fd, nix::errno::errno())
    } else {
        trace!("libc.{}({}): success", func, fd)
    }
}
