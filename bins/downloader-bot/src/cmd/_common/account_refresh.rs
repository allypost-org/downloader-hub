use std::future::Future;
use std::pin::Pin;

use app_database::entity::accounts::{AccountPlace, AccountUser, Platform};
use app_peer_comms::message::v1::central::{
    complete_account_refresh_result::CompleteAccountRefreshResult,
    get_account_refresh_item_result::GetAccountRefreshItemResult,
    work_request::{WorkRequest, WorkRequestInfo},
};
use tracing::{error, info, warn};

use crate::peering::rpc::RpcClient;

pub type UserFetchFut =
    Pin<Box<dyn Future<Output = Result<AccountUser, String>> + Send>>;
pub type PlaceFetchFut =
    Pin<Box<dyn Future<Output = Result<AccountPlace, String>> + Send>>;

pub async fn run_refresh_loop(
    platform: Platform,
    fetch_user: fn(&str) -> UserFetchFut,
    fetch_place: fn(&str) -> PlaceFetchFut,
) {
    let poll_interval = std::time::Duration::from_secs(5);
    loop {
        match RpcClient::get_account_refresh_item(platform).await {
            Ok(GetAccountRefreshItemResult::Ok(work)) => {
                process_one(*work, fetch_user, fetch_place).await;
            }
            Ok(GetAccountRefreshItemResult::NoWork) => {
                tokio::time::sleep(poll_interval).await;
            }
            Ok(GetAccountRefreshItemResult::BackendError) => {
                warn!("get_account_refresh_item backend error; backing off");
                tokio::time::sleep(poll_interval).await;
            }
            Ok(GetAccountRefreshItemResult::Unauthorized) => {
                error!("bot unauthorized for account refresh; stopping loop");
                return;
            }
            Err(e) => {
                warn!(?e, "get_account_refresh_item failed; backing off");
                tokio::time::sleep(poll_interval).await;
            }
        }
    }
}

async fn process_one(
    work: WorkRequest,
    fetch_user: fn(&str) -> UserFetchFut,
    fetch_place: fn(&str) -> PlaceFetchFut,
) {
    let request_id = work.request_id();
    let WorkRequestInfo::RefreshAccountInfo(payload) = work.info().clone() else {
        warn!(%request_id, "account refresh worker got non-refresh item; freeing");
        let _ = RpcClient::work_request_free(request_id.clone()).await;
        return;
    };

    let mut users = Vec::new();
    let mut places = Vec::new();
    let mut errors = Vec::new();

    for user_ref in &payload.users {
        match fetch_user(&user_ref.id).await {
            Ok(user) => users.push(user),
            Err(e) => errors.push(format!("user {}: {e}", user_ref.id)),
        }
    }

    for place_ref in &payload.places {
        match fetch_place(&place_ref.id).await {
            Ok(place) => places.push(place),
            Err(e) => errors.push(format!("place {}: {e}", place_ref.id)),
        }
    }

    if !errors.is_empty()
        && let Err(e) = RpcClient::work_request_add_errors(request_id.clone(), errors.clone()).await
    {
        warn!(?e, %request_id, "failed to record account refresh errors");
    }

    if users.is_empty() && places.is_empty() {
        let reason = errors
            .first()
            .cloned()
            .unwrap_or_else(|| "no account metadata fetched".to_string());
        if let Err(e) = RpcClient::work_request_fail(request_id.clone(), reason.into()).await {
            warn!(?e, %request_id, "failed to fail account refresh request");
        }
        return;
    }

    if let Err(e) = RpcClient::accounts_upsert(users, places).await {
        error!(?e, %request_id, "accounts_upsert failed during refresh");
        if let Err(fail_err) = RpcClient::work_request_fail(
            request_id.clone(),
            format!("accounts upsert failed: {e}").into(),
        )
        .await
        {
            warn!(?fail_err, %request_id, "failed to fail account refresh after upsert error");
        }
        return;
    }

    match RpcClient::complete_account_refresh(request_id.clone()).await {
        Ok(CompleteAccountRefreshResult::Ok) => {
            info!(%request_id, "account refresh request completed");
        }
        Ok(other) => {
            warn!(?other, %request_id, "complete_account_refresh rejected");
        }
        Err(e) => {
            error!(?e, %request_id, "complete_account_refresh rpc failed");
        }
    }
}
