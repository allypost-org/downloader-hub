use std::sync::{Arc, OnceLock};

use app_peer_comms::jwt::targeted::TargetedJwtPair;
use tokio::sync::RwLock;
use tracing::{debug, instrument, trace};
use url::Url;

static JWT_DATA: OnceLock<Arc<JwtData>> = OnceLock::new();

pub struct JwtData {
    tokens: Arc<RwLock<TargetedJwtPair>>,
}

impl JwtData {
    pub fn init(tokens: TargetedJwtPair) {
        _ = JWT_DATA.set(Arc::new(Self {
            tokens: Arc::new(RwLock::new(tokens)),
        }));
    }

    pub async fn init_or_update(tokens: TargetedJwtPair) {
        let Some(jwt_data) = JWT_DATA.get() else {
            Self::init(tokens);
            return;
        };

        jwt_data.update_tokens(tokens).await;
    }

    pub async fn update_tokens(&self, tokens: TargetedJwtPair) {
        let jwt_data = JWT_DATA.get().expect("jwt data is not initialized");
        *jwt_data.tokens.write().await = tokens;
    }

    pub fn global() -> Arc<Self> {
        JWT_DATA.get().expect("jwt data is not initialized").clone()
    }

    pub async fn token(&self) -> Arc<str> {
        self.tokens.read().await.token()
    }

    pub async fn get_token() -> Arc<str> {
        Self::global().token().await
    }
}

impl JwtData {
    #[instrument(skip_all, fields(from = %base_url.as_str()))]
    pub async fn fetch_with_api_key(
        base_url: Url,
        api_key: Arc<str>,
    ) -> Result<TargetedJwtPair, Box<dyn std::error::Error + Send + Sync>> {
        debug!("Fetching token with API key");
        let token_response = reqwest::Client::new()
            .post(base_url.join("/api/v1/auth/token")?)
            .header("Content-Type", "application/json")
            .body(
                serde_json::to_string(&serde_json::json!({
                    "apiKey": api_key,
                }))
                .expect("failed to serialize string"),
            )
            .send()
            .await?
            .error_for_status()?
            .json::<V1Response<TargetedJwtPair>>()
            .await?;

        trace!(?token_response, "Fetched token with API key");

        let token_response = match token_response {
            V1Response::Ok(x) => x,
            V1Response::Err(e) => {
                return Err(e.into());
            }
        };

        debug!("Got token");

        Ok(token_response)
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "status", content = "data")]
pub enum V1Response<T> {
    #[serde(rename = "ok")]
    Ok(T),
    #[serde(rename = "error")]
    Err(String),
}
