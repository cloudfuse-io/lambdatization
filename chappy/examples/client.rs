use log::{debug, info};
use rand::RngCore;
use rand::{rngs::SmallRng, SeedableRng};
use std::env;
use std::io::prelude::*;
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

    let mut stream_write = stream.try_clone().unwrap();
    let write_handle = std::thread::spawn(move || {
        info!("Starting write...");
        let mut rng = SmallRng::seed_from_u64(0);
        let mut buf = vec![0u8; BATCH_SIZE];
        for i in 1..=NB_BATCH {
            rng.fill_bytes(&mut buf);
            stream_write
                .write_all(&buf)
                .expect("write to echo server tcp failed");
            if i % DEBUG_EVERY_BATCH == 0 {
                debug!("{} batches written", DEBUG_EVERY_BATCH);
            }
        }
        stream_write.flush().unwrap();
        // stream_write.shutdown(std::net::Shutdown::Write).unwrap();
        debug!("Write thread done");
    });
    let mut stream_read = stream.try_clone().unwrap();
    let read_handle = std::thread::spawn(move || {
        info!("Starting read...");
        let mut rng = SmallRng::seed_from_u64(0);
        let mut rng_buf = vec![0u8; BATCH_SIZE];
        let mut read_buf = vec![0u8; BATCH_SIZE];
        for i in 1..=NB_BATCH {
            rng.fill_bytes(&mut rng_buf);
            stream_read
                .read_exact(&mut read_buf)
                .expect("read from echo server tcp failed");
            assert_eq!(rng_buf, read_buf);
            if i % DEBUG_EVERY_BATCH == 0 {
                debug!("{} batches read", DEBUG_EVERY_BATCH);
            }
        }
        debug!("Read thread done");
    });
    write_handle.join().unwrap();
    info!("Write {} Bytes completed", BYTES_SENT);
    read_handle.join().unwrap();
    info!("Read {} Bytes completed", BYTES_SENT);
}
