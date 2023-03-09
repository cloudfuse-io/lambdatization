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
    let stream1 = TcpStream::connect(&server_address).unwrap();
    info!("Connecting to echo server {} again", server_address);
    let stream2 = TcpStream::connect(&server_address).unwrap();
    info!("Connections to echo server successful");

    let (write1, read1) = send_pseudo_random(stream1, BATCH_SIZE, NB_BATCH / 2, DEBUG_EVERY_BATCH);
    let (write2, read2) = send_pseudo_random(stream2, BATCH_SIZE, NB_BATCH / 2, DEBUG_EVERY_BATCH);
    write1.join().unwrap();
    info!("Write {} Bytes completed", BYTES_SENT / 2);
    read1.join().unwrap();
    info!("Read {} Bytes completed", BYTES_SENT / 2);
    write2.join().unwrap();
    info!("Write {} Bytes completed", BYTES_SENT / 2);
    read2.join().unwrap();
    info!("Read {} Bytes completed", BYTES_SENT / 2);
}
