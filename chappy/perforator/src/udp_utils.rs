use socket2::{Domain, Socket, Type};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::str::FromStr;
use tokio::net::UdpSocket as TokioSocket;

pub async fn send_from_reusable_port(src_port: u16, buff: &[u8], dest_addr: String) {
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, None).unwrap();
    sock.set_reuse_port(true).unwrap();
    let src_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, src_port);
    sock.bind(&src_addr.into()).unwrap();
    let tokio_sock = TokioSocket::from_std(sock.into()).unwrap();
    let dest_addr = SocketAddrV4::from_str(&dest_addr).unwrap();
    let sent = tokio_sock.send_to(buff, &dest_addr).await.unwrap();
    assert_eq!(sent, buff.len());
}
