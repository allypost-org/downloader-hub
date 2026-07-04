use std::{sync::OnceLock, time::Duration};

use app_config::common::PeerCommsWorkerTicketFromApiConfig;
use app_peer_comms::{
    IrohEndpointAddr,
    message::v1::{
        central::{
            get_work_item_result::GetWorkItemResult,
            work_request::{WorkRequest, WorkRequestInfo},
        },
        common::file::FileReference,
    },
    rpc::request::{Capabilities, HandlerEntry},
};
use tracing::{debug, error, info, instrument};

use crate::cmd::CmdResult;

pub(super) mod broadcaster;
pub(super) mod helpers;
pub(super) mod process;

static HEARTBEAT: OnceLock<()> = OnceLock::new();

#[instrument(name = "worker", skip_all)]
pub async fn run(
    config: PeerCommsWorkerTicketFromApiConfig,
    central_addr: IrohEndpointAddr,
) -> CmdResult {
    let capabilities = Capabilities::Worker {
        extractors: app_actions::extractors::AVAILABLE_EXTRACTORS
            .iter()
            .filter(|e| e.is_enabled())
            .map(|e| HandlerEntry {
                name: e.name().to_string(),
                description: e.description().to_string(),
            })
            .collect(),
        downloaders: app_actions::downloaders::AVAILABLE_DOWNLOADERS
            .iter()
            .map(|d| HandlerEntry {
                name: d.name().to_string(),
                description: d.description().to_string(),
            })
            .collect(),
        fixers: app_actions::fixers::AVAILABLE_FIXERS
            .iter()
            .map(|f| HandlerEntry {
                name: f.name().to_string(),
                description: f.description().to_string(),
            })
            .collect(),
    };

    crate::cmd::work::rpc::RpcClient::init(config.key.clone(), central_addr, capabilities).await?;

    broadcaster::Broadcaster::init();

    HEARTBEAT.get_or_init(|| {
        tokio::spawn(async {
            loop {
                let jitter = rand::random_range(0..5_000u64);
                tokio::time::sleep(Duration::from_millis(30_000 + jitter)).await;
                if let Err(e) = crate::cmd::work::rpc::RpcClient::heartbeat().await {
                    debug!(?e, "heartbeat failed");
                }
            }
        });
    });

    info!("Connected to central (irpc); waiting for work via getWorkItem");

    loop {
        let work_request = match crate::cmd::work::rpc::RpcClient::get_work_item().await {
            Ok(GetWorkItemResult::Ok(item)) => *item,
            Ok(GetWorkItemResult::BackendError) => {
                error!("central reported a backend error on getWorkItem");
                return Err("central backend error".into());
            }
            Ok(GetWorkItemResult::Unauthorized) => {
                error!(
                    "this worker is not authorized to receive work items; the API key is likely \
                     revoked or expired. Terminating."
                );
                std::process::exit(1);
            }
            Err(e) => {
                error!(?e, "getWorkItem failed");
                return Err(e.into());
            }
        };

        if can_process(&work_request).await {
            debug!(id = %work_request.request_id(), "Processing work item");
            process::process_work_request(work_request).await;
        } else {
            debug!(id = %work_request.request_id(), "Cannot process work item; refusing");
            if let Err(e) =
                crate::cmd::work::rpc::RpcClient::refuse_work_item(work_request.request_id()).await
            {
                error!(?e, "refuse_work_item failed");
            }
        }
    }
}

async fn can_process(work_request: &WorkRequest) -> bool {
    match &work_request.info {
        WorkRequestInfo::DownloadAndFix(file_reference) => {
            can_process_download_and_fix(file_reference).await
        }
    }
}

async fn can_process_download_and_fix(file_reference: &FileReference) -> bool {
    match file_reference {
        FileReference::BlobTicket(_) => true,
        FileReference::Url(url) => {
            debug!("Checking if worker can process URL item");
            let Ok(req) = helpers::extract_info_request::file_url_to_extract_info_request(url)
            else {
                return false;
            };
            req.first_available_extractor().await.is_some()
        }
    }
}
