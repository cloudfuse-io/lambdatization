use chrono::prelude::{DateTime, Utc};
use tracing_subscriber::fmt::{format::Writer, time::FormatTime};
use tracing_subscriber::EnvFilter;

struct CustomTime;

impl FormatTime for CustomTime {
    fn format_time(&self, w: &mut Writer<'_>) -> core::fmt::Result {
        let dt: DateTime<Utc> = std::time::SystemTime::now().into();
        write!(w, "{}", dt.format("%H:%M:%S%.3f"))
    }
}

/// Configure and init tracing for executables
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .with_timer(CustomTime)
        .init();
}

/// Configure and init tracing with some tweeks specific to shared libraries
pub fn init_tracing_shared_lib() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_timer(CustomTime)
        .try_init()
        .ok();
}
