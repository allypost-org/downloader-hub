use tracing::{Level, debug, trace};

mod cmd;
mod config;

fn main() {
    app_peer_comms::install_default_crypto_provider()
        .expect("install rustls aws-lc-rs CryptoProvider");

    let loaded_dotenv = dotenvy::dotenv();

    if let Err(e) = &loaded_dotenv
        && !e.not_found()
    {
        panic!("Failed to load dotenv file: {e:?}");
    }

    let config = config::Config::init_parsed().expect("Failed to initialize config");

    let file_log_level = config.log_file_level.as_deref().map(|s| {
        s.parse::<Level>()
            .unwrap_or_else(|e| panic!("Invalid --log-file-level {s:?}: {e}"))
    });

    app_logger::init_with_options(
        app_logger::LogOptions::new()
            .with_log_file(config.log_file.clone())
            .with_console_format(config.log_format)
            .with_file_format(config.log_file_format)
            .with_file_log_level(file_log_level),
    );

    match loaded_dotenv {
        Err(_) => {
            debug!("No dotenv file found");
        }
        Ok(loaded_dotenv) => {
            debug!(path = ?loaded_dotenv, "Loaded dotenv file");
        }
    }

    trace!(config = ?*config, "Running with config");
    debug!(
        app_name = config::Config::app_name_with_version(),
        app_version = config::Config::app_version(),
        built = config::Config::build_date(),
        rustc_version = config::Config::rustc_version(),
        "Build info"
    );

    cmd::run(config.cmd.clone());
}
