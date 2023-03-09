use chappy_perforator::{forwarder::Forwarder, perforator::Perforator, protocol::ParsedTcpStream};

use log::debug;

use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();
    let listener = TcpListener::bind("127.0.0.1:5000").await.unwrap();
    let forwarder = Forwarder::new(5001, 5002);
    let perforator = Arc::new(Perforator::new(forwarder));
    loop {
        let (stream, _) = listener.accept().await.unwrap();
        debug!(
            "New connection to perforator TCP server from {}:{}",
            stream.peer_addr().unwrap().ip(),
            stream.peer_addr().unwrap().port()
        );
        let perforator = Arc::clone(&perforator);
        tokio::spawn(async move {
            let parsed_stream = ParsedTcpStream::from(stream).await;
            match parsed_stream {
                ParsedTcpStream::ClientRegistration {
                    source_port,
                    target_virtual_ip,
                    target_port,
                } => perforator.register_client(source_port, target_virtual_ip, target_port),
                ParsedTcpStream::ServerRegistration { registered_port } => {
                    perforator.register_server(registered_port)
                }
                ParsedTcpStream::Raw(stream) => perforator.forward_client(stream).await,
            }
        });
    }
}
