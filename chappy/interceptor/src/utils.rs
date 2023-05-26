use crate::{conf, RUNTIME};
use nix::libc::{c_int, sockaddr, socklen_t};
use nix::sys::socket::{self, SockaddrIn, SockaddrLike, SockaddrStorage};
use std::io::Result as IoResult;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::str::FromStr;
use tracing::{debug, error};

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

pub(crate) fn request_punch(sockfd: c_int, addr_in: SockaddrIn) -> IoResult<SockaddrIn> {
    let src_port = bind_random_port(sockfd);

    // TODO: blocking here is not ideal because it makes the connect blocking
    // event if it wasn't supposed to be. But if made none-blocking by spawning a task,
    // we have to make sure that the task is brought to completion.
    RUNTIME.block_on(async move {
        let res = chappy_util::protocol::register_client(
            PERFORATOR_ADDRESS,
            src_port,
            addr_in.ip().into(),
            addr_in.port(),
        )
        .await;
        match &res {
            Ok(()) => debug!(
                "Perforator call for registering client port {} (socket {}) to address {}:{} completed",
                src_port,
                sockfd,
                Ipv4Addr::from(addr_in.ip()),
                addr_in.port(),
            ),
            Err(err) => error!(
                "Perforator call for registering client port {} (socket {}) to address {}:{} failed: {}",
                src_port,
                sockfd,
                Ipv4Addr::from(addr_in.ip()),
                addr_in.port(),
                err,
            )
        };
        res
    })?;
    Ok(SockaddrIn::from_str(PERFORATOR_ADDRESS).unwrap())
}

pub(crate) enum ParsedAddress {
    RemoteVirtual(SockaddrIn),
    LocalVirtual(SockaddrIn),
    NotVirtual,
    Unknown,
}

pub(crate) unsafe fn parse_virtual(addr: *const sockaddr, len: socklen_t) -> ParsedAddress {
    let addr_stor = SockaddrStorage::from_raw(addr, Some(len)).unwrap();
    let (virt_ip, virt_range) = match (conf::virtual_ip(), conf::virtual_subnet()) {
        (Some(ip), Some(range)) => (ip, range),
        _ => {
            debug!("virtual IP or range not defined");
            return ParsedAddress::Unknown;
        }
    };
    match addr_stor.as_sockaddr_in() {
        Some(addr_in) => {
            let ip = Ipv4Addr::from(addr_in.ip());
            if ip.to_string() == virt_ip {
                ParsedAddress::LocalVirtual(*addr_in)
            } else if virt_range.contains(&ip) {
                ParsedAddress::RemoteVirtual(*addr_in)
            } else {
                debug!(
                    "{} not in virtual network {}",
                    Ipv4Addr::from(addr_in.ip()).to_string(),
                    virt_range
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
