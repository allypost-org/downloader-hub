use std::sync::Arc;

pub use jsonwebtoken::errors::{Error as JwtError, ErrorKind as JwtErrorKind};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};

use super::{JwtPair, MAIN_ISSUER};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TargetedJwtPair(JwtPair);
impl TargetedJwtPair {
    #[must_use]
    pub const fn new(pair: JwtPair) -> Self {
        Self(pair)
    }

    #[must_use]
    pub fn token(&self) -> Arc<str> {
        self.0.token.clone()
    }

    #[must_use]
    pub fn refresh_token(&self) -> Arc<str> {
        self.0.refresh_token.clone()
    }

    #[must_use]
    pub fn into_pair(self) -> JwtPair {
        self.0
    }

    pub fn generate<'a, T>(config: &TargetedJwtConfig, encoding_key: T) -> Result<Self, JwtError>
    where
        T: Into<&'a [u8]>,
    {
        let key = EncodingKey::from_secret(encoding_key.into());

        let now = chrono::Utc::now();

        let token_timeout = now
            .checked_add_signed(config.token_expiration_duration)
            .expect("Failed to add token expiration duration to now");

        let refresh_token_timeout = now
            .checked_add_signed(config.refresh_token_expiration_duration)
            .expect("Failed to add refresh token expiration duration to now");

        let max_expires_at = config.expires_at.unwrap_or(refresh_token_timeout);

        let claims_base = TargetedJwtClaims {
            issuer: MAIN_ISSUER.into(),
            audience: "".into(),
            issued_at: now.timestamp(),
            expires_at: 0,
            id: config.id.clone(),
            target: "".into(),
        };

        let claims = claims_base
            .clone()
            .with_expires_at(token_timeout.timestamp())
            .with_target(config.audience.clone())
            .with_audience_target(config.audience.as_ref());

        let header = header();
        let token = encode(&header, &claims, &key)?;

        let refresh_claims = claims_base
            .with_expires_at(
                refresh_token_timeout
                    .timestamp()
                    .min(max_expires_at.timestamp()),
            )
            .with_target(TargetedJwtClaims::refresh_audience().into())
            .with_audience_refresh();

        let refresh_token = encode(&header, &refresh_claims, &key)?;

        Ok(Self::new(JwtPair {
            token: token.into(),
            refresh_token: refresh_token.into(),
        }))
    }
}

pub struct TargetedJwtConfig {
    pub id: Arc<str>,
    pub audience: Arc<str>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub token_expiration_duration: chrono::Duration,
    pub refresh_token_expiration_duration: chrono::Duration,
}

impl TargetedJwtConfig {
    #[must_use]
    pub const fn new(id: Arc<str>, audience: Arc<str>) -> Self {
        Self {
            id,
            audience,
            expires_at: None,
            token_expiration_duration: Self::default_token_expiration_duration(),
            refresh_token_expiration_duration: Self::default_refresh_token_expiration_duration(),
        }
    }

    #[must_use]
    pub const fn with_expires_at(
        mut self,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Self {
        self.expires_at = expires_at;
        self
    }

    #[must_use]
    pub const fn default_token_expiration_duration() -> chrono::Duration {
        chrono::Duration::minutes(5)
    }

    #[must_use]
    pub const fn default_refresh_token_expiration_duration() -> chrono::Duration {
        chrono::Duration::days(30)
    }
}

#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetedJwtClaims {
    #[serde(rename = "iss")]
    pub issuer: Arc<str>,
    #[serde(rename = "aud")]
    pub audience: Arc<str>,
    #[serde(rename = "iat")]
    pub issued_at: i64,
    #[serde(rename = "exp")]
    pub expires_at: i64,
    #[serde(rename = "id")]
    pub id: Arc<str>,
    #[serde(rename = "t")]
    pub target: Arc<str>,
}

impl TargetedJwtClaims {
    fn in_audience(audience: &str) -> Arc<str> {
        format!("{}::{}", MAIN_ISSUER, audience).into()
    }

    #[must_use]
    pub const fn refresh_audience() -> &'static str {
        "refresh"
    }

    fn audience_refresh() -> Arc<str> {
        Self::in_audience(Self::refresh_audience())
    }

    #[must_use]
    pub const fn with_expires_at(mut self, expires_at: i64) -> Self {
        self.expires_at = expires_at;
        self
    }

    #[must_use]
    pub fn with_audience_target(self, audience: &str) -> Self {
        let aud = Self::in_audience(audience);
        self.with_audience(aud)
    }

    #[must_use]
    pub fn with_audience_refresh(self) -> Self {
        let aud = Self::audience_refresh();
        self.with_audience(aud)
    }

    fn with_audience(mut self, audience: Arc<str>) -> Self {
        self.audience = audience;
        self
    }

    #[must_use]
    pub fn with_target(mut self, target: Arc<str>) -> Self {
        self.target = target;
        self
    }

    #[must_use]
    pub fn into_config(self) -> TargetedJwtConfig {
        TargetedJwtConfig::new(self.id.clone(), self.audience)
    }
}

impl TargetedJwtClaims {
    #[must_use]
    pub fn is_refresh(&self) -> bool {
        self.audience.as_ref() == Self::audience_refresh().as_ref()
    }
}

impl TargetedJwtClaims {
    pub fn parse<'a, T>(
        target: Option<&str>,
        token: &str,
        decoding_key: T,
    ) -> Result<Self, JwtError>
    where
        T: Into<&'a [u8]>,
    {
        let key = DecodingKey::from_secret(decoding_key.into());

        let header = header();
        let validation = {
            let mut v = jsonwebtoken::Validation::new(header.alg);
            if let Some(target) = target {
                v.set_audience(&[Self::in_audience(target), Self::audience_refresh()]);
            } else {
                let hacky_parsed = Self::parse_unchecked(token)?;
                v.set_audience(&[Self::in_audience(hacky_parsed.target.as_ref())]);
            }
            v.set_issuer(&[MAIN_ISSUER.to_string()]);

            v
        };

        let token_data = jsonwebtoken::decode(token, &key, &validation)?;

        Ok(token_data.claims)
    }

    pub fn parse_unchecked(token: &str) -> Result<Self, JwtError> {
        let token_data = jsonwebtoken::dangerous::insecure_decode(token)?;

        Ok(token_data.claims)
    }
}

fn header() -> Header {
    Header::default()
}
