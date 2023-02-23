use crate::{quic_utils, seed_client, REGISTER_MAGIC_BYTES, RUNTIME, VIRTUAL_NET};
use futures::StreamExt;
use log::debug;
use nix::libc::{c_int, sockaddr, socklen_t};
use nix::sys::socket::{self, sockopt, SockaddrIn, SockaddrLike, SockaddrStorage};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::os::fd::AsRawFd;
use std::str::FromStr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

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
    let perforator_addr = "127.0.0.1:5000";
    let src_port = bind_random_port(sockfd);

    RUNTIME.block_on(async move {
        let mut stream = TcpStream::connect(perforator_addr).await.unwrap();
        stream.write_all(&REGISTER_MAGIC_BYTES).await.unwrap();
        stream.write_u16(src_port).await.unwrap();
        stream.write_u32(addr_in.ip()).await.unwrap();
        stream.write_u16(addr_in.port()).await.unwrap();
        assert_eq!(stream.read_u8().await.unwrap(), 1);
        debug!(
            "Port mapping {}->{}:{} for socket {} registered on perforator",
            src_port,
            Ipv4Addr::from(addr_in.ip()),
            addr_in.port(),
            sockfd,
        )
    });
    SockaddrIn::from_str(perforator_addr).unwrap()
}

pub(crate) fn register(addr_in: SockaddrIn) -> SockaddrIn {
    let registered_port = addr_in.port();
    RUNTIME.spawn(async move {
        let p2p_port = 5002;
        let stream = seed_client::register(p2p_port, registered_port).await;
        // For each incoming server punch request, we create a hole punched connection.
        // We then forward that connection to the local listening server.
        stream
            .map(|punch_req| async {
                // holepunch connection
                let socket = std::net::UdpSocket::bind(format!("0.0.0.0:{}", p2p_port)).unwrap();
                socket::setsockopt(socket.as_raw_fd(), sockopt::ReusePort, &true).unwrap();
                let client_nated_addr = punch_req.unwrap().client_nated_addr.unwrap();
                let client_nated_url =
                    format!("{}:{}", client_nated_addr.ip, client_nated_addr.port);
                socket.send_to(&[1, 2, 3, 4], client_nated_url).unwrap();

                // quic server
                let (server_config, _server_cert) = quic_utils::configure_server().unwrap();
                let endpoint =
                    quinn::Endpoint::server(server_config, "0.0.0.0:0".parse().unwrap()).unwrap();
                // let socket = std::net::UdpSocket::bind(format!("0.0.0.0:{}", p2p_port)).unwrap();
                // socket::setsockopt(socket.as_raw_fd(), sockopt::ReusePort, &true).unwrap();
                endpoint.rebind(socket).unwrap();
                let conn = endpoint.accept().await.unwrap().await.unwrap();
                let (mut quic_send, mut quic_recv) = conn.accept_bi().await.unwrap();

                // forwarding connection
                let localhost_url = format!("localhost:{}", registered_port);
                let fwd_stream = TcpStream::connect(localhost_url).await.unwrap();

                // pipe holepunch connection to forwarding connection
                let (mut fwd_read, mut fwd_write) = fwd_stream.into_split();
                let out_handle = tokio::spawn(async move {
                    tokio::io::copy(&mut quic_recv, &mut fwd_write)
                        .await
                        .unwrap()
                });
                let in_handle = tokio::spawn(async move {
                    tokio::io::copy(&mut fwd_read, &mut quic_send)
                        .await
                        .unwrap()
                });
                out_handle.await.unwrap();
                in_handle.await.unwrap();
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
