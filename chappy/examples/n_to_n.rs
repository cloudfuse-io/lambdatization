use helpers::send_pseudo_random;

use log::{debug, error, info};
use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

fn handle_client(mut stream: TcpStream) {
    // read 20 bytes at a time from stream echoing back to stream
    let mut bytes_echoed = 0;
    loop {
        let mut read = [0; 16 * 1028];
        match stream.read(&mut read) {
            Ok(n) => {
                bytes_echoed += n;
                if n == 0 {
                    stream.flush().unwrap();
                    debug!("Stream EOF, bytes echoed: {}", bytes_echoed);
                    break;
                }
                stream.write_all(&read[0..n]).unwrap();
            }
            Err(err) => {
                panic!("{:?}", err);
            }
        }
    }
}

fn start_server() {
    info!("Starting Server");
    let listener = TcpListener::bind("localhost:8080").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                info!(
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

#[allow(non_snake_case)]
fn main() {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();

    let virtual_ip = env::var("CHAPPY_VIRTUAL_IP").unwrap();
    let cluster_ips = env::var("CLUSTER_IPS").unwrap();
    let BATCH_SIZE: usize = env::var("BATCH_SIZE").unwrap().parse().unwrap();
    let BYTES_SENT: usize = env::var("BYTES_SENT").unwrap().parse().unwrap();
    let NB_BATCH: usize = BYTES_SENT / BATCH_SIZE;
    info!("Running {} in cluster [{}]", virtual_ip, cluster_ips);
    info!(
        "Exchanging {} bytes in {} batches of size {} with each node",
        BYTES_SENT, NB_BATCH, BATCH_SIZE
    );

    thread::spawn(start_server);
    thread::sleep(Duration::from_millis(100));

    let mut handles: Vec<(thread::JoinHandle<()>, thread::JoinHandle<()>)> = Vec::new();
    for ip in cluster_ips.split(',') {
        let srv_addr = format!("{}:{}", ip, 8080);
        let stream = TcpStream::connect(&srv_addr).unwrap();
        info!("Connection to echo server {} successful", srv_addr);
        let (write_handle, read_handle) = send_pseudo_random(stream, BATCH_SIZE, NB_BATCH, 1);
        handles.push((write_handle, read_handle));
    }

    for (write_handle, read_handle) in handles {
        write_handle.join().unwrap();
        info!("Write {} Bytes completed", BYTES_SENT);
        read_handle.join().unwrap();
        info!("Read {} Bytes completed", BYTES_SENT);
    }
    info!("Reads and writes completed, waiting for other clients to complete...");
    thread::sleep(Duration::from_secs(1));
    info!("Completed!");
}
