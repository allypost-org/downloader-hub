use std::sync::Arc;

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, request::Parts},
};
use axum_extra::extract::cookie::{Cookie, Expiration, Key, SameSite, SignedCookieJar};
use cookie::time::{Duration as TimeDuration, OffsetDateTime};
use serde::{Deserialize, Serialize};

use super::AppState;

const SESSION_COOKIE: &str = "admin_session";
const SESSION_TTL_SECS: i64 = 12 * 60 * 60;

/// In debug builds the cookie is served without `Secure` so plaintext `http://`
/// dev setups work; in release builds `Secure` is always set.
const COOKIE_SECURE: bool = !cfg!(debug_assertions);

#[derive(Clone)]
pub struct SessionKey(Arc<Key>);

impl SessionKey {
    #[must_use]
    pub fn new(secret: &[u8]) -> Self {
        Self(Arc::new(Key::derive_from(secret)))
    }
}

impl FromRef<AppState> for SessionKey {
    fn from_ref(state: &AppState) -> Self {
        Self(state.session_key.0.clone())
    }
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.session_key.0.as_ref().clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionClaims {
    pub admin_id: String,
    pub readonly: bool,
    pub exp: i64,
}

pub fn build_session_cookie(claims: &SessionClaims) -> Cookie<'static> {
    let value = serde_json::to_string(claims).unwrap_or_default();
    let expires =
        Expiration::from(OffsetDateTime::now_utc() + TimeDuration::seconds(SESSION_TTL_SECS));
    let mut builder = Cookie::build((SESSION_COOKIE, value))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Strict)
        .expires(expires);
    if COOKIE_SECURE {
        builder = builder.secure(true);
    }
    builder.build()
}

pub fn read_claims(jar: &SignedCookieJar) -> Option<SessionClaims> {
    let cookie = jar.get(SESSION_COOKIE)?;
    let claims: SessionClaims = serde_json::from_str(cookie.value()).ok()?;
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    if claims.exp < now_secs {
        return None;
    }
    Some(claims)
}

pub struct AdminSession(pub SessionClaims);

impl FromRequestParts<AppState> for AdminSession {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = SignedCookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        read_claims(&jar).map(Self).ok_or(StatusCode::UNAUTHORIZED)
    }
}

impl AdminSession {
    #[must_use]
    pub fn admin_id(&self) -> &str {
        &self.0.admin_id
    }

    #[must_use]
    pub const fn readonly(&self) -> bool {
        self.0.readonly
    }
}

/// Like [`AdminSession`], but additionally rejects sessions whose token has
/// `readonly = true`. Use on any handler that mutates state.
pub struct WriteSession(pub AdminSession);

impl FromRequestParts<AppState> for WriteSession {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = AdminSession::from_request_parts(parts, state).await?;
        if session.readonly() {
            return Err(StatusCode::FORBIDDEN);
        }
        Ok(Self(session))
    }
}

impl std::ops::Deref for WriteSession {
    type Target = AdminSession;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub const fn session_cookie_name() -> &'static str {
    SESSION_COOKIE
}

pub fn make_claims(admin_id: &str, readonly: bool) -> SessionClaims {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    SessionClaims {
        admin_id: admin_id.to_string(),
        readonly,
        exp: now_secs + SESSION_TTL_SECS,
    }
}
