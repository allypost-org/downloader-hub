use app_database::entity::authed::AuthedForRole;
use app_peer_comms::{
    EndpointId, GossipEvent, PeeringEndpoint,
    jwt::{
        JwtPair,
        targeted::{TargetedJwtClaims, TargetedJwtPair},
    },
    message::{
        Message, SignedMessage,
        v1::{V1Message, bot::BotMessage, central::CentralMessage, worker::WorkerMessage},
    },
};
use futures::StreamExt;
use tracing::{debug, error, info, instrument, trace, warn};

use crate::cmd::central::{
    auth::{ValidAuth, authentication::Authenticatable},
    broadcaster::{BroadcastAudience, Broadcaster},
    components::_ipc::IpcMessage,
    config::CentralConfig,
    rpc_handler::handle_rpc,
};

pub async fn run() -> super::ComponentResult {
    let pe = PeeringEndpoint::global();

    {
        let node_id = pe.endpoint_id().await;
        trace!(
            id = ?node_id,
            "Initialized peering endpoint"
        );
    }

    let topic = pe.gossip_subscribe().await?;

    trace!(topic = ?pe.topic_id, "Initialized gossip topic subscriber");

    info!("Component ready");

    IpcMessage::PeersReady.send()?;

    let (_send, mut recv) = topic.split();

    let mut js = tokio::task::JoinSet::new();

    js.spawn(async move {
        while let Some(event) = recv.next().await {
            let event = match event {
                Ok(event) => event,
                Err(e) => {
                    warn!(?e, "Error receiving gossip event");
                    continue;
                }
            };

            match event {
                GossipEvent::NeighborDown(key) => {
                    debug!(?key, "Neighbour down");
                }
                GossipEvent::NeighborUp(key) => {
                    trace!(?key, "Got new neighbour");
                }
                GossipEvent::Lagged => {
                    warn!("Lagged and missed some messages");
                }
                GossipEvent::Received(msg) => {
                    trace!(from = ?msg.delivered_from, "Received message");

                    let (sender_id, message, jwt_pair) =
                        match SignedMessage::verify_and_decode(&msg.content) {
                            Ok(x) => x,
                            Err(e) => {
                                warn!(?e, "Failed to verify and decode message");
                                continue;
                            }
                        };

                    handle_message(sender_id, message, jwt_pair).await;
                }
            }
        }
    });

    js.join_all().await;

    Ok(())
}

#[instrument(name = "peer-message", skip_all, fields(sender = %sender_id.fmt_short()))]
#[allow(clippy::too_many_lines)]
async fn handle_message(sender_id: EndpointId, message: Message, jwt_pair: Option<JwtPair>) {
    trace!(?message, "Handling message");

    let mut audiences = vec![BroadcastAudience::Endpoint(sender_id)];

    if let Some(jwt_pair) = jwt_pair {
        let auth = match TargetedJwtClaims::parse(
            None,
            &jwt_pair.token,
            CentralConfig::jwt_secret().as_bytes(),
        ) {
            Ok(x) => ValidAuth {
                authed_id: x.id,
                expires_at: x.expires_at,
            },
            Err(e) => {
                warn!(?e, "Failed to parse JWT");
                return;
            }
        };

        audiences.push(BroadcastAudience::Authed(auth.authed_id.clone()));

        match handle_rpc(message, auth, audiences).await {
            Ok(msg) => {
                trace!(?msg, "RPC handled successfully");
            }
            Err(e) => {
                warn!(?e, "Failed to handle RPC");
            }
        }

        return;
    }

    match message {
        Message::V1(v1) => match v1 {
            V1Message::Worker(msg) => match msg {
                WorkerMessage::Authorize(authorization) => {
                    let jwt_config = match authorization
                        .as_targeted_jwt_config(AuthedForRole::Worker)
                        .await
                    {
                        Ok(jwt_config) => jwt_config,
                        Err(e) => {
                            error!(?e, "Failed to generate JWT config");
                            _ = Broadcaster::send_to_audiences(
                                CentralMessage::RejectAuthentication { reason: e },
                                audiences,
                            );
                            return;
                        }
                    };

                    let jwt_pair = TargetedJwtPair::generate(
                        &jwt_config,
                        CentralConfig::jwt_secret().as_bytes(),
                    );

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
                            return;
                        }
                    };

                    let resp = CentralMessage::AcceptAuthentication(jwt_pair.into_pair());

                    _ = Broadcaster::send_to_audiences(resp, audiences);
                }
                msg => {
                    trace!(?msg, "Ignoring unauthenticated worker message");
                }
            },
            V1Message::Bot(msg) => match msg {
                BotMessage::Authorize(authorization) => {
                    let jwt_config = match authorization
                        .as_targeted_jwt_config(AuthedForRole::Bot)
                        .await
                    {
                        Ok(jwt_config) => jwt_config,
                        Err(e) => {
                            error!(?e, "Failed to generate JWT config");
                            _ = Broadcaster::send_to_audiences(
                                CentralMessage::RejectAuthentication { reason: e },
                                audiences,
                            );
                            return;
                        }
                    };

                    let jwt_pair = TargetedJwtPair::generate(
                        &jwt_config,
                        CentralConfig::jwt_secret().as_bytes(),
                    );

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
                            return;
                        }
                    };

                    let resp = CentralMessage::AcceptAuthentication(jwt_pair.into_pair());

                    _ = Broadcaster::send_to_audiences(resp, audiences);
                }
                msg => {
                    trace!(?msg, "Ignoring unauthenticated bot message");
                }
            },
            V1Message::Central(msg) => {
                trace!(?msg, "Ignoring unauthenticated central message");
            }
        },
    }
}
