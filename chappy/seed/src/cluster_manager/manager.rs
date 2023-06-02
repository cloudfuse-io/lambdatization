use super::message::*;
use super::state::*;
use super::summary::*;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, warn};

pub struct ClusterManager {
    tx: mpsc::UnboundedSender<(String, Message)>,
}

struct ClusterMap(HashMap<String, TracedClusterState>);

impl ClusterMap {
    fn forward(&mut self, cluster_id: String, message: Message) {
        if let Message::BindNodeStart { cluster_size, .. } = &message {
            let state = self
                .0
                .entry(cluster_id.to_owned())
                .and_modify(|cluster_state| {
                    if cluster_state.finished() {
                        warn!(cluster_id, "Cluster already exists, replacing it...");
                        *cluster_state = TracedClusterState::new(&cluster_id, *cluster_size);
                    }
                })
                .or_insert_with(|| {
                    debug!(cluster_id, "Creating new cluster");
                    TracedClusterState::new(&cluster_id, *cluster_size)
                });
            state.update(message);
        } else if let Some(state) = self.0.get_mut(&cluster_id) {
            state.update(message);
        } else {
            error!(
                cluster_id,
                ?message,
                "Cluster not found, message forwarding failed"
            );
        }
    }
}

pub struct ClusterManagerTask(JoinHandle<()>);

impl ClusterManagerTask {
    /// Wait for the manager to be dropped and all messages processed
    pub async fn wait(self) {
        self.0.await.unwrap();
    }
}

impl ClusterManager {
    async fn event_loop(mut rx: mpsc::UnboundedReceiver<(String, Message)>) {
        let mut clusters = ClusterMap(HashMap::new());
        while let Some((cluster_id, msg)) = rx.recv().await {
            clusters.forward(cluster_id, msg);
        }
    }

    pub fn send(&self, cluster_id: String, message: Message) {
        self.tx.send((cluster_id, message)).unwrap();
    }

    pub async fn get_summary(&self, cluster_id: String) -> Summary {
        let (msg, rx) = Message::get_summary();
        self.send(cluster_id, msg);
        rx.await.unwrap()
    }

    pub fn new() -> (Self, ClusterManagerTask) {
        let (tx, rx) = mpsc::unbounded_channel();
        let task = ClusterManagerTask(tokio::spawn(Self::event_loop(rx)));
        (Self { tx }, task)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[tokio::test]
    async fn test_manager() {
        let instant_1 = Utc.with_ymd_and_hms(2023, 5, 17, 16, 15, 30).unwrap();
        let instant_2 = Utc.with_ymd_and_hms(2023, 5, 17, 16, 15, 31).unwrap();
        let (manager, manager_task) = ClusterManager::new();
        let cluster_id = "cluster_id_1";

        let msg = Message::BindNodeStart {
            cluster_size: 2,
            time: instant_1,
            virt_ip: String::from("192.168.0.1"),
        };
        manager.send(cluster_id.to_owned(), msg);

        let msg = Message::BindNodeStart {
            cluster_size: 2,
            time: instant_1,
            virt_ip: String::from("192.168.0.2"),
        };
        manager.send(cluster_id.to_owned(), msg);

        let msg = Message::BindNodeEnd {
            time: instant_2,
            virt_ip: String::from("192.168.0.1"),
        };
        manager.send(cluster_id.to_owned(), msg);

        let msg = Message::BindNodeEnd {
            time: instant_2,
            virt_ip: String::from("192.168.0.2"),
        };
        manager.send(cluster_id.to_owned(), msg);

        let Summary { node, interval } = manager.get_summary(cluster_id.to_owned()).await;
        assert_eq!(
            &format!("{:?}", interval),
            "starts: 0ns, ends: 0ns"
        );
        assert_eq!(&format!("{:?}", node), "2 expected, 2 started, 2 ended");

        drop(manager);
        manager_task.wait().await;
    }
}
