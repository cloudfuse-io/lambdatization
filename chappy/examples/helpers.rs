use log::{debug, info};
use rand::RngCore;
use rand::{rngs::SmallRng, SeedableRng};
use std::io::prelude::*;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub fn send_pseudo_random(
    stream: std::net::TcpStream,
    batch_size: usize,
    nb_batch: usize,
    debug_every_batch: usize,
) -> (std::thread::JoinHandle<()>, std::thread::JoinHandle<()>) {
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

fn monitored_spawn<T>(
    monitor: Option<&tokio_metrics::TaskMonitor>,
    fut: T,
) -> tokio::task::JoinHandle<T::Output>
where
    T: std::future::Future + Send + 'static,
    T::Output: Send + 'static,
{
    match monitor {
        Some(m) => tokio::spawn(m.instrument(fut)),
        None => tokio::spawn(fut),
    }
}

pub async fn send_pseudo_random_async(
    stream: tokio::net::TcpStream,
    batch_size: usize,
    nb_batch: usize,
    debug_every_batch: usize,
    monitor: Option<&tokio_metrics::TaskMonitor>,
) -> (tokio::task::JoinHandle<()>, tokio::task::JoinHandle<()>) {
    let (mut stream_read, mut stream_write) = stream.into_split();
    let write_handle = monitored_spawn(monitor, async move {
        info!("Starting write...");
        let mut rng = SmallRng::seed_from_u64(0);
        let mut buf = vec![0u8; batch_size];
        for i in 1..=nb_batch {
            rng.fill_bytes(&mut buf);
            stream_write
                .write_all(&buf)
                .await
                .expect("write to echo server tcp failed");
            if i % debug_every_batch == 0 {
                debug!("{} batches written", debug_every_batch);
            }
        }
        stream_write.flush().await.unwrap();
        // stream_write.shutdown(std::net::Shutdown::Write).unwrap();
        debug!("Write thread done");
    });
    let read_handle = monitored_spawn(monitor, async move {
        info!("Starting read...");
        let mut rng = SmallRng::seed_from_u64(0);
        let mut rng_buf = vec![0u8; batch_size];
        let mut read_buf = vec![0u8; batch_size];
        for i in 1..=nb_batch {
            rng.fill_bytes(&mut rng_buf);
            stream_read
                .read_exact(&mut read_buf)
                .await
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

pub async fn send_slow(
    stream: tokio::net::TcpStream,
    nb_batch: usize,
    time_between_batches: Duration,
    monitor: Option<&tokio_metrics::TaskMonitor>,
) -> (tokio::task::JoinHandle<()>, tokio::task::JoinHandle<()>) {
    const BATCH_SIZE: usize = 128;
    let (mut stream_read, mut stream_write) = stream.into_split();
    let write_handle = monitored_spawn(monitor, async move {
        info!("Starting write...");
        let mut rng = SmallRng::seed_from_u64(0);
        let mut buf = vec![0u8; BATCH_SIZE];
        for i in 1..=nb_batch {
            rng.fill_bytes(&mut buf);
            stream_write
                .write_all(&buf)
                .await
                .expect("write to echo server tcp failed");
            debug!("batche written");
            if i < nb_batch {
                tokio::time::sleep(time_between_batches).await;
            }
        }
        stream_write.flush().await.unwrap();
        // stream_write.shutdown(std::net::Shutdown::Write).unwrap();
        debug!("Write thread done");
    });
    let read_handle = monitored_spawn(monitor, async move {
        info!("Starting read...");
        let mut rng = SmallRng::seed_from_u64(0);
        let mut rng_buf = vec![0u8; BATCH_SIZE];
        let mut read_buf = vec![0u8; BATCH_SIZE];
        for _ in 1..=nb_batch {
            rng.fill_bytes(&mut rng_buf);
            stream_read
                .read_exact(&mut read_buf)
                .await
                .expect("read from echo server tcp failed");
            assert_eq!(rng_buf, read_buf);
            debug!("batch read");
        }
        debug!("Read thread done");
    });
    (write_handle, read_handle)
}
