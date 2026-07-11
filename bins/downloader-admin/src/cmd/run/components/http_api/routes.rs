use std::sync::Arc;

use app_database::{
    Database,
    api::{
        accounts::OptionalField,
        authed::{AuthedFullInfo, AuthedRemoveResult, AuthedRevokeResult, AuthedRotateTokenResult},
        requests::{
            CancelResult, RemoveResult, RequestStatusType, RequestsByStatusPage, RetryResult,
        },
    },
    entity::authed::AuthedForRole,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use super::{
    AppState,
    auth::{AdminSession, WriteSession, make_claims},
    envelope::V1Response,
};

#[derive(Debug, Deserialize)]
pub struct ListRequestsQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub cursor: Option<Arc<str>>,
}

fn parse_status_type(s: &str) -> Option<RequestStatusType> {
    match s {
        "pending" => Some(RequestStatusType::Pending),
        "inProgress" => Some(RequestStatusType::InProgress),
        "delivering" => Some(RequestStatusType::Delivering),
        "done" => Some(RequestStatusType::Done),
        "failed" => Some(RequestStatusType::Failed),
        _ => None,
    }
}

pub async fn list_counts(_session: AdminSession) -> impl IntoResponse {
    match Database::global().requests_counts().await {
        Ok(counts) => V1Response::ok(counts),
        Err(e) => {
            tracing::error!(?e, "list_counts failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

const REQUESTS_LIMIT_MAX: i64 = 500;

pub async fn list_requests(
    _session: AdminSession,
    _state: State<AppState>,
    Query(q): Query<ListRequestsQuery>,
) -> impl IntoResponse {
    let Some(status) = q.status.as_deref().and_then(parse_status_type) else {
        return V1Response::<RequestsByStatusPage>::err(
            StatusCode::BAD_REQUEST,
            "invalid or missing `status`",
        );
    };
    let limit = q.limit.map(|n| n.clamp(1, REQUESTS_LIMIT_MAX));
    match Database::global()
        .requests_get_by_status(status, limit, q.cursor)
        .await
    {
        Ok(page) => V1Response::ok(page),
        Err(e) => {
            tracing::error!(?e, "list_requests failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn get_request(_session: AdminSession, Path(id): Path<String>) -> impl IntoResponse {
    match Database::global()
        .requests_get(Arc::from(id.as_str()))
        .await
    {
        Ok(req) => V1Response::ok(req),
        Err(e) => {
            tracing::error!(?e, "get_request failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn retry_request(_session: WriteSession, Path(id): Path<String>) -> impl IntoResponse {
    match Database::global()
        .requests_retry(Arc::from(id.as_str()))
        .await
    {
        Ok(RetryResult::Ok) => V1Response::ok(serde_json::json!({ "retried": true })),
        Ok(RetryResult::RequestNotFound) => {
            V1Response::err(StatusCode::NOT_FOUND, "request not found")
        }
        Ok(RetryResult::RequestNotRetryable) => V1Response::err(
            StatusCode::CONFLICT,
            "request is not in a retryable status (failed or done)",
        ),
        Err(e) => {
            tracing::error!(?e, "retry_request failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn cancel_request(session: WriteSession, Path(id): Path<String>) -> impl IntoResponse {
    match Database::global()
        .requests_cancel(Arc::from(id.as_str()), Arc::from(session.admin_id()))
        .await
    {
        Ok(CancelResult::Ok) => V1Response::ok(serde_json::json!({ "cancelled": true })),
        Ok(CancelResult::RequestNotFound) => {
            V1Response::err(StatusCode::NOT_FOUND, "request not found")
        }
        Err(e) => {
            tracing::error!(?e, "cancel_request failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn remove_request(_session: WriteSession, Path(id): Path<String>) -> impl IntoResponse {
    match Database::global()
        .requests_remove(Arc::from(id.as_str()))
        .await
    {
        Ok(RemoveResult::Ok) => V1Response::ok(serde_json::json!({ "removed": true })),
        Ok(RemoveResult::RequestNotFound) => {
            V1Response::err(StatusCode::NOT_FOUND, "request not found")
        }
        Err(e) => {
            tracing::error!(?e, "remove_request failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn clear_refusals(_session: WriteSession, Path(id): Path<String>) -> impl IntoResponse {
    match Database::global()
        .requests_clear_refusals(Arc::from(id.as_str()))
        .await
    {
        Ok(res) => V1Response::ok(res),
        Err(e) => {
            tracing::error!(?e, "clear_refusals failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub id: Arc<str>,
    pub name: Arc<str>,
    pub for_role: String,
    pub readonly: bool,
}

pub async fn login(
    _state: State<AppState>,
    jar: axum_extra::extract::SignedCookieJar,
    Json(body): Json<LoginBody>,
) -> axum::response::Response {
    match Database::global()
        .authed_get_info_by_token(Arc::from(body.token.as_str()))
        .await
    {
        Ok(app_database::api::authed::AuthedInfoResponse::Authorized(info)) => {
            if !matches!(info.for_role, AuthedForRole::Admin) {
                return V1Response::<()>::err(StatusCode::FORBIDDEN, "token is not an admin token")
                    .into_response();
            }
            let claims = make_claims(info.id.as_ref(), info.readonly);
            let jar = jar.add(super::auth::build_session_cookie(&claims));
            let resp = V1Response::ok(MeResponse {
                id: info.id,
                name: info.name,
                for_role: "admin".to_string(),
                readonly: info.readonly,
            });
            (jar, resp).into_response()
        }
        Ok(app_database::api::authed::AuthedInfoResponse::NotAuthorized { error }) => {
            V1Response::<()>::err(StatusCode::UNAUTHORIZED, error).into_response()
        }
        Err(e) => {
            tracing::error!(?e, "login DB lookup failed");
            V1Response::<()>::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
                .into_response()
        }
    }
}

pub async fn logout(jar: axum_extra::extract::SignedCookieJar) -> axum::response::Response {
    let jar = jar.remove(super::auth::session_cookie_name());
    let resp = V1Response::ok(serde_json::json!({ "logged_out": true }));
    (jar, resp).into_response()
}

pub async fn me(session: AdminSession) -> impl IntoResponse {
    let readonly = session.readonly();
    match Database::global()
        .authed_get_info_by_id(Arc::from(session.admin_id()))
        .await
    {
        Ok(app_database::api::authed::AuthedInfoResponse::Authorized(info)) => {
            V1Response::ok(MeResponse {
                id: info.id,
                name: info.name,
                for_role: format!("{}", info.for_role),
                readonly,
            })
        }
        _ => V1Response::err(StatusCode::UNAUTHORIZED, "session invalid"),
    }
}

pub async fn list_authed(_session: AdminSession) -> impl IntoResponse {
    match Database::global().authed_list_full().await {
        Ok(rows) => V1Response::ok(rows),
        Err(e) => {
            tracing::error!(?e, "list_authed failed");
            V1Response::<Arc<[AuthedFullInfo]>>::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error",
            )
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateAuthedBody {
    pub name: String,
    #[serde(rename = "for")]
    pub for_role: AuthedForRole,
    #[serde(default)]
    pub readonly: bool,
    pub only_tagged: Option<Vec<String>>,
    pub expires_at: Option<i64>,
}

pub async fn create_authed(
    _session: WriteSession,
    Json(body): Json<CreateAuthedBody>,
) -> impl IntoResponse {
    match Database::global()
        .authed_create(
            &body.name,
            body.for_role,
            body.readonly,
            body.only_tagged,
            body.expires_at,
        )
        .await
    {
        Ok(info) => V1Response::ok(info),
        Err(e) => {
            tracing::error!(?e, "create_authed failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn revoke_authed(_session: WriteSession, Path(id): Path<String>) -> impl IntoResponse {
    match Database::global()
        .authed_revoke(Arc::from(id.as_str()))
        .await
    {
        Ok(AuthedRevokeResult::Ok) => V1Response::ok(serde_json::json!({ "revoked": true })),
        Ok(AuthedRevokeResult::NotFound) => {
            V1Response::err(StatusCode::NOT_FOUND, "authed not found")
        }
        Err(e) => {
            tracing::error!(?e, "revoke_authed failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn rotate_authed(_session: WriteSession, Path(id): Path<String>) -> impl IntoResponse {
    match Database::global()
        .authed_rotate_token(Arc::from(id.as_str()))
        .await
    {
        Ok(AuthedRotateTokenResult::Ok { token }) => {
            V1Response::ok(serde_json::json!({ "token": token }))
        }
        Ok(AuthedRotateTokenResult::NotFound) => {
            V1Response::err(StatusCode::NOT_FOUND, "authed not found")
        }
        Err(e) => {
            tracing::error!(?e, "rotate_authed failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn remove_authed(_session: WriteSession, Path(id): Path<String>) -> impl IntoResponse {
    match Database::global()
        .authed_remove(Arc::from(id.as_str()))
        .await
    {
        Ok(AuthedRemoveResult::Ok) => V1Response::ok(serde_json::json!({ "removed": true })),
        Ok(AuthedRemoveResult::NotFound) => {
            V1Response::err(StatusCode::NOT_FOUND, "authed not found")
        }
        Err(e) => {
            tracing::error!(?e, "remove_authed failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn connections(
    _session: AdminSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Some(central) = state.central() {
        match central.proxy_connections().await {
            Ok(conns) => {
                return V1Response::ok(serde_json::json!({ "connections": conns }));
            }
            Err(e) => tracing::warn!(?e, "central /connections proxy failed; falling back to DB"),
        }
    }
    match Database::global().connections_list().await {
        Ok(rows) => V1Response::ok(serde_json::json!({ "connections": rows })),
        Err(e) => {
            tracing::error!(?e, "connections_list failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn list_account_users(_session: AdminSession) -> impl IntoResponse {
    match Database::global().accounts_list_users().await {
        Ok(rows) => V1Response::ok(rows),
        Err(e) => {
            tracing::error!(?e, "list_account_users failed");
            V1Response::<Arc<[app_database::api::accounts::AccountUserInfo]>>::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error",
            )
        }
    }
}

pub async fn list_account_places(_session: AdminSession) -> impl IntoResponse {
    match Database::global().accounts_list_places().await {
        Ok(rows) => V1Response::ok(rows),
        Err(e) => {
            tracing::error!(?e, "list_account_places failed");
            V1Response::<Arc<[app_database::api::accounts::AccountPlaceInfo]>>::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error",
            )
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AccountUserPatchBody {
    pub username: Option<OptionalField<String>>,
    pub display_name: Option<OptionalField<String>>,
    pub is_bot: Option<OptionalField<bool>>,
}

pub async fn update_account_user(
    _session: WriteSession,
    Path(id): Path<String>,
    Json(body): Json<AccountUserPatchBody>,
) -> impl IntoResponse {
    let patch = app_database::api::accounts::AccountUserPatch {
        username: body.username.unwrap_or_default(),
        display_name: body.display_name.unwrap_or_default(),
        is_bot: body.is_bot.unwrap_or_default(),
    };
    match Database::global().accounts_update_user(&id, patch).await {
        Ok(res) => V1Response::ok(res),
        Err(e) => {
            tracing::error!(?e, "update_account_user failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AccountPlacePatchBody {
    pub kind: Option<OptionalField<String>>,
    pub name: Option<OptionalField<String>>,
    pub username: Option<OptionalField<String>>,
    pub parent_platform_id: Option<OptionalField<String>>,
}

pub async fn update_account_place(
    _session: WriteSession,
    Path(id): Path<String>,
    Json(body): Json<AccountPlacePatchBody>,
) -> impl IntoResponse {
    let patch = app_database::api::accounts::AccountPlacePatch {
        kind: body.kind.unwrap_or_default(),
        name: body.name.unwrap_or_default(),
        username: body.username.unwrap_or_default(),
        parent_platform_id: body.parent_platform_id.unwrap_or_default(),
    };
    match Database::global().accounts_update_place(&id, patch).await {
        Ok(res) => V1Response::ok(res),
        Err(e) => {
            tracing::error!(?e, "update_account_place failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn backfill_ordered_refs(_session: WriteSession) -> impl IntoResponse {
    match Database::global()
        .requests_start_backfill_ordered_refs()
        .await
    {
        Ok(res) => V1Response::ok(res),
        Err(e) => {
            tracing::error!(?e, "backfill_ordered_refs failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn metrics(_session: AdminSession, State(state): State<AppState>) -> impl IntoResponse {
    if let Some(central) = state.central() {
        match central.proxy_metrics_raw().await {
            Ok(text) => {
                return (
                    StatusCode::OK,
                    [(
                        header::CONTENT_TYPE,
                        "text/plain; version=0.0.4; charset=utf-8",
                    )],
                    text,
                )
                    .into_response();
            }
            Err(e) => tracing::warn!(?e, "central /metrics proxy failed"),
        }
    }
    V1Response::<()>::err(
        StatusCode::SERVICE_UNAVAILABLE,
        "central metrics unavailable",
    )
    .into_response()
}

pub async fn central_sessions(
    _session: AdminSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let Some(central) = state.central() else {
        return V1Response::err(
            StatusCode::SERVICE_UNAVAILABLE,
            "central client not connected",
        );
    };
    match central.list_sessions().await {
        Ok(app_peer_comms::rpc::request::AdminSessionsResult::Ok(sessions)) => {
            V1Response::ok(sessions)
        }
        Ok(app_peer_comms::rpc::request::AdminSessionsResult::Unauthorized) => {
            V1Response::err(StatusCode::FORBIDDEN, "central rejected admin session")
        }
        Err(e) => {
            tracing::error!(?e, "central_sessions RPC failed");
            V1Response::err(StatusCode::BAD_GATEWAY, "central RPC error")
        }
    }
}

pub async fn central_parked_workers(
    _session: AdminSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let Some(central) = state.central() else {
        return V1Response::err(
            StatusCode::SERVICE_UNAVAILABLE,
            "central client not connected",
        );
    };
    match central.list_parked_workers().await {
        Ok(app_peer_comms::rpc::request::AdminParkedWorkersResult::Ok(workers)) => {
            V1Response::ok(workers)
        }
        Ok(app_peer_comms::rpc::request::AdminParkedWorkersResult::Unauthorized) => {
            V1Response::err(StatusCode::FORBIDDEN, "central rejected admin session")
        }
        Err(e) => {
            tracing::error!(?e, "central_parked_workers RPC failed");
            V1Response::err(StatusCode::BAD_GATEWAY, "central RPC error")
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListRestrictionsQuery {
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

pub async fn list_restrictions(
    _session: AdminSession,
    Query(q): Query<ListRestrictionsQuery>,
) -> impl IntoResponse {
    match q.kind.as_deref() {
        Some("ban") => match Database::global().restrictions_list_bans().await {
            Ok(rows) => V1Response::ok(rows),
            Err(e) => {
                tracing::error!(?e, "list_restrictions(ban) failed");
                V1Response::<Arc<[app_database::entity::restrictions::RestrictionRow]>>::err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "database error",
                )
            }
        },
        Some("limit") => match Database::global().restrictions_list_limits().await {
            Ok(rows) => V1Response::ok(rows),
            Err(e) => {
                tracing::error!(?e, "list_restrictions(limit) failed");
                V1Response::<Arc<[app_database::entity::restrictions::RestrictionRow]>>::err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "database error",
                )
            }
        },
        _ => V1Response::<Arc<[app_database::entity::restrictions::RestrictionRow]>>::err(
            StatusCode::BAD_REQUEST,
            "invalid or missing `type` (expected `ban` or `limit`)",
        ),
    }
}

#[derive(Debug, Deserialize)]
pub struct AccountRefBody {
    pub platform: app_database::entity::accounts::Platform,
    pub id: String,
}

impl AccountRefBody {
    fn to_user_ref(&self) -> app_database::entity::accounts::AccountUserRef {
        app_database::entity::accounts::AccountUserRef {
            platform: self.platform,
            id: self.id.clone(),
        }
    }
    fn to_place_ref(&self) -> app_database::entity::accounts::AccountPlaceRef {
        app_database::entity::accounts::AccountPlaceRef {
            platform: self.platform,
            id: self.id.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "Type", rename_all = "lowercase")]
pub enum RuleBody {
    Ban {
        reason: String,
        ends_at: Option<String>,
        duration: Option<String>,
    },
    Limit {
        count: u64,
        timeframe: String,
    },
}

fn parse_span(s: &str) -> Result<jiff::Span, String> {
    jiff::fmt::friendly::SpanParser::new()
        .parse_span(s)
        .map_err(|e| e.to_string())
}

fn rule_body_to_rule(body: RuleBody) -> Result<app_database::entity::restrictions::Rule, String> {
    match body {
        RuleBody::Ban {
            reason,
            ends_at,
            duration,
        } => {
            let ends_at = if let Some(dur) = duration {
                let span = parse_span(&dur)?;
                Some(
                    jiff::Timestamp::now()
                        .checked_add(span)
                        .map_err(|e| e.to_string())?,
                )
            } else if let Some(ts) = ends_at {
                Some(ts.parse::<jiff::Timestamp>().map_err(|e| e.to_string())?)
            } else {
                None
            };
            Ok(app_database::entity::restrictions::Rule::Ban { reason, ends_at })
        }
        RuleBody::Limit { count, timeframe } => {
            let timeframe = parse_span(&timeframe)?;
            Ok(app_database::entity::restrictions::Rule::Limit { count, timeframe })
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateRestrictionBody {
    pub user: Option<AccountRefBody>,
    pub place: Option<AccountRefBody>,
    pub rule: RuleBody,
}

pub async fn create_restriction(
    _session: WriteSession,
    Json(body): Json<CreateRestrictionBody>,
) -> impl IntoResponse {
    if body.user.is_none() && body.place.is_none() {
        return V1Response::<app_database::api::restrictions::RestrictionCreateInfo>::err(
            StatusCode::BAD_REQUEST,
            "at least one of `user` or `place` is required",
        );
    }
    let rule = match rule_body_to_rule(body.rule) {
        Ok(r) => r,
        Err(e) => {
            return V1Response::<app_database::api::restrictions::RestrictionCreateInfo>::err(
                StatusCode::BAD_REQUEST,
                &e,
            );
        }
    };
    let user_ref = body.user.as_ref().map(AccountRefBody::to_user_ref);
    let place_ref = body.place.as_ref().map(AccountRefBody::to_place_ref);
    match Database::global()
        .restriction_create(user_ref.as_ref(), place_ref.as_ref(), &rule)
        .await
    {
        Ok(info) => V1Response::ok(info),
        Err(e) => {
            tracing::error!(?e, "create_restriction failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn remove_restriction(
    _session: WriteSession,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match Database::global()
        .restriction_remove(Arc::from(id.as_str()))
        .await
    {
        Ok(app_database::api::restrictions::RestrictionRemoveResult::Ok) => {
            V1Response::ok(serde_json::json!({ "removed": true }))
        }
        Ok(app_database::api::restrictions::RestrictionRemoveResult::NotFound) => {
            V1Response::err(StatusCode::NOT_FOUND, "restriction not found")
        }
        Err(e) => {
            tracing::error!(?e, "remove_restriction failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn replace_restriction(
    _session: WriteSession,
    Path(id): Path<String>,
    Json(body): Json<CreateRestrictionBody>,
) -> impl IntoResponse {
    if body.user.is_none() && body.place.is_none() {
        return V1Response::<serde_json::Value>::err(
            StatusCode::BAD_REQUEST,
            "at least one of `user` or `place` is required",
        );
    }
    let rule = match rule_body_to_rule(body.rule) {
        Ok(r) => r,
        Err(e) => {
            return V1Response::<serde_json::Value>::err(StatusCode::BAD_REQUEST, &e);
        }
    };
    let user_ref = body.user.as_ref().map(AccountRefBody::to_user_ref);
    let place_ref = body.place.as_ref().map(AccountRefBody::to_place_ref);
    match Database::global()
        .restriction_replace(
            Arc::from(id.as_str()),
            user_ref.as_ref(),
            place_ref.as_ref(),
            &rule,
        )
        .await
    {
        Ok(app_database::api::restrictions::RestrictionRemoveResult::Ok) => {
            V1Response::ok(serde_json::json!({ "updated": true }))
        }
        Ok(app_database::api::restrictions::RestrictionRemoveResult::NotFound) => {
            V1Response::err(StatusCode::NOT_FOUND, "restriction not found")
        }
        Err(e) => {
            tracing::error!(?e, "replace_restriction failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}
