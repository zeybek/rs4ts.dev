//! Structured logging setup via `tracing` + `tracing-subscriber`.
//!
//! `tracing` is to Rust what `pino`/`winston` are to Node, but it is built
//! around *spans* (timed, nested units of work) as well as flat events. The
//! `#[tracing::instrument]` attributes on the handlers create those spans; the
//! subscriber configured here decides how they are rendered.

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::config::{LogFormat, Settings};

/// Install the global tracing subscriber based on the configured log format.
///
/// Call this exactly once, as early in `main` as possible, so that even
/// startup messages are captured.
pub fn init(settings: &Settings) {
    let filter = EnvFilter::new(&settings.log_filter);
    let registry = tracing_subscriber::registry().with(filter);

    match settings.log_format {
        LogFormat::Json => registry
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_target(true),
            )
            .init(),
        LogFormat::Pretty => registry
            .with(tracing_subscriber::fmt::layer().with_target(true))
            .init(),
    }
}
