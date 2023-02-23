use crate::{quic_utils, seed_client, RUNTIME, VIRTUAL_NET};
use futures::StreamExt;
use log::debug;
use nix::libc::{sockaddr, socklen_t};
use nix::sys::socket::{self, sockopt, SockaddrIn, SockaddrLike, SockaddrStorage};
use std::net::Ipv4Addr;
use std::os::fd::AsRawFd;
use std::str::FromStr;
use tokio::net::{TcpListener, TcpStream};

async fn request_punch_async(target_virtual_ip: String, target_port: u16) -> (String, u16) {
    let forwarder_port = 5000;
    let listener = TcpListener::bind(format!("127.0.0.1:{}", forwarder_port))
        .await
        .unwrap();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let p2p_port = 5001;

        let addr = seed_client::request_punch(p2p_port, target_virtual_ip, target_port)
            .await
            .target_nated_addr
            .unwrap();

        let socket = std::net::UdpSocket::bind(format!("0.0.0.0:{}", p2p_port)).unwrap();
        socket::setsockopt(socket.as_raw_fd(), sockopt::ReusePort, &true).unwrap();
        let mut quic_endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        quic_endpoint.rebind(socket).unwrap();
        quic_endpoint.set_default_client_config(quic_utils::configure_client());
        let quic_con = quic_endpoint
            .connect(
                format!("{}:{}", addr.ip, addr.port).parse().unwrap(),
                "chappy",
            )
            .unwrap()
            .await
            .unwrap();
        let (mut quic_send, mut quic_recv) = quic_con.open_bi().await.unwrap();
        let (mut tcp_read, mut tcp_write) = stream.into_split();
        let out_handle = tokio::spawn(async move {
            tokio::io::copy(&mut tcp_read, &mut quic_send)
                .await
                .unwrap()
        });
        let in_handle = tokio::spawn(async move {
            tokio::io::copy(&mut quic_recv, &mut tcp_write)
                .await
                .unwrap()
        });
        out_handle.await.unwrap();
        in_handle.await.unwrap();
    });
    // wait for the forwarder to be up
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    (String::from("127.0.0.1"), forwarder_port)
}

pub(crate) fn request_punch(addr_in: SockaddrIn) -> SockaddrIn {
    let ip: Ipv4Addr = addr_in.ip().into();
    let (target_ip, target_port) =
        RUNTIME.block_on(request_punch_async(ip.to_string(), addr_in.port()));
    debug!("Local relay server address {}:{}", target_ip, target_port);
    SockaddrIn::from_str(&format!("{}:{}", target_ip, target_port)).unwrap()
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
                tokio::spawn(async move {
                    tokio::io::copy(&mut quic_recv, &mut fwd_write)
                        .await
                        .unwrap()
                });
                tokio::spawn(async move {
                    tokio::io::copy(&mut fwd_read, &mut quic_send)
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
