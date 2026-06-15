use axum::{Json, http::HeaderMap, response::IntoResponse};

use crate::cmd::central::{
    broadcaster::BroadcastAudience,
    components::worker_api::{auth::ValidAuth, request::json_or_accept::JsonOrAccept},
    rpc_handler::handle_rpc,
};

pub async fn post_rpc(
    auth: ValidAuth,
    headers: HeaderMap,
    Json(body): Json<app_peer_comms::Message>,
) -> impl IntoResponse {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    enum Resp {
        Data(Option<app_peer_comms::Message>),
        Error(String),
    }

    let audiences = vec![BroadcastAudience::Authed(auth.authed_id.clone())];

    let resp = match handle_rpc(body, auth, audiences).await {
        Ok(x) => Resp::Data(x),
        Err(e) => Resp::Error(e.to_string()),
    };

    JsonOrAccept(resp, headers)
}
