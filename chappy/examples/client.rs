use helpers::send_pseudo_random;

use log::info;
use std::env;
use std::net::TcpStream;

#[allow(non_snake_case)]
fn main() {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();

    let SERVER_VIRTUAL_IP: String = env::var("SERVER_VIRTUAL_IP").unwrap();
    let BATCH_SIZE: usize = env::var("BATCH_SIZE").unwrap().parse().unwrap();
    let BYTES_SENT: usize = env::var("BYTES_SENT").unwrap().parse().unwrap();
    let NB_BATCH: usize = BYTES_SENT / BATCH_SIZE;
    let DEBUG_EVERY_BATCH: usize = NB_BATCH / 10;

    let server_address = format!("{}:8080", SERVER_VIRTUAL_IP);
    info!("Connecting to echo server {}", server_address);
    let stream = TcpStream::connect(&server_address).unwrap();
    info!("Connection to echo server successful");

    let (write_handle, read_handle) =
        send_pseudo_random(stream, BATCH_SIZE, NB_BATCH, DEBUG_EVERY_BATCH);
    write_handle.join().unwrap();
    info!("Write {} Bytes completed", BYTES_SENT);
    read_handle.join().unwrap();
    info!("Read {} Bytes completed", BYTES_SENT);
}
