use helpers::send_pseudo_random_async;

use log::info;
use std::env;
use tokio::net::TcpStream;

#[allow(non_snake_case)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();
    let monitor = tokio_metrics::TaskMonitor::new();

    let SERVER_VIRTUAL_IP: String = env::var("SERVER_VIRTUAL_IP").unwrap();
    let BATCH_SIZE: usize = env::var("BATCH_SIZE").unwrap().parse().unwrap();
    let BYTES_SENT: usize = env::var("BYTES_SENT").unwrap().parse().unwrap();
    let NB_BATCH: usize = BYTES_SENT / BATCH_SIZE;
    let DEBUG_EVERY_BATCH: usize = NB_BATCH / 10;

    let server_address = format!("{}:8080", SERVER_VIRTUAL_IP);
    info!("Connecting to echo server {}", server_address);
    let stream = monitor
        .instrument(TcpStream::connect(&server_address))
        .await
        .unwrap();
    info!("Connection to echo server successful");

    let (write_handle, read_handle) = send_pseudo_random_async(
        stream,
        BATCH_SIZE,
        NB_BATCH,
        DEBUG_EVERY_BATCH,
        Some(&monitor),
    )
    .await;
    write_handle.await.unwrap();
    info!("Write {} Bytes completed", BYTES_SENT);
    read_handle.await.unwrap();
    info!("Read {} Bytes completed", BYTES_SENT);
    info!("Monitor: {:?}", monitor);
}
