use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use app_config::common::PeerCommsWorkerTicketFromApiConfig;
use app_peer_comms::{
    AccountPlaceRef, AccountUserRef, IrohEndpointAddr, PeeringEndpoint,
    message::v1::{
        central::{
            get_work_item_result::GetWorkItemResult,
            work_request::{WorkRequest, WorkRequestInfo},
        },
        common::file::FileReference,
    },
    rpc::request::{Capabilities, HandlerEntry, SecretEntry},
};
use arc_swap::ArcSwapOption;
use tracing::{debug, error, info, instrument};

use crate::cmd::{CmdResult, work::rpc};

pub(super) mod broadcaster;
pub(super) mod helpers;
pub(super) mod process;

#[derive(Clone, Default)]
pub struct WorkerLoop {
    pub rpc: Arc<ArcSwapOption<rpc::RpcClient>>,
    secrets: Arc<RwLock<HashMap<String, SecretEntry>>>,
}

impl WorkerLoop {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_rpc(&self, client: rpc::RpcClient) {
        self.rpc.store(Some(Arc::new(client)));
    }

    #[must_use]
    pub fn rpc(&self) -> Option<Arc<rpc::RpcClient>> {
        self.rpc.load_full()
    }

    pub fn set_secrets(&self, secrets: Vec<SecretEntry>) {
        let map = secrets.into_iter().map(|s| (s.name.clone(), s)).collect();
        *self.secrets.write().expect("secrets lock poisoned") = map;
    }

    #[must_use]
    pub fn secret_value_for(
        &self,
        url: &url::Url,
        ordered_by: Option<&AccountUserRef>,
        ordered_in: Option<&AccountPlaceRef>,
    ) -> Option<String> {
        let name = platform_secret_name(url.host_str()?)?;
        let guard = self.secrets.read().ok()?;
        let entry = guard.get(name)?;
        let user_ok = entry.allowed_users.is_empty()
            || ordered_by.is_some_and(|user| entry.allowed_users.contains(user));
        let place_ok = entry.allowed_places.is_empty()
            || ordered_in.is_some_and(|place| entry.allowed_places.contains(place));
        let value = (user_ok && place_ok).then(|| entry.value.clone());
        drop(guard);
        value
    }
}

fn platform_secret_name(host: &str) -> Option<&'static str> {
    match host.to_lowercase().as_str() {
        "instagram.com" | "www.instagram.com" => Some("instagram"),
        _ => None,
    }
}

#[instrument(name = "worker", skip_all)]
pub async fn run(
    actions: Arc<app_actions::Actions>,
    peering: Arc<PeeringEndpoint>,
    loop_state: WorkerLoop,
    config: PeerCommsWorkerTicketFromApiConfig,
    central_addr: IrohEndpointAddr,
) -> CmdResult {
    let capabilities = Capabilities::Worker {
        extractors: actions
            .enabled_extractors()
            .map(|e| HandlerEntry {
                name: e.name().to_string(),
                description: e.description().to_string(),
            })
            .collect(),
        downloaders: actions
            .enabled_downloaders()
            .iter()
            .map(|d| HandlerEntry {
                name: d.name().to_string(),
                description: d.description().to_string(),
            })
            .collect(),
        fixers: actions
            .enabled_fixers()
            .await
            .iter()
            .map(|f| HandlerEntry {
                name: f.name().to_string(),
                description: f.description().to_string(),
            })
            .collect(),
    };

    let rpc = rpc::RpcClient::connect(
        peering.as_ref(),
        config.key.clone(),
        central_addr,
        capabilities,
    )
    .await?;
    loop_state.set_rpc(rpc);

    let broadcaster = broadcaster::Broadcaster::new(loop_state.rpc.clone());

    refresh_dynamic_settings(&loop_state).await;

    info!("Connected to central (irpc); waiting for work via getWorkItem");

    let Some(rpc) = loop_state.rpc() else {
        return Err("RPC client not connected".into());
    };

    loop {
        let work_request = match rpc.get_work_item().await {
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

        if can_process(&actions, &work_request).await {
            debug!(id = %work_request.request_id(), "Processing work item");
            process::process_work_request(
                actions.clone(),
                peering.clone(),
                broadcaster.clone(),
                loop_state.clone(),
                work_request,
            )
            .await;
        } else {
            debug!(id = %work_request.request_id(), "Cannot process work item; refusing");
            if let Err(e) = rpc.refuse_work_item(work_request.request_id()).await {
                error!(?e, "refuse_work_item failed");
            }
        }
    }
}

pub(super) async fn refresh_dynamic_settings(loop_state: &WorkerLoop) {
    let Some(rpc) = loop_state.rpc() else {
        debug!("skipping dynamic settings refresh; RPC client not connected");
        return;
    };
    if rpc.heartbeat().await.is_err() {
        debug!("heartbeat failed");
        return;
    }
    match rpc.get_log_settings().await {
        Ok(app_peer_comms::rpc::request::LogSettingsResult::Ok(settings)) => {
            let settings = app_logger::LogFilterSettings {
                console: settings.console,
                file: settings.file,
            };
            if let Err(e) = app_logger::apply_log_filter_settings(&settings) {
                error!(?e, "Failed to apply dynamic log settings");
            }
        }
        Ok(result) => debug!(?result, "central did not return log settings"),
        Err(e) => debug!(?e, "log settings request failed"),
    }
    match rpc.get_secrets().await {
        Ok(app_peer_comms::rpc::request::SecretsResult::Ok(entries)) => {
            loop_state.set_secrets(entries);
        }
        Ok(result) => debug!(?result, "central did not return secrets"),
        Err(e) => debug!(?e, "secrets request failed"),
    }
}

async fn can_process(actions: &app_actions::Actions, work_request: &WorkRequest) -> bool {
    match &work_request.info {
        WorkRequestInfo::DownloadAndFix(file_reference) => {
            can_process_download_and_fix(actions, file_reference).await
        }
        WorkRequestInfo::RefreshAccountInfo(_) => false,
    }
}

async fn can_process_download_and_fix(
    actions: &app_actions::Actions,
    file_reference: &FileReference,
) -> bool {
    match file_reference {
        FileReference::BlobTicket(_) => true,
        FileReference::Url(url) => {
            debug!("Checking if worker can process URL item");
            let Ok(req) = helpers::extract_info_request::file_url_to_extract_info_request(url)
            else {
                return false;
            };
            actions.first_available_extractor(&req).await.is_some()
        }
    }
}
