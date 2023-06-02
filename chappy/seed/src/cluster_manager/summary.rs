use chrono::{DateTime, Utc};
use std::fmt;

pub enum IntervalSummary {
    Some {
        first_node_start: DateTime<Utc>,
        last_node_start: DateTime<Utc>,
        first_node_end: Option<DateTime<Utc>>,
        last_node_end: Option<DateTime<Utc>>,
    },
    Empty,
}

pub struct NodeSummary {
    pub expected_size: u32,
    pub nodes: u32,
    pub finished_nodes: u32,
}

pub struct Summary {
    pub interval: IntervalSummary,
    pub node: NodeSummary,
}

impl fmt::Debug for IntervalSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            IntervalSummary::Some {
                first_node_start,
                last_node_start,
                first_node_end,
                last_node_end,
            } => {
                let start_interval = last_node_start.signed_duration_since(*first_node_start);
                if let (Some(fne), Some(lne)) = (first_node_end, last_node_end) {
                    let end_interval = lne.signed_duration_since(*fne);
                    write!(
                        f,
                        "starts: {:?}, ends: {:?}",
                        start_interval.to_std().unwrap(),
                        end_interval.to_std().unwrap(),
                    )
                } else {
                    write!(f, "starts: {:?}, no end", start_interval.to_std().unwrap())
                }
            }
            IntervalSummary::Empty => f.write_str("Empty cluster"),
        }
    }
}

impl fmt::Debug for NodeSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{} expected, {} started, {} ended",
            self.expected_size, self.nodes, self.finished_nodes
        )
    }
}

impl fmt::Debug for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{:?} ({:?})", self.interval, self.node)
    }
}
