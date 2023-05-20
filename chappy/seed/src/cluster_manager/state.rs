use super::message::*;
use super::summary::*;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use tracing::{debug_span, error, info, Span};

type UtcTime = DateTime<Utc>;

pub struct NodeState {
    pub start_time: UtcTime,
    pub end_time: Option<UtcTime>,
}

pub struct TracedNodeState {
    state: NodeState,
    span: Option<Span>,
}

pub struct ClusterState {
    pub expected_size: u32,
    pub nodes: HashMap<String, TracedNodeState>,
    pub finished_nodes: u32,
}

pub struct TracedClusterState {
    state: ClusterState,
    span: Option<Span>,
}

impl TracedClusterState {
    pub fn new(cluster_id: &str, cluster_size: u32) -> Self {
        Self {
            state: ClusterState {
                expected_size: cluster_size,
                nodes: HashMap::new(),
                finished_nodes: 0,
            },
            span: Some(debug_span!("cluster", cluster_id)),
        }
    }

    pub fn update(&mut self, message: Message) {
        let _cluster_enter = self.span.as_ref().map(|s| s.enter());
        match message {
            Message::BindNodeStart {
                cluster_size,
                virt_ip,
                time,
            } => {
                if cluster_size != self.state.expected_size {
                    error!("Declared cluster sizes diverge between member nodes");
                }
                self.state.add_traced_node(
                    &virt_ip,
                    NodeState {
                        start_time: time,
                        end_time: None,
                    },
                )
            }
            Message::BindNodeEnd { virt_ip, time } => {
                self.state.edit_node(&virt_ip, "BindNodeEnd", true, |n| {
                    n.end_time = Some(time);
                });
                self.state.finished_nodes += 1;
            }
            Message::GetSummary { tx } => {
                if let Err(err) = tx.send(self.state.summary()) {
                    error!("caller dropped before getting its summary: {:?}", err)
                }
            }
            Message::BindClientStart {
                src_virt_ip,
                tgt_virt_ip,
            } => {
                self.state
                    .edit_node(&src_virt_ip, "BindClientStart", false, |_| {
                        info!(tgt_virt_ip, "client start")
                    });
            }
            Message::BindClientEnd {
                src_virt_ip,
                tgt_virt_ip,
            } => self
                .state
                .edit_node(&src_virt_ip, "BindClientEnd", false, |_| {
                    info!(tgt_virt_ip, "client end")
                }),
            Message::BindServerStart { virt_ip } => {
                self.state
                    .edit_node(&virt_ip, "BindServerStart", false, |_| {
                        info!("server start")
                    })
            }
            Message::BindServerResponse { virt_ip } => {
                self.state
                    .edit_node(&virt_ip, "BindServerResponse", false, |_| {
                        info!("server response")
                    })
            }
        }
        if self.finished() {
            // drop the cluster span to close it
            drop(_cluster_enter);
            self.span.take();
        }
    }

    pub fn finished(&self) -> bool {
        self.state.finished()
    }
}

impl ClusterState {
    pub fn add_traced_node(&mut self, virt_ip: &str, state: NodeState) {
        self.nodes
            .entry(virt_ip.to_owned())
            .and_modify(|_| error!(virt_ip, "Ip already bound"))
            .or_insert(TracedNodeState {
                state,
                span: Some(debug_span!("node", virt_ip)),
            });
    }

    pub fn edit_node<F>(&mut self, virt_ip: &str, err_msg: &str, close: bool, f: F)
    where
        F: FnOnce(&mut NodeState),
    {
        if let Some(node) = self.nodes.get_mut(virt_ip) {
            let _e = node.span.as_ref().map(|s| s.enter());
            f(&mut node.state);
            if close {
                // drop the node span to close it
                drop(_e);
                node.span.take();
            }
        } else {
            error!(virt_ip, "{} failed", err_msg)
        }
    }

    pub fn finished(&self) -> bool {
        self.finished_nodes == self.expected_size
    }

    fn comp<F, T>(op: F, a: Option<T>, b: Option<T>) -> Option<T>
    where
        F: FnOnce(Option<T>, Option<T>) -> Option<T>,
    {
        match (&a, &b) {
            (Some(_), Some(_)) => op(a, b),
            (Some(_), None) => a,
            (None, Some(_)) => b,
            (None, None) => None,
        }
    }

    pub fn interval_summary(&self) -> IntervalSummary {
        if let Some(val) = self.nodes.values().next() {
            let mut first_node_start = val.state.start_time;
            let mut last_node_start = val.state.start_time;
            let mut first_node_end = val.state.end_time;
            let mut last_node_end = val.state.end_time;
            for node in self.nodes.values() {
                first_node_start = first_node_start.min(node.state.start_time);
                last_node_start = last_node_start.max(node.state.start_time);
                first_node_end =
                    Self::comp(Option::<UtcTime>::min, first_node_end, node.state.end_time);
                last_node_end =
                    Self::comp(Option::<UtcTime>::max, last_node_end, node.state.end_time);
            }
            IntervalSummary::Some {
                first_node_start,
                last_node_start,
                first_node_end,
                last_node_end,
            }
        } else {
            IntervalSummary::Empty
        }
    }

    pub fn node_summary(&self) -> NodeSummary {
        NodeSummary {
            expected_size: self.expected_size,
            nodes: self.nodes.len() as u32,
            finished_nodes: self.finished_nodes,
        }
    }

    pub fn summary(&self) -> Summary {
        Summary {
            interval: self.interval_summary(),
            node: self.node_summary(),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[tokio::test]
    async fn test_state_updates() {
        chappy_util::init_tracing("test_state_update");
        let mut state = TracedClusterState::new("cluster_id", 2);
        let instant_1 = Utc.with_ymd_and_hms(2023, 5, 12, 10, 15, 30).unwrap();
        let instant_2 = Utc.with_ymd_and_hms(2023, 5, 12, 10, 15, 31).unwrap();
        let instant_3 = Utc.with_ymd_and_hms(2023, 5, 12, 10, 15, 32).unwrap();
        let instant_4 = Utc.with_ymd_and_hms(2023, 5, 12, 10, 15, 33).unwrap();

        state.update(Message::BindNodeStart {
            time: instant_1,
            cluster_size: 2,
            virt_ip: String::from("192.68.0.1"),
        });

        let node = state
            .state
            .nodes
            .get("192.68.0.1")
            .expect("Node 192.68.0.1 should have been added");
        assert!(node.span.is_some());
        assert!(state.span.is_some());
        assert!(!state.finished());
        assert_eq!(
            &format!("{:?}", state.state.interval_summary()),
            "starts: 2023-05-12T10:15:30Z -> 2023-05-12T10:15:30Z, ends: None -> None"
        );
        assert_eq!(
            &format!("{:?}", state.state.node_summary()),
            "2 expected, 1 started, 0 ended"
        );

        state.update(Message::BindNodeStart {
            time: instant_2,
            cluster_size: 2,
            virt_ip: String::from("192.68.0.2"),
        });

        let node = state
            .state
            .nodes
            .get("192.68.0.2")
            .expect("Node 192.68.0.2 should have been added");
        assert!(node.span.is_some());
        assert!(state.span.is_some());
        assert!(!state.finished());
        assert_eq!(
            &format!("{:?}", state.state.interval_summary()),
            "starts: 2023-05-12T10:15:30Z -> 2023-05-12T10:15:31Z, ends: None -> None"
        );
        assert_eq!(
            &format!("{:?}", state.state.node_summary()),
            "2 expected, 2 started, 0 ended"
        );

        state.update(Message::BindNodeEnd {
            time: instant_3,
            virt_ip: String::from("192.68.0.2"),
        });

        assert_eq!(state.state.expected_size, 2);
        let node = state
            .state
            .nodes
            .get("192.68.0.2")
            .expect("Node 192.68.0.2 should not have been removed");
        assert!(node.span.is_none());
        assert!(state.span.is_some());
        assert!(!state.finished());
        assert_eq!(
            &format!("{:?}", state.state.interval_summary()),
            "starts: 2023-05-12T10:15:30Z -> 2023-05-12T10:15:31Z, ends: Some(2023-05-12T10:15:32Z) -> Some(2023-05-12T10:15:32Z)"
        );
        assert_eq!(
            &format!("{:?}", state.state.node_summary()),
            "2 expected, 2 started, 1 ended"
        );

        state.update(Message::BindNodeEnd {
            time: instant_4,
            virt_ip: String::from("192.68.0.1"),
        });

        let node = state
            .state
            .nodes
            .get("192.68.0.1")
            .expect("Node 192.68.0.1 should not have been removed");
        assert!(node.span.is_none());
        assert!(state.span.is_none());
        assert!(state.finished());
        assert_eq!(
            &format!("{:?}", state.state.interval_summary()),
            "starts: 2023-05-12T10:15:30Z -> 2023-05-12T10:15:31Z, ends: Some(2023-05-12T10:15:32Z) -> Some(2023-05-12T10:15:33Z)"
        );
        assert_eq!(
            &format!("{:?}", state.state.node_summary()),
            "2 expected, 2 started, 2 ended"
        );
    }
}
