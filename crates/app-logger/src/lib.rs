use std::{env, sync::OnceLock};

use tracing::{Level, debug, trace, warn};
use tracing_subscriber::{EnvFilter, filter::Directive, fmt, prelude::*};

#[allow(clippy::type_complexity)]
static RELOAD_HANDLE: OnceLock<
    tracing_subscriber::reload::Handle<
        EnvFilter,
        tracing_subscriber::layer::Layered<
            fmt::Layer<
                tracing_subscriber::Registry,
                fmt::format::DefaultFields,
                fmt::format::Format,
                fn() -> std::io::Stderr,
            >,
            tracing_subscriber::Registry,
        >,
    >,
> = OnceLock::new();

pub const COMPONENT_LEVELS: &[(&str, Level)] = &[
    // Binaries
    // Libraries
    // Other
    // External
];

/// Initialize the logger
///
/// # Panics
/// Panics if the logger fails to initialize
pub fn init() {
    init_with(COMPONENT_LEVELS.to_vec());
}

pub fn init_with_app_level(level: Level) {
    let levels = COMPONENT_LEVELS
        .iter()
        .map(|(k, _v)| (k.to_owned(), level))
        .collect::<Vec<_>>();

    init_with(levels);
}

pub fn init_with<T>(levels: T)
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
        .with_default_directive(Level::WARN.into())
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

    let (base_level, reload_handle) = tracing_subscriber::reload::Layer::new(base_level);
    RELOAD_HANDLE
        .set(reload_handle)
        .expect("Logger was already initialized");

    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr as fn() -> std::io::Stderr))
        .with(base_level)
        .try_init()
        .expect("setting default subscriber failed");
}

pub fn set_log_level(log_level: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut base_level = EnvFilter::builder()
        .with_default_directive(Level::WARN.into())
        .parse_lossy("warn");

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
        .modify(|filter| *filter = base_level)
        .map_err(|e| format!("Failed to set log level: {:?}", e).into())
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
        .clone_current()
        .expect("Failed to clone current log level")
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
