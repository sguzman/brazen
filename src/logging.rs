use std::path::Path;
use std::sync::OnceLock;

use chrono::Local;
use thiserror::Error;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};

use crate::config::LoggingConfig;

static TRACING_GUARD: OnceLock<WorkerGuard> = OnceLock::new();
static TRACING_READY: OnceLock<()> = OnceLock::new();

static LOG_BROADCASTER: OnceLock<tokio::sync::broadcast::Sender<String>> = OnceLock::new();

pub fn get_log_receiver() -> tokio::sync::broadcast::Receiver<String> {
    LOG_BROADCASTER
        .get_or_init(|| {
            let (tx, _) = tokio::sync::broadcast::channel(1024);
            tx
        })
        .subscribe()
}

struct BroadcastLayer;

impl<S> Layer<S> for BroadcastLayer
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut buf = String::new();
        let mut visitor = LogVisitor(&mut buf);
        event.record(&mut visitor);

        if let Some(tx) = LOG_BROADCASTER.get() {
            let meta = event.metadata();
            let log_line = format!(
                "{} [{}] {}: {}",
                Local::now().format("%H:%M:%S%.3f"),
                meta.level(),
                meta.target(),
                buf
            );
            let _ = tx.send(log_line);
        }
    }
}

struct LogVisitor<'a>(&'a mut String);

impl<'a> tracing::field::Visit for LogVisitor<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            use std::fmt::Write;
            let _ = write!(self.0, "{:?}", value);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggingPlan {
    pub console_filter: String,
    pub file_filter: String,
    pub file_name_prefix: String,
}

impl LoggingPlan {
    pub fn from_config(config: &LoggingConfig) -> Self {
        Self {
            console_filter: config.console_filter.clone(),
            file_filter: config.file_filter.clone(),
            file_name_prefix: config.file_name_prefix.clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum LoggingError {
    #[error("failed to create logs directory {path}: {source}")]
    CreateLogsDir {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid console log filter `{filter}`: {source}")]
    ConsoleFilter {
        filter: String,
        #[source]
        source: tracing_subscriber::filter::ParseError,
    },
    #[error("invalid file log filter `{filter}`: {source}")]
    FileFilter {
        filter: String,
        #[source]
        source: tracing_subscriber::filter::ParseError,
    },
    #[error("failed to install tracing subscriber")]
    InstallGlobalSubscriber,
}

pub fn init_tracing(config: &LoggingConfig, logs_dir: &Path) -> Result<(), LoggingError> {
    if TRACING_READY.get().is_some() {
        return Ok(());
    }

    std::fs::create_dir_all(logs_dir).map_err(|source| LoggingError::CreateLogsDir {
        path: logs_dir.display().to_string(),
        source,
    })?;

    let plan = LoggingPlan::from_config(config);
    let console_filter =
        EnvFilter::try_new(&plan.console_filter).map_err(|source| LoggingError::ConsoleFilter {
            filter: plan.console_filter.clone(),
            source,
        })?;
    let file_filter =
        EnvFilter::try_new(&plan.file_filter).map_err(|source| LoggingError::FileFilter {
            filter: plan.file_filter.clone(),
            source,
        })?;

    let file_name = build_log_file_name(&plan.file_name_prefix);
    let file_appender = tracing_appender::rolling::never(logs_dir, file_name);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let _ = TRACING_GUARD.set(guard);

    let console_layer = fmt::layer()
        .with_target(true)
        .with_ansi(config.ansi)
        .with_filter(console_filter);
    let file_layer = fmt::layer()
        .with_ansi(false)
        .with_writer(non_blocking)
        .with_target(true)
        .with_filter(file_filter);

    let subscriber = Registry::default()
        .with(console_layer)
        .with(file_layer)
        .with(BroadcastLayer);
    if tracing::subscriber::set_global_default(subscriber).is_err() {
        let _ = TRACING_READY.set(());
        return Ok(());
    }

    let _ = TRACING_READY.set(());
    tracing::info!(target: "brazen::logging", path = %logs_dir.display(), "tracing initialized");
    Ok(())
}

fn build_log_file_name(prefix: &str) -> String {
    let base = prefix.trim_end_matches(".log").trim();
    let base = if base.is_empty() { "brazen" } else { base };
    let timestamp = Local::now().format("%Y%m%d-%H%M%S");
    format!("{base}-{timestamp}.log")
}

#[cfg(test)]
mod tests {
    use super::build_log_file_name;

    #[test]
    fn log_file_name_includes_timestamp() {
        let name = build_log_file_name("brazen");
        assert!(name.starts_with("brazen-"));
        assert!(name.ends_with(".log"));
        let mid = name.trim_start_matches("brazen-").trim_end_matches(".log");
        assert_eq!(mid.len(), "YYYYMMDD-HHMMSS".len());
        assert!(mid.chars().all(|c| c.is_ascii_digit() || c == '-'));
    }
}
