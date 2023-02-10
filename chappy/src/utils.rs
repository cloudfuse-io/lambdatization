use crate::{seed_client, utils};
use futures::StreamExt;
use log::debug;
use nix::libc::{c_int, sockaddr, socklen_t};
use nix::sys::socket::{
    self, sockopt::ReusePort, AddressFamily, SockFlag, SockType, SockaddrIn, SockaddrLike,
    SockaddrStorage,
};
use std::env;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::os::fd::RawFd;
use std::str::FromStr;
use tokio::runtime;

lazy_static! {
    static ref RUNTIME: runtime::Runtime = runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    static ref VIRTUAL_NET: ipnet::Ipv4Net = env::var("VIRTUAL_SUBNET").unwrap().parse().unwrap();
}

pub(crate) fn bind_random_port(sockfd: RawFd) -> u16 {
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
    // bind the socket connect() is planning to use to a reusable random port
    socket::setsockopt(sockfd, ReusePort, &true).unwrap();
    debug!("SO_REUSEPORT=true set on {}", sockfd);
    let source_port = utils::bind_random_port(sockfd);
    debug!("bound source socket {} to port {}", sockfd, source_port);
    // convert sockaddr into (ip: String, port: u16)
    let ip: Ipv4Addr = addr_in.ip().into();

    let resp = RUNTIME
        .block_on(seed_client::request_punch(
            source_port,
            ip.to_string(),
            addr_in.port(),
        ))
        .target_nated_addr
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    SockaddrIn::from_str(&format!("{}:{}", resp.ip, resp.port)).unwrap()
}

pub(crate) fn register(sockfd: c_int, addr_in: SockaddrIn) -> SockaddrIn {
    socket::setsockopt(sockfd, ReusePort, &true).unwrap();
    debug!("SO_REUSEPORT=true set on {}", sockfd);
    let registered_port = addr_in.port();
    RUNTIME.spawn(async move {
        let stream = seed_client::register(addr_in.port()).await;
        // For each incoming server punch request, connect to the client
        // NATed address from the server port to configure the NAT on the
        // server side.
        stream
            .map(|punch_req| async {
                let client_nated_addr = punch_req.unwrap().client_nated_addr.unwrap();
                let sockfd = nix::sys::socket::socket(
                    AddressFamily::Inet,
                    SockType::Stream,
                    SockFlag::empty(),
                    None,
                )
                .unwrap();
                socket::setsockopt(sockfd, ReusePort, &true).unwrap();
                debug!("SO_REUSEPORT=true set on {}", sockfd);

                // set socket timeouts
                let time_val = nix::sys::time::TimeVal::new(0, 1);
                socket::setsockopt(sockfd, nix::sys::socket::sockopt::SendTimeout, &time_val)
                    .unwrap();
                socket::setsockopt(sockfd, nix::sys::socket::sockopt::ReceiveTimeout, &time_val)
                    .unwrap();

                nix::sys::socket::bind(sockfd, &SockaddrIn::new(0, 0, 0, 0, registered_port))
                    .unwrap();
                // This can be used to controll which sides tries to connect first
                // tokio::time::sleep(Duration::from_millis(50)).await;
                let url = format!("{}:{}", client_nated_addr.ip, client_nated_addr.port);
                debug!(
                    "Configure NAT by connecting 0.0.0.0:{} -> {}",
                    registered_port, url
                );
                match nix::sys::socket::connect(sockfd, &SockaddrIn::from_str(&url).unwrap()) {
                    Ok(_) => debug!("NAT configuration: successful connection"),
                    Err(err) => debug!("NAT configuration: error {}", err),
                }
                // safety: this socket was created here and isn't reused after
                let close_res = unsafe { nix::libc::close(sockfd) };
                debug!("Socket {} closed with return code {}", sockfd, close_res);
            })
            .buffer_unordered(usize::MAX)
            .for_each(|_| async {})
            .await
    });
    SockaddrIn::new(0, 0, 0, 0, registered_port)
}

pub(crate) unsafe fn parse_virtual(addr: *const sockaddr, len: socklen_t) -> Option<SockaddrIn> {
    let addr_stor = SockaddrStorage::from_raw(addr, Some(len)).unwrap();
    match addr_stor.as_sockaddr_in() {
        Some(addr_in) if VIRTUAL_NET.contains(&Ipv4Addr::from(addr_in.ip())) => {
            Some(addr_in.clone())
        }
        Some(addr_in) => {
            debug!(
                "{} not in virtual network {}, forwarding to libc",
                Ipv4Addr::from(addr_in.ip()).to_string(),
                VIRTUAL_NET.to_string()
            );
            None
        }
        None => {
            debug!("Not an IPv4 addr");
            None
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
