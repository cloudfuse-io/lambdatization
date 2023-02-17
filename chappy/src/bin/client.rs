use log::{debug, info};
use rand::RngCore;
use rand::{rngs::SmallRng, SeedableRng};
use std::env;
use std::io::prelude::*;
use std::net::TcpStream;
use std::sync::Arc;

// bigger batches seem to fail
const BATCH_SIZE: usize = 1024 * 64;
const BYTES_SENT: usize = 1024 * 1024;
const DEBUG_EVERY_BYTES: usize = 1024 * 64;
const DEBUG_EVERY_BATCH: usize = DEBUG_EVERY_BYTES / BATCH_SIZE;
const NB_BATCH: usize = BYTES_SENT / BATCH_SIZE;

fn main() {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();
    let virtual_ip = env::var("SERVER_VIRTUAL_IP").unwrap();
    let server_address = format!("{}:8080", virtual_ip);
    info!("Connecting to echo server {}", server_address);
    let stream_res = TcpStream::connect(&server_address);
    let stream_ref = Arc::new(stream_res.unwrap());
    info!("Connection to echo server successful");

    let stream_write_ref = Arc::clone(&stream_ref);
    let write_handle = std::thread::spawn(move || {
        info!("Starting write...");
        let mut rng = SmallRng::seed_from_u64(0);
        let mut buf = vec![0u8; BATCH_SIZE];
        for i in 1..=NB_BATCH {
            rng.fill_bytes(&mut buf);
            stream_write_ref
                .as_ref()
                .write_all(&buf)
                .expect("write to echo server tcp failed");
            if i % DEBUG_EVERY_BATCH == 0 {
                debug!("{} batches written", DEBUG_EVERY_BATCH);
            }
        }
        stream_write_ref
            .shutdown(std::net::Shutdown::Write)
            .unwrap();
        debug!("Write thread done");
    });
    let stream_read_ref = Arc::clone(&stream_ref);
    let read_handle = std::thread::spawn(move || {
        info!("Starting read...");
        let mut rng = SmallRng::seed_from_u64(0);
        let mut rng_buf = vec![0u8; BATCH_SIZE];
        let mut read_buf = vec![0u8; BATCH_SIZE];
        for i in 1..=NB_BATCH {
            rng.fill_bytes(&mut rng_buf);
            stream_read_ref
                .as_ref()
                .read_exact(&mut read_buf)
                .expect("write to echo server tcp failed");
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
