use chrono::{DateTime, Utc};
use tokio::sync::oneshot;

use super::summary::*;

#[derive(Debug)]
pub enum Message {
    BindNodeStart {
        time: DateTime<Utc>,
        cluster_size: u32,
        virt_ip: String,
    },
    BindNodeEnd {
        time: DateTime<Utc>,
        virt_ip: String,
    },
    BindServerStart {
        virt_ip: String,
    },
    BindServerResponse {
        virt_ip: String,
    },
    // BindServerEnd usually arrives too late
    BindClientStart {
        src_virt_ip: String,
        tgt_virt_ip: String,
    },
    BindClientEnd {
        src_virt_ip: String,
        tgt_virt_ip: String,
    },
    GetSummary {
        tx: oneshot::Sender<Summary>,
    },
}

impl Message {
    pub fn now() -> DateTime<Utc> {
        Utc::now()
    }

    pub fn get_summary() -> (Message, oneshot::Receiver<Summary>) {
        let (tx, rx) = oneshot::channel();
        (Message::GetSummary { tx }, rx)
    }
}
