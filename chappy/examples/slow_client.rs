use helpers::send_slow;

use log::info;
use std::{env, time::Duration};
use tokio::net::TcpStream;

#[allow(non_snake_case)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();
    let monitor = tokio_metrics::TaskMonitor::new();

    let SERVER_VIRTUAL_IP: String = env::var("SERVER_VIRTUAL_IP").unwrap();
    let NB_BATCH: usize = 2;
    let TIME_BETWEEN_BATCH_SEC = 10;

    let server_address = format!("{}:8080", SERVER_VIRTUAL_IP);
    info!("Connecting to echo server {}", server_address);
    let stream = monitor
        .instrument(TcpStream::connect(&server_address))
        .await
        .unwrap();
    info!("Connection to echo server successful");

    let (write_handle, read_handle) = send_slow(
        stream,
        NB_BATCH,
        Duration::from_secs(TIME_BETWEEN_BATCH_SEC),
        Some(&monitor),
    )
    .await;
    write_handle.await.unwrap();
    info!("Write {} batches completed", NB_BATCH);
    read_handle.await.unwrap();
    info!("Read {} batches completed", NB_BATCH);
    info!("Monitor: {:?}", monitor);
}
