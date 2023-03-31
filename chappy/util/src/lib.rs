use chrono::prelude::{DateTime, Utc};
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
pub fn init_tracing(service_name: &'static str) {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_timer(CustomTime);

    let reg = tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(fmt_layer);

    // if correspongind host is configured, add a Zipkin layer
    let zipkin_layer = if let Ok(ot_host) = std::env::var("CHAPPY_OPENTELEMETRY_HOSTNAME") {
        let tracer = opentelemetry_zipkin::new_pipeline()
            .with_collector_endpoint(format!("http://{}:9411/api/v2/spans", ot_host))
            .with_service_name(service_name)
            .install_batch(opentelemetry::runtime::TokioCurrentThread)
            .unwrap();
        // let tracer = opentelemetry::sdk::export::trace::stdout::new_pipeline()
        //     .with_writer(std::io::stderr())
        //     .install_simple();
        let ot_layer = tracing_opentelemetry::layer().with_tracer(tracer);
        Some(ot_layer)
    } else {
        None
    };

    let otlp_layer = if let Ok(ot_key) = std::env::var("CHAPPY_OPENTELEMETRY_APIKEY") {
        use opentelemetry_otlp::WithExportConfig;
        let exporter = opentelemetry_otlp::new_exporter()
            .http()
            .with_endpoint("https://otelcol.aspecto.io/v1/traces")
            .with_headers(std::collections::HashMap::from([(
                "Authorization".into(),
                ot_key,
            )]));
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

    reg.with(zipkin_layer).with(otlp_layer).init();
}

/// Configure and init tracing with some tweeks specific to shared libraries
pub fn init_tracing_shared_lib() {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_timer(CustomTime);

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(EnvFilter::from_default_env())
        .try_init()
        .ok();
}

pub fn close_tracing() {
    opentelemetry::global::shutdown_tracer_provider();
}
