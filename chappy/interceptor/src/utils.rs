use crate::{CHAPPY_CONF, RUNTIME, VIRTUAL_NET};
use log::debug;
use nix::libc::{c_int, sockaddr, socklen_t};
use nix::sys::socket::{self, SockaddrIn, SockaddrLike, SockaddrStorage};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::str::FromStr;

const PERFORATOR_ADDRESS: &str = "127.0.0.1:5000";

fn bind_random_port(sockfd: c_int) -> u16 {
    // TODO support ipv6
    socket::bind(
        sockfd,
        &SockaddrIn::from(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0)),
    )
    .expect(
        "Bind failed on connect()'s socket, maybe it was already bound by the original caller?",
    );
    let bound_socket: SockaddrIn = socket::getsockname(sockfd).unwrap();
    bound_socket.port()
}

pub(crate) fn request_punch(sockfd: c_int, addr_in: SockaddrIn) -> SockaddrIn {
    let src_port = bind_random_port(sockfd);

    // TODO: blocking here is not ideal because it makes the connect blocking
    // event if it wasn't supposed to be. But if made none-blocking by spawning a task,
    // we have to make sure that the task is brought to completion.
    RUNTIME.block_on(async move {
        chappy_perforator::protocol::register_client(
            PERFORATOR_ADDRESS,
            src_port,
            addr_in.ip().into(),
            addr_in.port(),
        )
        .await;
        debug!(
            "Perforator call for registering client port {} (socket {}) to address {}:{} completed",
            src_port,
            sockfd,
            Ipv4Addr::from(addr_in.ip()),
            addr_in.port(),
        )
    });
    SockaddrIn::from_str(PERFORATOR_ADDRESS).unwrap()
}

pub(crate) fn register(addr_in: SockaddrIn) -> SockaddrIn {
    let registered_port = addr_in.port();
    RUNTIME.block_on(async move {
        chappy_perforator::protocol::register_server(PERFORATOR_ADDRESS).await;
        debug!(
            "Perforator call for registering server (port {}) completed",
            registered_port,
        )
    });
    SockaddrIn::new(127, 0, 0, 1, registered_port)
}

pub(crate) enum ParsedAddress {
    RemoteVirtual(SockaddrIn),
    LocalVirtual(SockaddrIn),
    NotVirtual,
}

pub(crate) unsafe fn parse_virtual(addr: *const sockaddr, len: socklen_t) -> ParsedAddress {
    let addr_stor = SockaddrStorage::from_raw(addr, Some(len)).unwrap();
    match addr_stor.as_sockaddr_in() {
        Some(addr_in) => {
            let ip = Ipv4Addr::from(addr_in.ip());
            if ip.to_string() == CHAPPY_CONF.virtual_ip {
                ParsedAddress::LocalVirtual(addr_in.clone())
            } else if VIRTUAL_NET.contains(&ip) {
                ParsedAddress::RemoteVirtual(addr_in.clone())
            } else {
                debug!(
                    "{} not in virtual network {}",
                    Ipv4Addr::from(addr_in.ip()).to_string(),
                    VIRTUAL_NET.to_string()
                );
                ParsedAddress::NotVirtual
            }
        }
        None => {
            debug!("Not an IPv4 addr");
            ParsedAddress::NotVirtual
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_double_free() {
        // tests of SockaddrIn memory ownership
        // from_raw actually uses std::ptr::read which does a memory copy

        let addr = SockaddrIn::new(0, 0, 0, 0, 80);
        let addr_ptr: *const sockaddr = Box::into_raw(Box::new(addr)).cast();

        let parsed_addr_1 = unsafe { SockaddrIn::from_raw(addr_ptr, None).unwrap() };
        let parsed_addr_2 = unsafe { SockaddrIn::from_raw(addr_ptr, None).unwrap() };

        println!("parsed_addr_1.port(): {:?}", parsed_addr_1.port());
        println!("parsed_addr_2.port(): {:?}", parsed_addr_2.port());

        drop(parsed_addr_1);

        println!("parsed_addr_2.port(): {:?}", parsed_addr_2.port());

        let parsed_addr_3 = unsafe { SockaddrIn::from_raw(addr_ptr, None).unwrap() };
        println!("parsed_addr_3.port(): {:?}", parsed_addr_3.port());

        drop(addr);

        println!("parsed_addr_2.port(): {:?}", parsed_addr_2.port());

        let parsed_addr_4 = unsafe { SockaddrIn::from_raw(addr_ptr, None).unwrap() };
        println!("parsed_addr_4.port(): {:?}", parsed_addr_4.port());
    }
}
