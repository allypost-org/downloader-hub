use std::string::ToString;

use app_entities::{client, entity_meta::common::path::AppPath};
use axum::{extract::Request, http};
use sea_orm::prelude::*;
use serde::Deserialize;
use tracing::trace;

use crate::{config::Config, db::AppDb};

const ADMIN_ID: i32 = 0;

pub type CurrentUser = app_entities::client::Model;

pub fn get_user_from_request(req: &Request) -> Option<&CurrentUser> {
    req.extensions().get::<CurrentUser>()
}

pub const fn is_admin(user: &CurrentUser) -> bool {
    user.id == ADMIN_ID
}

#[derive(Debug, Deserialize)]
pub struct AuthQueryKey {
    auth_client_key: Option<String>,
}

#[allow(clippy::needless_pass_by_ref_mut)]
pub async fn add_user_to_request(
    query: Option<AuthQueryKey>,
    req: &mut Request,
) -> Option<CurrentUser> {
    if let Some(user) = get_user_from_request(req) {
        trace!("User already in request");
        return Some(user.to_owned());
    }

    trace!("Adding user to request");

    let auth_key = query.and_then(|x| x.auth_client_key).or_else(|| {
        req.headers()
            .get(http::header::AUTHORIZATION)
            .and_then(|header| header.to_str().ok())
            .map(ToString::to_string)
    })?;

    trace!(key = ?auth_key, "Got authorization");

    AuthorizationSchema::authorize_from_value(&auth_key)
        .await
        .inspect(|user| {
            tracing::Span::current().record("user", tracing::field::debug(&user.id));
            trace!("Added user to request");
            req.extensions_mut().insert(user.clone());
        })
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
enum AuthorizationSchema {
    ApiKey(String),
    AdminKey(String),
}
impl AuthorizationSchema {
    fn from_value(s: &str) -> Option<Self> {
        let (auth_schema, auth_params) = s.split_once(' ')?;
        let val = match auth_schema.to_lowercase().as_str() {
            "client-key" => Self::ApiKey(auth_params.to_string()),
            "admin-key" => Self::AdminKey(auth_params.to_string()),
            _ => return None,
        };

        Some(val)
    }

    async fn authorize_from_value(s: &str) -> Option<CurrentUser> {
        Self::from_value(s)?.authorize(&AppDb::db()).await
    }

    pub async fn authorize(&self, db: &DatabaseConnection) -> Option<CurrentUser> {
        match self {
            Self::ApiKey(key) => {
                let res = client::Entity::find()
                    .filter(client::Column::ApiKey.eq(key))
                    .one(db)
                    .await
                    .ok()
                    .flatten()?;

                Some(res)
            }

            Self::AdminKey(key) if Config::server().run.admin_key == *key => Some(CurrentUser {
                id: ADMIN_ID,
                name: "admin".to_string(),
                api_key: key.clone(),
                app_meta: serde_json::json!({
                    "admin": true,
                }),
                download_folder: AppPath::None.into(),
                created_at: chrono::Utc::now().fixed_offset(),
                updated_at: chrono::Utc::now().fixed_offset(),
            }),
            Self::AdminKey(_) => None,
        }
    }
}
