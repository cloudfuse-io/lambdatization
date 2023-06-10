use chappy_perforator::{
    binding_service::BindingService,
    forwarder::Forwarder,
    metrics,
    perforator::Perforator,
    print_metrics,
    shutdown::{gracefull, GracefullyRunnable, Shutdown},
    CHAPPY_CONF,
};
use chappy_util::{close_tracing, init_tracing};
use std::{sync::Arc, time::Duration};
use tonic::async_trait;
use tracing::{info, info_span, Instrument};

struct SrvRunnable;

#[async_trait]
impl GracefullyRunnable for SrvRunnable {
    async fn run(&self, shutdown: &Shutdown) {
        let tcp_port = 5000;
        let seed_addr = format!("{}:{}", CHAPPY_CONF.seed_hostname, CHAPPY_CONF.seed_port);
        let quic_port = 5001;
        info!(
            perforator_tcp_port = tcp_port,
            perforator_quic_port = quic_port,
            seed_address = %seed_addr
        );

        let forwarder = Arc::new(Forwarder::new(quic_port));
        let binding_service = Arc::new(BindingService::new(quic_port));
        let perforator = Arc::new(Perforator::new(
            Arc::clone(&forwarder),
            binding_service,
            tcp_port,
        ));
        let node_binding = perforator.bind_node(shutdown.create_guard()).await;
        tokio::join!(
            perforator.run_tcp_server(shutdown),
            forwarder.run_quic_server(shutdown),
        );
        print_metrics();
        node_binding.close().await;
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    init_tracing(&format!("perf-{}", CHAPPY_CONF.virtual_ip));

    metrics(
        gracefull(SrvRunnable, Duration::from_secs(1))
            .instrument(info_span!("perforator", virt_ip = CHAPPY_CONF.virtual_ip)),
    )
    .await;
    close_tracing();
}
