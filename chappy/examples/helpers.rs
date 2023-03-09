use log::{debug, info};
use rand::RngCore;
use rand::{rngs::SmallRng, SeedableRng};
use std::io::prelude::*;
use std::net::TcpStream;
use std::thread::JoinHandle;

pub fn send_pseudo_random(
    stream: TcpStream,
    batch_size: usize,
    nb_batch: usize,
    debug_every_batch: usize,
) -> (JoinHandle<()>, JoinHandle<()>) {
    let mut stream_write = stream.try_clone().unwrap();
    let write_handle = std::thread::spawn(move || {
        info!("Starting write...");
        let mut rng = SmallRng::seed_from_u64(0);
        let mut buf = vec![0u8; batch_size];
        for i in 1..=nb_batch {
            rng.fill_bytes(&mut buf);
            stream_write
                .write_all(&buf)
                .expect("write to echo server tcp failed");
            if i % debug_every_batch == 0 {
                debug!("{} batches written", debug_every_batch);
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
        let mut rng_buf = vec![0u8; batch_size];
        let mut read_buf = vec![0u8; batch_size];
        for i in 1..=nb_batch {
            rng.fill_bytes(&mut rng_buf);
            stream_read
                .read_exact(&mut read_buf)
                .expect("read from echo server tcp failed");
            assert_eq!(rng_buf, read_buf);
            if i % debug_every_batch == 0 {
                debug!("{} batches read", debug_every_batch);
            }
        }
        debug!("Read thread done");
    });
    (write_handle, read_handle)
}
