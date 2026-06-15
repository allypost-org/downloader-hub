use tracing::{debug, error, trace};

mod cmd;
mod config;

fn main() {
    let loaded_dotenv = dotenvy::dotenv();

    app_logger::init();

    match loaded_dotenv {
        Ok(loaded_dotenv) => {
            debug!(path = ?loaded_dotenv, "Loaded dotenv file");
        }
        Err(e) if e.not_found() => {
            debug!("No dotenv file found");
        }
        Err(e) => {
            error!("Failed to load dotenv file: {e:?}");
            panic!("Failed to load dotenv file: {e:?}");
        }
    }

    let config = config::Config::init_parsed().expect("Failed to initialize config");

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
