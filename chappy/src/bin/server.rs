use env_logger;
use log::{debug, error, info};
use std::env;
use std::io::Read;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::thread;

fn handle_client(mut stream: TcpStream) {
    // read 20 bytes at a time from stream echoing back to stream
    let mut bytes_echoed = 0;
    loop {
        let mut read = [0; 1028];
        match stream.read(&mut read) {
            Ok(n) => {
                bytes_echoed += n;
                if n == 0 {
                    stream.flush().unwrap();
                    debug!("Stream EOF, bytes echoed: {}", bytes_echoed);
                    break;
                }
                stream.write_all(&read[0..n]).unwrap();
            }
            Err(err) => {
                panic!("{:?}", err);
            }
        }
    }
}

fn run() {
    let virtual_ip = env::var("CHAPPY_VIRTUAL_IP").unwrap();
    let listener = TcpListener::bind(format!("{}:8080", virtual_ip)).unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                info!(
                    "New incomming request on {} from {}",
                    stream.local_addr().unwrap(),
                    stream.peer_addr().unwrap()
                );
                thread::spawn(move || {
                    handle_client(stream);
                });
            }
            Err(_) => {
                error!("TcpListener incoming() failed.");
            }
        }
    }
}

fn main() {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();
    info!("Starting server...");
    run()
}
