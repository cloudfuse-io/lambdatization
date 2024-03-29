use chrono::prelude::{DateTime, Utc};
use opentelemetry_otlp::WithExportConfig;
use tracing_subscriber::{
    fmt::{format::Writer, time::FormatTime},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

struct CustomTime;

impl FormatTime for CustomTime {
    fn format_time(&self, w: &mut Writer<'_>) -> core::fmt::Result {
        let dt: DateTime<Utc> = std::time::SystemTime::now().into();
        write!(w, "{}", dt.format("%H:%M:%S%.3f"))
    }
}

/// Configure and init tracing for executables
pub fn init_tracing(service_name: &str) {
    let reg = tracing_subscriber::registry().with(EnvFilter::from_default_env());

    let mut fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_timer(CustomTime);
    fmt_layer.set_ansi(false);
    let reg = reg.with(fmt_layer);

    let otlp_config = (
        std::env::var("CHAPPY_OPENTELEMETRY_URL"),
        std::env::var("CHAPPY_OPENTELEMETRY_AUTHORIZATION"),
    );
    let otlp_layer = if let (Ok(ot_url), Ok(ot_auth)) = otlp_config {
        let headers = std::collections::HashMap::from([("Authorization".into(), ot_auth)]);
        let exporter = opentelemetry_otlp::new_exporter()
            .http()
            .with_endpoint(ot_url)
            .with_headers(headers);
        let otlp_tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(exporter)
            .with_trace_config(opentelemetry::sdk::trace::config().with_resource(
                opentelemetry::sdk::Resource::new(vec![opentelemetry::KeyValue::new(
                    "service.name",
                    service_name.to_owned(),
                )]),
            ))
            .install_batch(opentelemetry::runtime::TokioCurrentThread)
            .unwrap();
        let tracing_layer = tracing_opentelemetry::layer().with_tracer(otlp_tracer);
        Some(tracing_layer)
    } else {
        None
    };
    let reg = reg.with(otlp_layer);

    reg.init();
}

/// Configure and init tracing with some tweeks specific to shared libraries
pub fn init_tracing_shared_lib() {
    let mut fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_timer(CustomTime);
    fmt_layer.set_ansi(false);

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(EnvFilter::from_default_env())
        .try_init()
        .ok();
}

pub fn close_tracing() {
    opentelemetry::global::shutdown_tracer_provider();
}
