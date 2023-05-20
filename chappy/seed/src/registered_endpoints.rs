use crate::Address;
use chappy_util::awaitable_map::AwaitableMap;
use std::{net::SocketAddr, time::Duration};
use tokio::sync::mpsc::UnboundedSender;
use tokio::{sync::mpsc, time::timeout};
use tonic::{Result, Status};
use tracing::{error, info};

#[derive(PartialEq, Eq, Hash, Clone)]
struct VirtualTarget {
    cluster_id: String,
    ip: String,
}

#[derive(Clone)]
pub struct ResolvedTarget {
    pub natted_address: SocketAddr,
    pub punch_req_stream: mpsc::UnboundedSender<Address>,
    pub server_certificate: Vec<u8>,
}

/// Map virtual addresses to the NATed endpoint and punch request stream
pub struct RegisteredEndpoints(AwaitableMap<VirtualTarget, ResolvedTarget>);

impl RegisteredEndpoints {
    pub fn new() -> Self {
        Self(AwaitableMap::new())
    }

    pub async fn get(
        &self,
        tgt_ip: &str,
        cluster_id: &str,
    ) -> Result<ResolvedTarget, tonic::Status> {
        let virtual_target_key = VirtualTarget {
            ip: tgt_ip.to_owned(),
            cluster_id: cluster_id.to_owned(),
        };

        // TODO adjust timout duration
        let resolved_target_timeout = timeout(
            Duration::from_secs(10),
            self.0.get(virtual_target_key, |prev_tgt| {
                if prev_tgt.punch_req_stream.is_closed() {
                    info!(ip = tgt_ip, cluster_id, "replace closed target");
                    true
                } else {
                    false
                }
            }),
        )
        .await;

        let resolved_target = if let Ok(target) = resolved_target_timeout {
            target
        } else {
            let msg = "Target ip could not be resolved";
            error!(msg);
            return Err(Status::not_found(msg));
        };

        Ok(resolved_target)
    }

    pub fn insert(
        &self,
        server_nated_addr: SocketAddr,
        req_tx: UnboundedSender<Address>,
        server_certificate: &[u8],
        registered_ip: &str,
        cluster_id: &str,
    ) {
        let resolved_target = ResolvedTarget {
            natted_address: server_nated_addr,
            punch_req_stream: req_tx,
            server_certificate: server_certificate.to_vec(),
        };
        let virtual_target_key = VirtualTarget {
            ip: registered_ip.to_owned(),
            cluster_id: cluster_id.to_owned(),
        };

        // replace the new target in the registered endpoint map
        if let Some(prev_tgt) = self.0.insert(virtual_target_key.clone(), resolved_target) {
            if prev_tgt.punch_req_stream.is_closed() {
                info!(
                    ip = virtual_target_key.ip,
                    cluster = virtual_target_key.cluster_id,
                    "replaced closed target"
                );
            } else {
                error!(
                    ip = virtual_target_key.ip,
                    cluster = virtual_target_key.cluster_id,
                    "replaced unclosed target"
                );
            }
        }
    }
}
