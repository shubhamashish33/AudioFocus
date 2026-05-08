use std::path::PathBuf;

use chrono::Local;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::error::{AudioFocusError, Result};

pub fn init() -> Result<WorkerGuard> {
    let log_dir = log_directory();
    std::fs::create_dir_all(&log_dir)
        .map_err(|error| AudioFocusError::Logging(error.to_string()))?;

    let file_appender = tracing_appender::rolling::daily(&log_dir, "audiofocus.jsonl");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let file_layer = fmt::layer()
        .json()
        .with_current_span(false)
        .with_span_list(false)
        .with_writer(non_blocking);

    let stdout_layer = fmt::layer().compact().with_target(false);

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer)
        .try_init()
        .map_err(|error| AudioFocusError::Logging(error.to_string()))?;

    tracing::info!(
        log_dir = %log_dir.display(),
        started_at = %Local::now().to_rfc3339(),
        "structured logging initialized"
    );

    Ok(guard)
}

fn log_directory() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("AudioFocus")
        .join("logs")
}
