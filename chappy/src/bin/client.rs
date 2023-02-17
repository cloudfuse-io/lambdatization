use log::debug;
use std::env;
use std::io::prelude::*;
use std::net::TcpStream;

fn main() {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();
    let virtual_ip = env::var("SERVER_VIRTUAL_IP").unwrap();
    let server_address = format!("{}:8080", virtual_ip);
    debug!("Connecting to echo server {}", server_address);
    let stream_res = TcpStream::connect(&server_address);
    let mut stream = stream_res.unwrap();
    // first write/read
    debug!("Connection to echo server successful");
    let bytes_written = stream.write(&[1]).expect("write to echo server tcp failed");
    debug!("Successfully wrote {} byte(s)", bytes_written);
    let bytes_read = stream
        .read(&mut [0; 128])
        .expect("read from echo server tcp failed");
    debug!("Successfully read {} byte(s)", bytes_read);
    // second write/read
    let bytes_written = stream
        .write(&[1, 2])
        .expect("write to echo server tcp failed");
    debug!("Successfully wrote {} byte(s)", bytes_written);
    let bytes_read = stream
        .read(&mut [0; 128])
        .expect("read from echo server tcp failed");
    debug!("Successfully read {} byte(s)", bytes_read);
}
