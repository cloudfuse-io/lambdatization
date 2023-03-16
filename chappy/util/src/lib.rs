use chrono::prelude::{DateTime, Utc};
use tracing_subscriber::fmt::{format::Writer, time::FormatTime};

pub struct CustomTime;

impl FormatTime for CustomTime {
    fn format_time(&self, w: &mut Writer<'_>) -> core::fmt::Result {
        let dt: DateTime<Utc> = std::time::SystemTime::now().into();
        write!(w, "{}", dt.format("%H:%M:%S%.3f"))
    }
}
