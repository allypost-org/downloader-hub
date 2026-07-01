use app_database::{Database, entity::authed::AuthedForRole};
use app_peer_comms::{
    jwt::targeted::TargetedJwtPair,
    message::v1::{central::CentralMessage, worker::WorkerMessage},
};
use tracing::{debug, error, trace};

pub(super) use super::RpcReturn;
use crate::cmd::central::{
    auth::{ValidAuth, authentication::Authenticatable},
    broadcaster::{BroadcastAudience, Broadcaster},
    config::CentralConfig,
};

#[allow(clippy::too_many_lines)]
pub async fn handle_worker_rpc(
    msg: WorkerMessage,
    auth: ValidAuth,
    mut audiences: Vec<BroadcastAudience>,
) -> RpcReturn {
    match msg {
        WorkerMessage::Authorize(authorization) => {
            let jwt_config = match authorization
                .as_targeted_jwt_config(AuthedForRole::Worker)
                .await
            {
                Ok(jwt_config) => jwt_config,
                Err(e) => {
                    error!(?e, "Failed to generate JWT config");
                    _ = Broadcaster::send_to_audiences(
                        CentralMessage::RejectAuthentication { reason: e.clone() },
                        audiences,
                    );
                    return Ok(Some(
                        CentralMessage::RejectAuthentication { reason: e }.into(),
                    ));
                }
            };

            let jwt_pair =
                TargetedJwtPair::generate(&jwt_config, CentralConfig::jwt_secret().as_bytes());

            let jwt_pair = match jwt_pair {
                Ok(jwt_pair) => jwt_pair,
                Err(e) => {
                    error!(?e, "Failed to generate JWT pair");
                    _ = Broadcaster::send_to_audiences(
                        CentralMessage::RejectAuthentication {
                            reason: format!("Failed to generate JWT pair: {}", e),
                        },
                        audiences,
                    );
                    return Ok(Some(
                        CentralMessage::RejectAuthentication {
                            reason: format!("Failed to generate JWT pair: {}", e),
                        }
                        .into(),
                    ));
                }
            };

            let resp = CentralMessage::AcceptAuthentication(jwt_pair.into_pair());

            _ = Broadcaster::send_to_audiences(resp.clone(), audiences);

            Ok(Some(resp.into()))
        }

        WorkerMessage::WorkRequestTake { request_id } => {
            debug!(?request_id, "Recieved work request - take");

            let resp = Database::global()
                .requests_take(request_id.clone(), auth.authed_id.clone())
                .await;

            let resp = match resp {
                Ok(resp) => resp,
                Err(e) => {
                    error!(?e, "Failed to take work request");
                    return Err(("Failed to take work request", e).into());
                }
            };

            let resp: app_peer_comms::message::v1::central::CentralMessage =
                match (request_id, resp).try_into() {
                    Ok(x) => x,
                    Err(e) => {
                        error!(?e, "Failed to take work request");
                        return Err(("Failed to take work request", e).into());
                    }
                };

            debug!(?resp, "Work request taken");

            _ = Broadcaster::send_to_audiences(resp.clone(), audiences);

            Ok(Some(resp.into()))
        }

        WorkerMessage::WorkRequestFree { request_id } => {
            debug!(?request_id, "Work request freed");

            let resp = Database::global()
                .requests_free(request_id.clone(), auth.authed_id.clone())
                .await;

            let resp = match resp {
                Ok(resp) => resp,
                Err(e) => {
                    error!(?e, "Failed to free work request");
                    return Err(("Failed to free work request", e).into());
                }
            };

            debug!(?resp, "Work request freed");

            let resp: app_peer_comms::message::v1::central::CentralMessage =
                (request_id, resp).into();

            _ = Broadcaster::send_to_audiences(resp.clone(), audiences);

            Ok(Some(resp.into()))
        }

        WorkerMessage::WorkRequestUpdateStatusMessage {
            request_id,
            message,
        } => {
            trace!(?request_id, ?message, "Work request update status message");

            let resp = Database::global();

            let resp = resp
                .requests_update_status_message(
                    request_id.clone(),
                    auth.authed_id.clone(),
                    message.as_ref(),
                )
                .await;

            let resp = match resp {
                Ok(resp) => resp,
                Err(e) => {
                    error!(?e, "Failed to update work request status message");
                    return Err(("Failed to update work request status message", e).into());
                }
            };

            let resp: app_peer_comms::message::v1::central::CentralMessage =
                (request_id.clone(), resp).into();

            trace!(?request_id, "Work request status message updated");

            _ = Broadcaster::send_to_audiences(resp.clone(), audiences);

            Ok(Some(resp.into()))
        }

        WorkerMessage::WorkRequestAddErrors { request_id, errors } => {
            trace!(?request_id, ?errors, "Work request add errors");

            let resp = Database::global()
                .requests_add_errors(request_id.clone(), auth.authed_id.clone(), errors)
                .await;

            let resp = match resp {
                Ok(resp) => resp,
                Err(e) => {
                    error!(?e, "Failed to add errors to work request");
                    return Err(("Failed to add errors to work request", e).into());
                }
            };

            let resp: app_peer_comms::message::v1::central::CentralMessage =
                (request_id.clone(), resp).into();

            trace!(?request_id, "Work request errors added");

            _ = Broadcaster::send_to_audiences(resp.clone(), audiences);

            Ok(Some(resp.into()))
        }

        WorkerMessage::WorkRequestMoveToWaitingForRequester {
            request_id,
            files_data,
        } => {
            trace!(
                ?request_id,
                ?files_data,
                "Work request move to waiting for requester"
            );

            let resp = Database::global()
                .requests_move_to_waiting_for_requester(
                    request_id.clone(),
                    auth.authed_id.clone(),
                    files_data.into_iter().map(Into::into).collect(),
                )
                .await;

            let resp = match resp {
                Ok(resp) => resp,
                Err(e) => {
                    error!(?e, "Failed to move work request to waiting for requester");
                    return Err(("Failed to move work request to waiting for requester", e).into());
                }
            };

            let resp: app_peer_comms::message::v1::central::CentralMessage =
                (request_id.clone(), resp).into();

            trace!(?request_id, "Work request moved to waiting for requester");

            _ = Broadcaster::send_to_audiences(resp.clone(), audiences);

            Ok(Some(resp.into()))
        }

        WorkerMessage::WorkRequestFail { request_id, reason } => {
            trace!(?request_id, ?reason, "Work request fail");

            let resp = Database::global()
                .requests_fail(request_id.clone(), auth.authed_id.clone(), reason.as_ref())
                .await;

            let resp = match resp {
                Ok(resp) => resp,
                Err(e) => {
                    error!(?e, "Failed to fail work request");
                    return Err(("Failed to fail work request", e).into());
                }
            };

            if let app_database::api::requests::FailResult::Ok {
                ref requester_id, ..
            } = resp
            {
                audiences.push(BroadcastAudience::Authed(requester_id.clone()));
            }

            let resp: app_peer_comms::message::v1::central::CentralMessage =
                (request_id.clone(), resp).into();

            trace!(?request_id, "Work request failed");

            _ = Broadcaster::send_to_audiences(resp.clone(), audiences);

            Ok(Some(resp.into()))
        }
    }
}
