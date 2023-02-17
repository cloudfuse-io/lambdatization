use crate::{seed_client, utils, RUNTIME, VIRTUAL_NET};
use futures::StreamExt;
use log::debug;
use nix::libc::{c_int, sockaddr, socklen_t};
use nix::sys::socket::{self, sockopt, SockaddrIn, SockaddrLike, SockaddrStorage};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::os::fd::RawFd;
use std::str::FromStr;
use tokio::net::{TcpSocket, TcpStream};

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
    socket::setsockopt(sockfd, sockopt::ReusePort, &true).unwrap();
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
    debug!(
        "NATed server address received from Seed {}:{}",
        resp.ip, resp.port
    );
    SockaddrIn::from_str(&format!("{}:{}", resp.ip, resp.port)).unwrap()
}

pub(crate) fn register(sockfd: c_int, addr_in: SockaddrIn) -> SockaddrIn {
    socket::setsockopt(sockfd, sockopt::ReusePort, &true).unwrap();
    debug!("SO_REUSEPORT=true set on {}", sockfd);
    let registered_port = addr_in.port();
    RUNTIME.spawn(async move {
        let stream = seed_client::register(addr_in.port()).await;
        // For each incoming server punch request, we create a hole punched connection
        // by starting a connection from both sides. We then forward that connection
        // to the local listening server.
        stream
            .map(|punch_req| async {
                // holepunch connection
                let socket = TcpSocket::new_v4().unwrap();
                socket.set_reuseport(true).unwrap();
                let punch_bind_sock_addr =
                    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), registered_port);
                socket.bind(punch_bind_sock_addr).unwrap();
                let client_nated_addr = punch_req.unwrap().client_nated_addr.unwrap();
                let client_nated_url =
                    format!("{}:{}", client_nated_addr.ip, client_nated_addr.port);
                let punch_stream = socket
                    .connect(client_nated_url.parse().unwrap())
                    .await
                    .unwrap();

                // forwarding connection
                let localhost_url = format!("localhost:{}", registered_port);
                let fwd_stream = TcpStream::connect(localhost_url).await.unwrap();

                // pipe holepunch connection to forwarding connection
                let (mut punch_read, mut punch_write) = punch_stream.into_split();
                let (mut fwd_read, mut fwd_write) = fwd_stream.into_split();
                tokio::spawn(async move {
                    tokio::io::copy(&mut punch_read, &mut fwd_write)
                        .await
                        .unwrap()
                });
                tokio::spawn(async move {
                    tokio::io::copy(&mut fwd_read, &mut punch_write)
                        .await
                        .unwrap()
                });
            })
            .buffer_unordered(usize::MAX)
            .for_each(|_| async {})
            .await
    });
    SockaddrIn::new(127, 0, 0, 1, registered_port)
}

pub(crate) unsafe fn parse_virtual(addr: *const sockaddr, len: socklen_t) -> Option<SockaddrIn> {
    let addr_stor = SockaddrStorage::from_raw(addr, Some(len)).unwrap();
    match addr_stor.as_sockaddr_in() {
        Some(addr_in) if VIRTUAL_NET.contains(&Ipv4Addr::from(addr_in.ip())) => {
            Some(addr_in.clone())
        }
        Some(addr_in) => {
            debug!(
                "{} not in virtual network {}",
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
