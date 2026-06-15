use app_tasks::TaskRunner;
use tracing::{debug, error};

use crate::{
    config::Config,
    db::AppDb,
    queue::{TaskQueue, processor::TaskQueueProcessor},
};

mod config;
mod db;
mod queue;
mod server;
mod service;

#[tokio::main]
async fn main() {
    let loaded_dotenv = dotenvy::dotenv();

    Config::init_parsed().expect("Failed to initialize config");

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

    debug!(config = ?*Config::global(), "Running with config");

    AppDb::init().await.expect("Failed to initialize database");

    TaskQueue::init().await.expect("Failed to initialize queue");

    tokio::task::spawn(TaskQueueProcessor::run());
    tokio::task::spawn(TaskRunner::run());

    server::run().await.expect("Failed to run server");
}
