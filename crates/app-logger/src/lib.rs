use std::{
    env,
    fs::OpenOptions,
    io::{IsTerminal, stderr},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use app_config::LogFormat;
use tracing::{Level, debug, trace, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, Layer, Registry, filter::Directive, fmt, prelude::*};

use crate::maybe_writer::MaybeFileWriter;

mod file_fields;
mod maybe_writer;

use file_fields::{FileFields, JsonFileFields};

type StderrWriter = fn() -> std::io::Stderr;
type FileWriterFn = Box<dyn Fn() -> MaybeFileWriter + Send + Sync>;

struct ReloadBridge {
    apply: Box<dyn Fn(EnvFilter) -> Result<(), String> + Send + Sync>,
    current: Box<dyn Fn() -> Result<EnvFilter, String> + Send + Sync>,
}

impl ReloadBridge {
    fn set_filter(&self, filter: EnvFilter) -> Result<(), String> {
        (self.apply)(filter)
    }

    fn current_filter(&self) -> Result<EnvFilter, String> {
        (self.current)()
    }
}

static CONSOLE_RELOAD_HANDLE: OnceLock<ReloadBridge> = OnceLock::new();
static FILE_RELOAD_HANDLE: OnceLock<ReloadBridge> = OnceLock::new();
static FILE_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

pub const COMPONENT_LEVELS: &[(&str, Level)] = &[
    // Binaries
    // Libraries
    // Other
    // External
];

#[derive(Debug, Clone)]
pub struct LogOptions {
    log_level: Option<Level>,
    file_log_level: Option<Level>,
    log_file: Option<PathBuf>,
    console_format: LogFormat,
    file_format: LogFormat,
}

impl LogOptions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_log_file(mut self, log_file: Option<PathBuf>) -> Self {
        self.log_file = log_file;
        self
    }

    #[must_use]
    pub const fn with_console_format(mut self, console_format: LogFormat) -> Self {
        self.console_format = console_format;
        self
    }

    #[must_use]
    pub const fn with_file_format(mut self, file_format: LogFormat) -> Self {
        self.file_format = file_format;
        self
    }

    #[must_use]
    pub const fn with_log_level(mut self, log_level: Option<Level>) -> Self {
        self.log_level = log_level;
        self
    }

    #[must_use]
    pub const fn with_file_log_level(mut self, file_log_level: Option<Level>) -> Self {
        self.file_log_level = file_log_level;
        self
    }
}

impl Default for LogOptions {
    fn default() -> Self {
        Self {
            log_level: Some(Level::INFO),
            file_log_level: None,
            log_file: None,
            console_format: LogFormat::Pretty,
            file_format: LogFormat::Plain,
        }
    }
}

/// Initialize the logger
///
/// # Panics
/// Panics if the logger fails to initialize
pub fn init() {
    init_with_options(LogOptions::default());
}

pub fn init_with_options(options: LogOptions) {
    let levels = build_levels(options.log_level);
    let file_levels = build_levels(options.file_log_level.or(options.log_level));

    init_with(levels, file_levels, options);
}

pub fn init_with<T, U>(levels: T, file_levels: U, options: LogOptions)
where
    T: IntoIterator<Item = (&'static str, Level)>,
    U: IntoIterator<Item = (&'static str, Level)>,
{
    let console_env_filter =
        build_env_filter(levels, options.log_level, "DOWNLOADER_HUB_LOG_LEVEL", None);
    let file_env_filter = build_env_filter(
        file_levels,
        options.file_log_level.or(options.log_level),
        "DOWNLOADER_HUB_LOG_FILE_LEVEL",
        Some("DOWNLOADER_HUB_LOG_LEVEL"),
    );
    let writer_fn = build_file_writer(options.log_file);

    match options.console_format {
        LogFormat::Json => {
            let stderr_layer = fmt::layer()
                .json()
                .with_ansi(false)
                .with_writer(std::io::stderr as StderrWriter);

            match options.file_format {
                LogFormat::Json => finish_init(
                    stderr_layer,
                    fmt::layer()
                        .json()
                        .with_ansi(false)
                        .fmt_fields(JsonFileFields)
                        .with_writer(writer_fn),
                    console_env_filter,
                    file_env_filter,
                ),
                LogFormat::Plain | LogFormat::Pretty => finish_init(
                    stderr_layer,
                    fmt::layer()
                        .with_ansi(false)
                        .fmt_fields(FileFields)
                        .with_writer(writer_fn),
                    console_env_filter,
                    file_env_filter,
                ),
            }
        }
        LogFormat::Plain => {
            let stderr_layer = fmt::layer()
                .with_ansi(false)
                .with_writer(std::io::stderr as StderrWriter);

            match options.file_format {
                LogFormat::Json => finish_init(
                    stderr_layer,
                    fmt::layer()
                        .json()
                        .with_ansi(false)
                        .fmt_fields(JsonFileFields)
                        .with_writer(writer_fn),
                    console_env_filter,
                    file_env_filter,
                ),
                LogFormat::Plain | LogFormat::Pretty => finish_init(
                    stderr_layer,
                    fmt::layer()
                        .with_ansi(false)
                        .fmt_fields(FileFields)
                        .with_writer(writer_fn),
                    console_env_filter,
                    file_env_filter,
                ),
            }
        }
        LogFormat::Pretty => {
            let stderr_layer = fmt::layer()
                .with_ansi(IsTerminal::is_terminal(&stderr()))
                .with_writer(std::io::stderr as StderrWriter);

            match options.file_format {
                LogFormat::Json => finish_init(
                    stderr_layer,
                    fmt::layer()
                        .json()
                        .with_ansi(false)
                        .fmt_fields(JsonFileFields)
                        .with_writer(writer_fn),
                    console_env_filter,
                    file_env_filter,
                ),
                LogFormat::Plain | LogFormat::Pretty => finish_init(
                    stderr_layer,
                    fmt::layer()
                        .with_ansi(false)
                        .fmt_fields(FileFields)
                        .with_writer(writer_fn),
                    console_env_filter,
                    file_env_filter,
                ),
            }
        }
    }
}

fn finish_init<S, F>(
    stderr_layer: S,
    file_layer: F,
    console_filter: EnvFilter,
    file_filter: EnvFilter,
) where
    S: Layer<Registry> + Send + Sync + 'static,
    F: Layer<
            tracing_subscriber::layer::Layered<
                tracing_subscriber::filter::Filtered<
                    S,
                    tracing_subscriber::reload::Layer<EnvFilter, Registry>,
                    Registry,
                >,
                Registry,
            >,
        > + Send
        + Sync
        + 'static,
{
    let (console_filter, console_handle) = tracing_subscriber::reload::Layer::new(console_filter);
    let stderr_filtered = stderr_layer.with_filter(console_filter);

    let (file_filter, file_handle) = tracing_subscriber::reload::Layer::new(file_filter);
    let file_filtered = file_layer.with_filter(file_filter);

    let console_modify = console_handle.clone();
    let file_modify = file_handle.clone();

    assert!(
        store_reload_handle(
            &CONSOLE_RELOAD_HANDLE,
            Box::new(move |filter| {
                console_modify
                    .modify(|current| *current = filter)
                    .map_err(|e| format!("Failed to set log level: {e:?}"))
            }),
            Box::new(move || {
                console_handle
                    .clone_current()
                    .ok_or_else(|| "Failed to clone current log level".to_string())
            }),
        ),
        "Logger was already initialized"
    );
    assert!(
        store_reload_handle(
            &FILE_RELOAD_HANDLE,
            Box::new(move |filter| {
                file_modify
                    .modify(|current| *current = filter)
                    .map_err(|e| format!("Failed to set log level: {e:?}"))
            }),
            Box::new(move || {
                file_handle
                    .clone_current()
                    .ok_or_else(|| "Failed to clone current log level".to_string())
            }),
        ),
        "Logger was already initialized"
    );

    tracing_subscriber::registry()
        .with(stderr_filtered)
        .with(file_filtered)
        .try_init()
        .expect("setting default subscriber failed");
}

fn store_reload_handle(
    cell: &OnceLock<ReloadBridge>,
    apply: Box<dyn Fn(EnvFilter) -> Result<(), String> + Send + Sync>,
    current: Box<dyn Fn() -> Result<EnvFilter, String> + Send + Sync>,
) -> bool {
    cell.set(ReloadBridge { apply, current }).is_ok()
}

fn build_levels(level: Option<Level>) -> Vec<(&'static str, Level)> {
    level.map_or_else(
        || COMPONENT_LEVELS.to_vec(),
        |level| {
            COMPONENT_LEVELS
                .iter()
                .map(|(k, _v)| (k.to_owned(), level))
                .collect::<Vec<_>>()
        },
    )
}

fn build_env_filter<T>(
    levels: T,
    base_level: Option<Level>,
    env_var: &str,
    fallback_env_var: Option<&str>,
) -> EnvFilter
where
    T: IntoIterator<Item = (&'static str, Level)>,
{
    let default_levels = levels
        .into_iter()
        .map(|(k, v)| {
            if k.is_empty() {
                v.to_string()
            } else {
                format!("{}={}", k, v)
            }
        })
        .collect::<Vec<_>>()
        .join(",");

    let default_directive = base_level.unwrap_or(Level::INFO).into();

    let mut base_filter = EnvFilter::builder()
        .with_default_directive(default_directive)
        .parse_lossy(default_levels);

    let env_value = env::var(env_var).unwrap_or_default();
    let env_value = if env_value.is_empty() {
        fallback_env_var.map_or_else(String::new, |var| env::var(var).unwrap_or_default())
    } else {
        env_value
    };

    let env_directives = env_value
        .split(',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| match s.parse() {
            Ok(d) => Some(d),
            Err(e) => {
                eprintln!("Failed to parse log level directive {s:?}: {e:?}");
                None
            }
        })
        .collect::<Vec<Directive>>();

    for d in env_directives {
        base_filter = base_filter.add_directive(d);
    }

    base_filter
}

fn build_file_writer(log_file: Option<PathBuf>) -> FileWriterFn {
    let file_writer = log_file.map_or_else(
        || MaybeFileWriter::new(None),
        |path| {
            let file_parent_dir = path.parent().unwrap_or_else(|| Path::new("."));

            std::fs::create_dir_all(file_parent_dir).unwrap_or_else(|e| {
                eprintln!("Failed to create log file parent directory {file_parent_dir:?}: {e}");
                std::process::exit(1);
            });

            let file = OpenOptions::new()
                .write(true)
                .truncate(true)
                .create(true)
                .open(&path)
                .unwrap_or_else(|e| {
                    eprintln!("Failed to open log file {}: {e}", path.display());
                    std::process::exit(1);
                });

            let (non_blocking, guard) = tracing_appender::non_blocking(file);

            FILE_GUARD
                .set(guard)
                .expect("File logger guard was already set");

            MaybeFileWriter::new(Some(non_blocking))
        },
    );

    Box::new(move || file_writer.clone())
}

pub fn set_log_level(log_level: &str) -> Result<(), Box<dyn std::error::Error>> {
    set_filter(
        CONSOLE_RELOAD_HANDLE
            .get()
            .expect("Logger was not initialized"),
        log_level,
    )
}

pub fn set_file_log_level(log_level: &str) -> Result<(), Box<dyn std::error::Error>> {
    set_filter(
        FILE_RELOAD_HANDLE
            .get()
            .expect("Logger was not initialized"),
        log_level,
    )
}

fn set_filter(handle: &ReloadBridge, log_level: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut base_filter = EnvFilter::builder()
        .with_default_directive(Level::INFO.into())
        .parse_lossy("info");

    let set_directives = log_level
        .split(',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| match s.parse() {
            Ok(d) => Some(d),
            Err(err) => {
                warn!(directive = ?s, ?err, "Failed to parse log level directive");
                None
            }
        })
        .collect::<Vec<Directive>>();

    for d in set_directives {
        base_filter = base_filter.add_directive(d);
    }

    handle
        .set_filter(base_filter)
        .map_err(std::convert::Into::into)
}

pub fn update_log_level(log_level: &str) -> Result<(), Box<dyn std::error::Error>> {
    update_filter(
        CONSOLE_RELOAD_HANDLE
            .get()
            .expect("Logger was not initialized"),
        log_level,
    )
}

pub fn update_file_log_level(log_level: &str) -> Result<(), Box<dyn std::error::Error>> {
    update_filter(
        FILE_RELOAD_HANDLE
            .get()
            .expect("Logger was not initialized"),
        log_level,
    )
}

fn update_filter(handle: &ReloadBridge, log_level: &str) -> Result<(), Box<dyn std::error::Error>> {
    debug!(?log_level, "Updating log level");

    let default_levels = COMPONENT_LEVELS.iter().map(|(k, v)| {
        if k.is_empty() {
            v.to_string()
        } else {
            format!("{}={}", k, v)
        }
    });

    let current_levels = handle.current_filter()?.to_string();

    trace!(?current_levels, "Current log levels");

    let current_levels = current_levels
        .split(',')
        .map(std::string::ToString::to_string);

    let new_levels = log_level.split(',').map(std::string::ToString::to_string);

    let levels = default_levels
        .chain(current_levels)
        .chain(new_levels)
        .collect::<Vec<_>>()
        .join(",");

    trace!(?levels, "New log levels");

    set_filter(handle, &levels)
}
