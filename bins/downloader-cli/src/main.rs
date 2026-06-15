use std::{collections::HashSet, fmt::Debug, io::stderr, path::PathBuf, result::Result};

use app_actions::{
    actions::{
        Action, ActionRequest,
        handlers::{file_rename_to_id::RenameToId, split_scenes::SplitScenes},
    },
    download_file, fix_file,
};
use futures::{StreamExt, stream::FuturesUnordered};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{filter::LevelFilter, util::SubscriberInitExt};

use crate::config::Config;

mod config;

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() {
    init_log();

    let config = Config::init_parsed().expect("Failed to init config");

    debug!(config = ?*config, "Running with config");

    let urls = get_explicit_urls();
    let mut urls = print_errors("urls", urls);

    let files = get_explicit_files();
    let mut files = print_errors("files", files);

    let cli_config = &config.run;

    for x in &cli_config.entries_group.urls_or_files {
        let mut errs = vec![];

        match parse_url(x) {
            Ok(url) => {
                urls.push(url);
                continue;
            }
            Err(e) => errs.push(e),
        }

        match parse_file(x) {
            Ok(file) => {
                files.push(file);
                continue;
            }
            Err(e) => errs.push(e),
        }

        warn!("Failed to parse {x:?} as URL or file: {errs:?}");
    }

    debug!(urls = ?urls, files = ?files, "Parsed urls and files");

    info!("Outputting to {:?}", cli_config.output_directory);

    info!("Starting download");
    let downloaded_urls = urls
        .into_iter()
        .map(|url| async move {
            let url_str = url.to_string();
            download_file(url, &cli_config.output_directory)
                .await
                .into_iter()
                .map(|x| x.map_err(|e| (url_str.clone(), e)))
                .collect::<Vec<_>>()
        })
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    debug!(urls = ?downloaded_urls, "Downloaded urls");

    let (downloaded, failed_downloaded) = split_vec_err(downloaded_urls);
    info!(
        "Download completed: downloaded {} files, failed to download {} files",
        downloaded.len(),
        failed_downloaded.len()
    );

    let to_fix = downloaded
        .into_iter()
        .map(|x| x.path)
        .chain(files.clone())
        .collect::<Vec<_>>();

    debug!(files = ?to_fix, "Files to fix");
    info!("Starting fixing of {} files", to_fix.len());
    let fixed_files = to_fix
        .into_iter()
        .map(|x| async move {
            fix_file(&x)
                .await
                .map(|n| (x.clone(), n))
                .map_err(|e| (x, e))
        })
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>()
        .await;

    let (fixed, failed_fixed) = split_vec_err(fixed_files);
    info!(
        "Fixing completed: fixed {} files, failed to fix {} files",
        fixed.len(),
        failed_fixed.len()
    );

    if cli_config.and_rename {
        let files_set = {
            let mut new = HashSet::new();
            new.extend(files);
            new
        };

        for (old, new) in &fixed {
            if files_set.contains(old) {
                let req = match ActionRequest::in_same_dir(new.file_path.clone()) {
                    Some(x) => x,
                    None => {
                        error!("Failed to get request for {new:?}");
                        continue;
                    }
                };

                if let Err(e) = RenameToId.run(&req).await {
                    error!("Failed to rename {new:?}: {e:?}");
                }
            }
        }
    }

    let split_files = get_explicit_split_files()
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    info!("Starting split of {} files", split_files.len());
    let mut failed_split = vec![];
    for f in split_files {
        let req = ActionRequest::new(f.clone(), cli_config.output_directory.clone());

        if let Err(e) = SplitScenes.run(&req).await {
            error!("Failed to split {f:?}: {e}");
            failed_split.push((f.clone(), e));
        }
    }

    let (partially_failed_downloaded, failed_downloaded): (Vec<_>, Vec<_>) = failed_downloaded
        .into_iter()
        .partition(|(_url, x)| x.is_soft_error());

    for (x, e) in partially_failed_downloaded {
        warn!("Failed to download {x:?} (marked as soft error): {e}");
    }

    if !failed_downloaded.is_empty() || !failed_fixed.is_empty() || !failed_split.is_empty() {
        for (x, e) in failed_downloaded {
            error!("Failed to download {x:?}: {e}");
        }

        for (x, e) in failed_fixed {
            error!("Failed to fix {x:?}: {e}");
        }

        for (x, e) in failed_split {
            error!("Failed to split {x:?}: {e}");
        }

        std::process::exit(1);
    }
}

fn split_vec_err<T: Debug, E: Debug>(v: Vec<Result<T, E>>) -> (Vec<T>, Vec<E>) {
    v.into_iter()
        .fold((vec![], vec![]), |(mut acc_ok, mut acc_err), x| {
            match x {
                Ok(x) => {
                    acc_ok.push(x);
                }
                Err(e) => {
                    acc_err.push(e);
                }
            }

            (acc_ok, acc_err)
        })
}

fn print_errors<T: Sized>(name: &str, maybe_errors: Vec<Result<T, String>>) -> Vec<T> {
    let errors = maybe_errors
        .iter()
        .filter_map(|maybe_err| maybe_err.as_ref().err())
        .collect::<Vec<_>>();

    if !errors.is_empty() {
        let err = format!("Errors parsing {}", name);
        error!(err);
        for error in errors {
            error!("{error}");
        }
        std::process::exit(1);
    }

    maybe_errors
        .into_iter()
        .filter_map(|maybe_err| maybe_err.map_or_else(|_| None, |x| Some(x)))
        .collect()
}

fn get_explicit_urls() -> Vec<Result<url::Url, String>> {
    Config::global()
        .run
        .entries_group
        .urls
        .iter()
        .map(|x| (x, parse_url(x)))
        .map(|(u, maybe_err)| match maybe_err {
            Ok(x) => Ok(x),
            Err(err) => Err(format!("Failed to parse {u:?} as URL: {err}")),
        })
        .collect::<Vec<_>>()
}

fn parse_url(u: &str) -> Result<url::Url, String> {
    url::Url::parse(u).map_err(|x| x.to_string())
}

fn get_explicit_split_files() -> Vec<Result<PathBuf, String>> {
    Config::run()
        .entries_group
        .split_files
        .iter()
        .map(|x| (x, parse_file(x)))
        .map(|(f, maybe_err)| match maybe_err {
            Ok(x) => Ok(x),
            Err(err) => Err(format!("Failed to parse {f:?} as path: {err}")),
        })
        .collect::<Vec<_>>()
}

fn get_explicit_files() -> Vec<Result<PathBuf, String>> {
    Config::run()
        .entries_group
        .files
        .iter()
        .map(|x| (x, parse_file(x)))
        .map(|(f, maybe_err)| match maybe_err {
            Ok(x) => Ok(x),
            Err(err) => Err(format!("Failed to parse {f:?} as path: {err}")),
        })
        .collect::<Vec<_>>()
}

fn parse_file<T: Into<PathBuf>>(f: T) -> Result<PathBuf, String> {
    let f = f.into();

    if !f.exists() {
        return Err("File does not exist".to_string());
    }

    if !f.is_file() {
        return Err("Is not a file".to_string());
    }

    Ok(f)
}

fn init_log() {
    tracing_subscriber::fmt()
        .with_ansi(true)
        .with_env_filter(
            tracing_subscriber::filter::Builder::default()
                .with_default_directive(LevelFilter::INFO.into())
                .with_env_var("DOWNLOADER_HUB_LOG_LEVEL")
                .from_env_lossy(),
        )
        .with_writer(stderr)
        .finish()
        .init();
}
