use std::{collections::HashMap, sync::Arc};

use app_database::{
    Database,
    api::{
        accounts::{AccountPlaceInfo, AccountUserInfo, OptionalField},
        authed::{AuthedFullInfo, AuthedRemoveResult, AuthedRevokeResult, AuthedRotateTokenResult},
        log_settings::{LogSettings, LogSettingsScope},
        requests::{
            CancelResult, RemoveResult, RequestStatusType, RequestsByStatusPage, RetryResult,
        },
        secrets::SecretEntry,
    },
    entity::{
        accounts::{AccountPlaceRef, AccountUserRef, Platform},
        authed::AuthedForRole,
        requests::request_info::{RefreshAccountInfoPayload, RequestInfo},
    },
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

pub async fn list_counts(
    _session: AdminSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state.db.requests_counts().await {
        Ok(counts) => V1Response::ok(counts),
        Err(e) => {
            tracing::error!(?e, "list_counts failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

const LOG_SETTINGS_SCOPES: [LogSettingsScope; 5] = [
    LogSettingsScope::Global,
    LogSettingsScope::Central,
    LogSettingsScope::Worker,
    LogSettingsScope::Bot,
    LogSettingsScope::Admin,
];

fn parse_log_settings_scope(scope: &str) -> Option<LogSettingsScope> {
    LOG_SETTINGS_SCOPES
        .into_iter()
        .find(|candidate| candidate.as_str() == scope)
}

pub async fn list_log_settings(
    _session: AdminSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state.db.log_settings_list().await {
        Ok(rows) => {
            let settings = LOG_SETTINGS_SCOPES.map(|scope| {
                rows.iter()
                    .find(|setting| setting.scope == scope)
                    .cloned()
                    .unwrap_or(LogSettings {
                        scope,
                        console: None,
                        file: None,
                    })
            });
            V1Response::ok(settings)
        }
        Err(e) => {
            tracing::error!(?e, "list_log_settings failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SetLogSettingsBody {
    pub console: Option<String>,
    pub file: Option<String>,
}

pub async fn set_log_settings(
    _session: WriteSession,
    State(state): State<AppState>,
    Path(scope): Path<String>,
    Json(body): Json<SetLogSettingsBody>,
) -> impl IntoResponse {
    let Some(scope) = parse_log_settings_scope(&scope) else {
        return V1Response::<LogSettings>::err(StatusCode::BAD_REQUEST, "invalid scope");
    };
    let console = body.console.map(|value| value.trim().to_string());
    let file = body.file.map(|value| value.trim().to_string());
    for filter in [console.as_deref(), file.as_deref()].into_iter().flatten() {
        if let Err(e) = app_logger::validate_filter(filter) {
            return V1Response::<LogSettings>::err(
                StatusCode::BAD_REQUEST,
                format!("invalid log filter: {e}"),
            );
        }
    }

    match state
        .db
        .log_settings_set(scope, console.clone(), file.clone())
        .await
    {
        Ok(()) => V1Response::ok(LogSettings {
            scope,
            console,
            file,
        }),
        Err(e) => {
            tracing::error!(?e, "set_log_settings failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn list_secrets(
    _session: AdminSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state.db.secrets_list().await {
        Ok(rows) => V1Response::ok(rows),
        Err(e) => {
            tracing::error!(?e, "list_secrets failed");
            V1Response::<Vec<SecretEntry>>::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSecretBody {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub allowed_users: Vec<AccountRefBody>,
    #[serde(default)]
    pub allowed_places: Vec<AccountRefBody>,
}

pub async fn set_secret(
    _session: WriteSession,
    State(state): State<AppState>,
    Json(body): Json<SetSecretBody>,
) -> impl IntoResponse {
    let name = body.name.trim();
    let value = body.value.trim();
    if name.is_empty() {
        return V1Response::<SecretEntry>::err(StatusCode::BAD_REQUEST, "name is required");
    }
    let allowed_users: Vec<_> = body
        .allowed_users
        .iter()
        .map(AccountRefBody::to_user_ref)
        .collect();
    let allowed_places: Vec<_> = body
        .allowed_places
        .iter()
        .map(AccountRefBody::to_place_ref)
        .collect();
    match state
        .db
        .secrets_set(name, value, &allowed_users, &allowed_places)
        .await
    {
        Ok(()) => V1Response::ok(SecretEntry {
            name: name.to_string(),
            value: value.to_string(),
            updated_at: 0,
            allowed_users,
            allowed_places,
        }),
        Err(e) => {
            tracing::error!(?e, "set_secret failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn remove_secret(
    _session: WriteSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state.db.secrets_remove(&name).await {
        Ok(()) => V1Response::ok(serde_json::json!({ "removed": true })),
        Err(e) => {
            tracing::error!(?e, "remove_secret failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

const REQUESTS_LIMIT_MAX: i64 = 500;

pub async fn list_requests(
    _session: AdminSession,
    State(state): State<AppState>,
    Query(q): Query<ListRequestsQuery>,
) -> impl IntoResponse {
    let Some(status) = q.status.as_deref().and_then(parse_status_type) else {
        return V1Response::<RequestsByStatusPage>::err(
            StatusCode::BAD_REQUEST,
            "invalid or missing `status`",
        );
    };
    let limit = q.limit.map(|n| n.clamp(1, REQUESTS_LIMIT_MAX));
    match state
        .db
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

#[derive(Debug, Deserialize)]
pub struct ListRequestsByAccountQuery {
    pub platform: app_database::entity::accounts::Platform,
    pub id: String,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub cursor: Option<Arc<str>>,
}

pub async fn list_requests_by_user(
    _session: AdminSession,
    State(state): State<AppState>,
    Query(q): Query<ListRequestsByAccountQuery>,
) -> impl IntoResponse {
    if q.id.is_empty() {
        return V1Response::<RequestsByStatusPage>::err(StatusCode::BAD_REQUEST, "missing `id`");
    }
    let status = q.status.as_deref().and_then(parse_status_type);
    if q.status.is_some() && status.is_none() {
        return V1Response::<RequestsByStatusPage>::err(
            StatusCode::BAD_REQUEST,
            "invalid `status`",
        );
    }
    let limit = q.limit.map(|n| n.clamp(1, REQUESTS_LIMIT_MAX));
    match state
        .db
        .requests_get_by_ordered_by(q.platform, q.id.as_str(), status, limit, q.cursor)
        .await
    {
        Ok(page) => V1Response::ok(page),
        Err(e) => {
            tracing::error!(?e, "list_requests_by_user failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn list_requests_by_place(
    _session: AdminSession,
    State(state): State<AppState>,
    Query(q): Query<ListRequestsByAccountQuery>,
) -> impl IntoResponse {
    if q.id.is_empty() {
        return V1Response::<RequestsByStatusPage>::err(StatusCode::BAD_REQUEST, "missing `id`");
    }
    let status = q.status.as_deref().and_then(parse_status_type);
    if q.status.is_some() && status.is_none() {
        return V1Response::<RequestsByStatusPage>::err(
            StatusCode::BAD_REQUEST,
            "invalid `status`",
        );
    }
    let limit = q.limit.map(|n| n.clamp(1, REQUESTS_LIMIT_MAX));
    match state
        .db
        .requests_get_by_ordered_in(q.platform, q.id.as_str(), status, limit, q.cursor)
        .await
    {
        Ok(page) => V1Response::ok(page),
        Err(e) => {
            tracing::error!(?e, "list_requests_by_place failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn get_request(
    _session: AdminSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.requests_get(Arc::from(id.as_str())).await {
        Ok(req) => V1Response::ok(req),
        Err(e) => {
            tracing::error!(?e, "get_request failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn retry_request(
    _session: WriteSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.requests_retry(Arc::from(id.as_str())).await {
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

pub async fn cancel_request(
    session: WriteSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state
        .db
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

pub async fn remove_request(
    _session: WriteSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.requests_remove(Arc::from(id.as_str())).await {
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

pub async fn clear_refusals(
    _session: WriteSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state
        .db
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
    State(state): State<AppState>,
    jar: axum_extra::extract::SignedCookieJar,
    Json(body): Json<LoginBody>,
) -> axum::response::Response {
    match state
        .db
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

pub async fn me(session: AdminSession, State(state): State<AppState>) -> impl IntoResponse {
    let readonly = session.readonly();
    match state
        .db
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

pub async fn list_authed(
    _session: AdminSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state.db.authed_list_full().await {
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
    State(state): State<AppState>,
    Json(body): Json<CreateAuthedBody>,
) -> impl IntoResponse {
    match state
        .db
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

pub async fn revoke_authed(
    _session: WriteSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.authed_revoke(Arc::from(id.as_str())).await {
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

pub async fn rotate_authed(
    _session: WriteSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.authed_rotate_token(Arc::from(id.as_str())).await {
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

pub async fn remove_authed(
    _session: WriteSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.authed_remove(Arc::from(id.as_str())).await {
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
    match state.db.connections_list().await {
        Ok(rows) => V1Response::ok(serde_json::json!({ "connections": rows })),
        Err(e) => {
            tracing::error!(?e, "connections_list failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

pub async fn list_account_users(
    _session: AdminSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state.db.accounts_list_users().await {
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

pub async fn list_account_places(
    _session: AdminSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state.db.accounts_list_places().await {
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
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AccountUserPatchBody>,
) -> impl IntoResponse {
    let patch = app_database::api::accounts::AccountUserPatch {
        username: body.username.unwrap_or_default(),
        display_name: body.display_name.unwrap_or_default(),
        is_bot: body.is_bot.unwrap_or_default(),
    };
    match state.db.accounts_update_user(&id, patch).await {
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
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AccountPlacePatchBody>,
) -> impl IntoResponse {
    let patch = app_database::api::accounts::AccountPlacePatch {
        kind: body.kind.unwrap_or_default(),
        name: body.name.unwrap_or_default(),
        username: body.username.unwrap_or_default(),
        parent_platform_id: body.parent_platform_id.unwrap_or_default(),
    };
    match state.db.accounts_update_place(&id, patch).await {
        Ok(res) => V1Response::ok(res),
        Err(e) => {
            tracing::error!(?e, "update_account_place failed");
            V1Response::err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

const REFRESH_BATCH_SIZE: usize = 40;

fn refresh_idempotency_key(kind: &str, platform: Platform, platform_id: &str) -> String {
    let day = jiff::Timestamp::now().strftime("%Y%m%d").to_string();
    format!("refresh-{kind}-{platform}-{platform_id}-{day}")
}

async fn enqueue_account_refresh(
    db: &Database,
    admin_id: &str,
    payload: RefreshAccountInfoPayload,
    idempotency_key: String,
) -> Result<app_database::api::requests::RequestIdResponse, app_database::DatabaseError> {
    db.requests_add(
        Arc::from(admin_id),
        RequestInfo::RefreshAccountInfo(payload),
        HashMap::new(),
        Some(idempotency_key),
        None,
        None,
    )
    .await
}

const fn is_stale_user(user: &AccountUserInfo) -> bool {
    user.username.is_none() && user.display_name.is_none()
}

const fn is_stale_place(place: &AccountPlaceInfo) -> bool {
    place.name.is_none() && place.username.is_none()
}

fn batch_refs<T, F>(items: &[T], batch_size: usize, map_ref: F) -> Vec<Vec<T>>
where
    T: Clone,
    F: Fn(&T) -> (Platform, String),
{
    let mut by_platform: HashMap<Platform, Vec<T>> = HashMap::new();
    for item in items {
        let (platform, _) = map_ref(item);
        by_platform.entry(platform).or_default().push(item.clone());
    }

    let mut batches = Vec::new();
    for (_platform, group) in by_platform {
        for chunk in group.chunks(batch_size) {
            batches.push(chunk.to_vec());
        }
    }
    batches
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshEnqueueResponse {
    pub request_id: Arc<str>,
}

pub async fn refresh_account_user(
    session: WriteSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let user = match state.db.accounts_get_user(&id).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return V1Response::<RefreshEnqueueResponse>::err(
                StatusCode::NOT_FOUND,
                "user not found",
            );
        }
        Err(e) => {
            tracing::error!(?e, "refresh_account_user lookup failed");
            return V1Response::<RefreshEnqueueResponse>::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error",
            );
        }
    };

    let payload = RefreshAccountInfoPayload {
        users: vec![AccountUserRef {
            platform: user.platform,
            id: user.platform_id.clone(),
        }],
        places: vec![],
    };
    let idempotency_key = refresh_idempotency_key("user", user.platform, &user.platform_id);

    match enqueue_account_refresh(&state.db, session.admin_id(), payload, idempotency_key).await {
        Ok(res) => V1Response::ok(RefreshEnqueueResponse { request_id: res.id }),
        Err(e) => {
            tracing::error!(?e, "refresh_account_user enqueue failed");
            V1Response::<RefreshEnqueueResponse>::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error",
            )
        }
    }
}

pub async fn refresh_account_place(
    session: WriteSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let place = match state.db.accounts_get_place(&id).await {
        Ok(Some(place)) => place,
        Ok(None) => {
            return V1Response::<RefreshEnqueueResponse>::err(
                StatusCode::NOT_FOUND,
                "place not found",
            );
        }
        Err(e) => {
            tracing::error!(?e, "refresh_account_place lookup failed");
            return V1Response::<RefreshEnqueueResponse>::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error",
            );
        }
    };

    let payload = RefreshAccountInfoPayload {
        users: vec![],
        places: vec![AccountPlaceRef {
            platform: place.platform,
            id: place.platform_id.clone(),
        }],
    };
    let idempotency_key = refresh_idempotency_key("place", place.platform, &place.platform_id);

    match enqueue_account_refresh(&state.db, session.admin_id(), payload, idempotency_key).await {
        Ok(res) => V1Response::ok(RefreshEnqueueResponse { request_id: res.id }),
        Err(e) => {
            tracing::error!(?e, "refresh_account_place enqueue failed");
            V1Response::<RefreshEnqueueResponse>::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error",
            )
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RefreshStaleResponse {
    pub enqueued: u64,
}

pub async fn refresh_stale_accounts(
    session: WriteSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let (users, places) = match tokio::try_join!(
        state.db.accounts_list_users(),
        state.db.accounts_list_places(),
    ) {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!(?e, "refresh_stale_accounts list failed");
            return V1Response::<RefreshStaleResponse>::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error",
            );
        }
    };

    let stale_users: Vec<AccountUserInfo> =
        users.iter().filter(|u| is_stale_user(u)).cloned().collect();
    let stale_places: Vec<AccountPlaceInfo> = places
        .iter()
        .filter(|p| is_stale_place(p))
        .cloned()
        .collect();

    let user_batches = batch_refs(&stale_users, REFRESH_BATCH_SIZE, |u| {
        (u.platform, u.platform_id.clone())
    });
    let place_batches = batch_refs(&stale_places, REFRESH_BATCH_SIZE, |p| {
        (p.platform, p.platform_id.clone())
    });

    let mut enqueued = 0u64;
    let admin_id = session.admin_id();

    for batch in user_batches {
        if batch.is_empty() {
            continue;
        }
        let platform = batch[0].platform;
        let refs: Vec<AccountUserRef> = batch
            .iter()
            .map(|u| AccountUserRef {
                platform: u.platform,
                id: u.platform_id.clone(),
            })
            .collect();
        let idempotency_key = refresh_idempotency_key(
            "users-batch",
            platform,
            &format!("{}:{}", batch.len(), batch[0].platform_id),
        );
        let payload = RefreshAccountInfoPayload {
            users: refs,
            places: vec![],
        };
        match enqueue_account_refresh(&state.db, admin_id, payload, idempotency_key).await {
            Ok(_) => enqueued += 1,
            Err(e) => {
                tracing::error!(?e, "refresh_stale_accounts user batch failed");
                return V1Response::<RefreshStaleResponse>::err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "database error",
                );
            }
        }
    }

    for batch in place_batches {
        if batch.is_empty() {
            continue;
        }
        let platform = batch[0].platform;
        let refs: Vec<AccountPlaceRef> = batch
            .iter()
            .map(|p| AccountPlaceRef {
                platform: p.platform,
                id: p.platform_id.clone(),
            })
            .collect();
        let idempotency_key = refresh_idempotency_key(
            "places-batch",
            platform,
            &format!("{}:{}", batch.len(), batch[0].platform_id),
        );
        let payload = RefreshAccountInfoPayload {
            users: vec![],
            places: refs,
        };
        match enqueue_account_refresh(&state.db, admin_id, payload, idempotency_key).await {
            Ok(_) => enqueued += 1,
            Err(e) => {
                tracing::error!(?e, "refresh_stale_accounts place batch failed");
                return V1Response::<RefreshStaleResponse>::err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "database error",
                );
            }
        }
    }

    V1Response::ok(RefreshStaleResponse { enqueued })
}

pub async fn backfill_ordered_refs(
    _session: WriteSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state.db.requests_start_backfill_ordered_refs().await {
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
    State(state): State<AppState>,
    Query(q): Query<ListRestrictionsQuery>,
) -> impl IntoResponse {
    match q.kind.as_deref() {
        Some("ban") => match state.db.restrictions_list_bans().await {
            Ok(rows) => V1Response::ok(rows),
            Err(e) => {
                tracing::error!(?e, "list_restrictions(ban) failed");
                V1Response::<Arc<[app_database::entity::restrictions::RestrictionRow]>>::err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "database error",
                )
            }
        },
        Some("limit") => match state.db.restrictions_list_limits().await {
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
    State(state): State<AppState>,
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
    match state
        .db
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
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.restriction_remove(Arc::from(id.as_str())).await {
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
    State(state): State<AppState>,
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
    match state
        .db
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
