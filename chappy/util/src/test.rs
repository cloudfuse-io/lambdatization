use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

/// Get some available ports from the OS by probing with shortlived TCP servers.
///
/// Note that the port being free at the time this function completes does not
/// guaranty the it will still be the case when the caller tries to bind to it.
pub async fn available_ports(number: usize) -> Vec<u16> {
    let (tx, mut rx) = mpsc::channel(number);

    // Spawn as many servers as requested ports in parallel, to avoid that the
    // OS reassigns the same port multiple times.
    #[allow(clippy::needless_collect)]
    let spawned_servers: Vec<(oneshot::Sender<()>, JoinHandle<()>)> = (0..number)
        .map(|_| {
            let tx = tx.clone();
            let (resp_tx, resp_rx) = oneshot::channel();
            let handle = tokio::spawn(async move {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let port = listener.local_addr().unwrap().port();
                tx.send(port).await.unwrap();
                drop(tx);
                resp_rx.await.unwrap();
            });
            (resp_tx, handle)
        })
        .collect();
    drop(tx);

    // First gather all the ports and only then send the shutdown signal to the
    // servers
    let mut ports = Vec::with_capacity(number);
    while let Some(port) = rx.recv().await {
        ports.push(port);
    }
    let handles = spawned_servers
        .into_iter()
        .map(|(tx, handle)| {
            tx.send(()).unwrap();
            handle
        })
        .collect::<Vec<_>>();

    // Wait for all the servers to be dropped to make sure the ports can now be
    // reused
    for handle in handles {
        handle.await.unwrap();
    }
    ports
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[tokio::test]
    async fn test_available_ports() {
        let target_len = 10;
        let ports = available_ports(target_len).await;
        assert_eq!(ports.len(), target_len);
        ports
            .iter()
            .for_each(|&p| assert!(p > 1024, "not in the RFC 6056 ephemeral range"));
        let set = HashSet::<_>::from_iter(ports.into_iter());
        assert_eq!(set.len(), target_len);
        for port in set {
            TcpListener::bind(format!("127.0.0.1:{}", port))
                .await
                .unwrap();
        }
    }
}
