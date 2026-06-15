use app_database::entity::authed::AuthedForRole;
use app_peer_comms::{
    jwt::targeted::TargetedJwtPair,
    message::v1::{
        bot::BotMessage,
        central::{
            CentralMessage,
            create_result::{CreateResult, CreateResultData},
        },
    },
};
use tracing::error;

pub(super) use super::{RpcError, RpcReturn};
use crate::cmd::central::{
    auth::{ValidAuth, authentication::Authenticatable},
    broadcaster::{BroadcastAudience, Broadcaster},
    config::CentralConfig,
};

#[allow(clippy::too_many_lines)]
pub async fn handle_bot_rpc(
    msg: BotMessage,
    auth: ValidAuth,
    audiences: Vec<BroadcastAudience>,
) -> RpcReturn {
    match msg {
        BotMessage::Authorize(authorization) => {
            let jwt_config = match authorization
                .as_targeted_jwt_config(AuthedForRole::Bot)
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
                            reason: e.to_string(),
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

        BotMessage::WorkRequestMake {
            info,
            metadata,
            idempotency_key,
        } => {
            let resp = app_database::Database::global()
                .requests_add(auth.authed_id.clone(), info, metadata, idempotency_key)
                .await;

            let resp = match resp {
                Ok(resp) => resp,
                Err(e) => {
                    error!(?e, "Failed to create work request");
                    return Err(RpcError::Database(e));
                }
            };

            let resp =
                CentralMessage::WorkRequestCreateResponse(CreateResult::Ok(CreateResultData {
                    id: resp.id,
                }));

            _ = Broadcaster::send_to_audiences(resp.clone(), audiences);

            Ok(Some(resp.into()))
        }

        BotMessage::WorkRequestGetMineInProgress => {
            let resp = app_database::Database::global()
                .requests_get_mine_in_progress(auth.authed_id.clone())
                .await;

            let resp = match resp {
                Ok(resp) => resp,
                Err(e) => {
                    error!(?e, "Failed to take work request");
                    return Err(e.into());
                }
            };

            let resp: app_peer_comms::message::v1::central::CentralMessage =
                match CentralMessage::work_requests(resp.iter()) {
                    Ok(x) => x,
                    Err(e) => {
                        error!(?e, "Failed to take work request");
                        return Err(("Failed to take work request", e).into());
                    }
                };

            _ = Broadcaster::send_to_audiences(resp.clone(), audiences);

            Ok(Some(resp.into()))
        }

        BotMessage::WorkRequestAddErrors { request_id, errors } => {
            let resp = app_database::Database::global()
                .requests_add_errors(request_id.clone(), auth.authed_id.clone(), errors)
                .await;

            let resp = match resp {
                Ok(resp) => resp,
                Err(e) => {
                    error!(?e, "Failed to add errors to work request");
                    return Err(e.into());
                }
            };

            let resp: app_peer_comms::message::v1::central::CentralMessage =
                (request_id, resp).into();

            _ = Broadcaster::send_to_audiences(resp.clone(), audiences);

            Ok(Some(resp.into()))
        }

        BotMessage::WorkRequestComplete { request_id } => {
            let resp = app_database::Database::global()
                .requests_finish(request_id.clone(), auth.authed_id.clone())
                .await;

            let resp = match resp {
                Ok(x) => x,
                Err(e) => {
                    error!(?e, "Failed to complete work request");
                    return Err(e.into());
                }
            };

            let resp: app_peer_comms::message::v1::central::CentralMessage = resp.into();

            _ = Broadcaster::send_to_audiences(resp.clone(), audiences);

            Ok(Some(resp.into()))
        }
    }
}
