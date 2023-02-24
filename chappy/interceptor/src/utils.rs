use crate::{RUNTIME, VIRTUAL_NET};
use chappy_util::{REGISTER_CLIENT_HEADER_BYTES, REGISTER_SERVER_HEADER_BYTES};
use log::debug;
use nix::libc::{c_int, sockaddr, socklen_t};
use nix::sys::socket::{self, SockaddrIn, SockaddrLike, SockaddrStorage};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::str::FromStr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

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

    RUNTIME.block_on(async move {
        let mut stream = TcpStream::connect(PERFORATOR_ADDRESS).await.unwrap();
        stream
            .write_all(&REGISTER_CLIENT_HEADER_BYTES)
            .await
            .unwrap();
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
    SockaddrIn::from_str(PERFORATOR_ADDRESS).unwrap()
}

pub(crate) fn register(addr_in: SockaddrIn) -> SockaddrIn {
    let registered_port = addr_in.port();
    RUNTIME.block_on(async move {
        let mut stream = TcpStream::connect(PERFORATOR_ADDRESS).await.unwrap();
        stream
            .write_all(&REGISTER_SERVER_HEADER_BYTES)
            .await
            .unwrap();
        stream.write_u16(registered_port).await.unwrap();
        assert_eq!(stream.read_u8().await.unwrap(), 1);
        debug!(
            "Registered server on port {} to perforator",
            registered_port,
        )
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
