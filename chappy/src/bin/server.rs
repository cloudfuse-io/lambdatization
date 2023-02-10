use env_logger;
use log::{debug, error};
use std::env;
use std::io::Read;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::thread;

fn handle_client(mut stream: TcpStream) {
    // read 20 bytes at a time from stream echoing back to stream
    loop {
        let mut read = [0; 1028];
        match stream.read(&mut read) {
            Ok(n) => {
                if n == 0 {
                    // connection was closed
                    break;
                }
                stream.write(&read[0..n]).unwrap();
            }
            Err(err) => {
                panic!("{:?}", err);
            }
        }
    }
}

fn run() {
    let virtual_ip = env::var("SERVER_VIRTUAL_IP").unwrap();
    let listener = TcpListener::bind(format!("{}:8080", virtual_ip)).unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                debug!(
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
    env_logger::init();
    debug!("Starting server...");
    run()
}
