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
use tracing_subscriber::{
    EnvFilter, Layer, Registry, filter::Directive, fmt, prelude::*, reload::Handle,
};

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

static RELOAD_HANDLE: OnceLock<ReloadBridge> = OnceLock::new();
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
}

impl Default for LogOptions {
    fn default() -> Self {
        Self {
            log_level: Some(Level::INFO),
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
    let levels = options.log_level.map_or_else(
        || COMPONENT_LEVELS.to_vec(),
        |level| {
            COMPONENT_LEVELS
                .iter()
                .map(|(k, _v)| (k.to_owned(), level))
                .collect::<Vec<_>>()
        },
    );

    init_with(levels, options);
}

pub fn init_with<T>(levels: T, options: LogOptions)
where
    T: IntoIterator<Item = (&'static str, Level)>,
{
    let env_filter = build_env_filter(levels);
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
                    env_filter,
                ),
                LogFormat::Plain | LogFormat::Pretty => finish_init(
                    stderr_layer,
                    fmt::layer()
                        .with_ansi(false)
                        .fmt_fields(FileFields)
                        .with_writer(writer_fn),
                    env_filter,
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
                    env_filter,
                ),
                LogFormat::Plain | LogFormat::Pretty => finish_init(
                    stderr_layer,
                    fmt::layer()
                        .with_ansi(false)
                        .fmt_fields(FileFields)
                        .with_writer(writer_fn),
                    env_filter,
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
                    env_filter,
                ),
                LogFormat::Plain | LogFormat::Pretty => finish_init(
                    stderr_layer,
                    fmt::layer()
                        .with_ansi(false)
                        .fmt_fields(FileFields)
                        .with_writer(writer_fn),
                    env_filter,
                ),
            }
        }
    }
}

fn finish_init<S, F>(stderr_layer: S, file_layer: F, env_filter: EnvFilter)
where
    S: Layer<Registry> + Send + Sync + 'static,
    F: Layer<tracing_subscriber::layer::Layered<S, Registry>> + Send + Sync + 'static,
{
    let (filter_layer, reload_handle) = tracing_subscriber::reload::Layer::new(env_filter);

    assert!(
        store_reload_handle(reload_handle),
        "Logger was already initialized"
    );

    tracing_subscriber::registry()
        .with(stderr_layer)
        .with(file_layer)
        .with(filter_layer)
        .try_init()
        .expect("setting default subscriber failed");
}

fn store_reload_handle<S>(reload_handle: Handle<EnvFilter, S>) -> bool
where
    S: Send + Sync + 'static,
{
    let modify_handle = reload_handle.clone();
    let current_handle = reload_handle;

    RELOAD_HANDLE
        .set(ReloadBridge {
            apply: Box::new(move |filter| {
                modify_handle
                    .modify(|current| *current = filter)
                    .map_err(|e| format!("Failed to set log level: {e:?}"))
            }),
            current: Box::new(move || {
                current_handle
                    .clone_current()
                    .ok_or_else(|| "Failed to clone current log level".to_string())
            }),
        })
        .is_ok()
}

fn build_env_filter<T>(levels: T) -> EnvFilter
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
        .fold(String::new(), |acc, a| format!("{},{}", acc, a));

    let mut base_level = EnvFilter::builder()
        .with_default_directive(Level::INFO.into())
        .parse_lossy(default_levels);

    let env_directives = env::var("DOWNLOADER_HUB_LOG_LEVEL")
        .unwrap_or_default()
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
        base_level = base_level.add_directive(d);
    }

    base_level
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
    let mut base_level = EnvFilter::builder()
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
        base_level = base_level.add_directive(d);
    }

    let reload_handle = RELOAD_HANDLE.get().expect("Logger was not initialized");

    reload_handle
        .set_filter(base_level)
        .map_err(std::convert::Into::into)
}

pub fn update_log_level(log_level: &str) -> Result<(), Box<dyn std::error::Error>> {
    debug!(?log_level, "Updating log level");

    let default_levels = COMPONENT_LEVELS.iter().map(|(k, v)| {
        if k.is_empty() {
            v.to_string()
        } else {
            format!("{}={}", k, v)
        }
    });

    let current_levels = RELOAD_HANDLE
        .get()
        .expect("Logger was not initialized")
        .current_filter()?
        .to_string();

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

    set_log_level(&levels)
}
