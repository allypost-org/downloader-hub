use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    Database, DatabaseError, DatabaseRequest,
    entity::accounts::{AccountPlace, AccountPlaceRef, AccountUser, AccountUserRef, Platform},
    error::ResponseError,
};

fn platform_value(platform: Platform) -> convex::Value {
    convex::Value::String(platform.as_str().to_string())
}

fn insert_opt_string(
    obj: &mut std::collections::BTreeMap<String, convex::Value>,
    key: &str,
    value: Option<&String>,
) {
    if let Some(v) = value {
        obj.insert(key.into(), convex::Value::String(v.clone()));
    }
}

fn insert_opt_bool(
    obj: &mut std::collections::BTreeMap<String, convex::Value>,
    key: &str,
    value: Option<bool>,
) {
    if let Some(v) = value {
        obj.insert(key.into(), convex::Value::Boolean(v));
    }
}

pub(crate) fn user_ref_value(r#ref: &AccountUserRef) -> convex::Value {
    let mut obj: std::collections::BTreeMap<String, convex::Value> =
        std::collections::BTreeMap::new();
    obj.insert("platform".into(), platform_value(r#ref.platform));
    obj.insert("id".into(), convex::Value::String(r#ref.id.clone()));
    convex::Value::Object(obj)
}

pub(crate) fn place_ref_value(r#ref: &AccountPlaceRef) -> convex::Value {
    let mut obj: std::collections::BTreeMap<String, convex::Value> =
        std::collections::BTreeMap::new();
    obj.insert("platform".into(), platform_value(r#ref.platform));
    obj.insert("id".into(), convex::Value::String(r#ref.id.clone()));
    convex::Value::Object(obj)
}

fn user_value(user: &AccountUser) -> convex::Value {
    let mut obj: std::collections::BTreeMap<String, convex::Value> =
        std::collections::BTreeMap::new();
    obj.insert("platform".into(), platform_value(user.platform));
    obj.insert(
        "platformId".into(),
        convex::Value::String(user.platform_id.clone()),
    );
    insert_opt_string(&mut obj, "username", user.username.as_ref());
    insert_opt_string(&mut obj, "displayName", user.display_name.as_ref());
    insert_opt_bool(&mut obj, "isBot", user.is_bot);
    convex::Value::Object(obj)
}

fn place_value(place: &AccountPlace) -> convex::Value {
    let mut obj: std::collections::BTreeMap<String, convex::Value> =
        std::collections::BTreeMap::new();
    obj.insert("platform".into(), platform_value(place.platform));
    obj.insert(
        "platformId".into(),
        convex::Value::String(place.platform_id.clone()),
    );
    insert_opt_string(&mut obj, "kind", place.kind.as_ref());
    insert_opt_string(&mut obj, "name", place.name.as_ref());
    insert_opt_string(&mut obj, "username", place.username.as_ref());
    insert_opt_string(
        &mut obj,
        "parentPlatformId",
        place.parent_platform_id.as_ref(),
    );
    convex::Value::Object(obj)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsUpsertResult {
    #[serde(with = "crate::helpers::serde::bigint")]
    pub users: u64,
    #[serde(with = "crate::helpers::serde::bigint")]
    pub places: u64,
}

impl Database {
    pub async fn accounts_upsert(
        &self,
        users: &[AccountUser],
        places: &[AccountPlace],
    ) -> Result<AccountsUpsertResult, DatabaseError> {
        DatabaseRequest::named("accounts:upsert")
            .with_arg(
                "users",
                convex::Value::Array(users.iter().map(user_value).collect()),
            )
            .with_arg(
                "places",
                convex::Value::Array(places.iter().map(place_value).collect()),
            )
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUserInfo {
    pub id: Arc<str>,
    pub platform: Platform,
    pub platform_id: String,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub is_bot: Option<bool>,
    #[serde(with = "crate::helpers::serde::bigint")]
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountPlaceInfo {
    pub id: Arc<str>,
    pub platform: Platform,
    pub platform_id: String,
    pub kind: Option<String>,
    pub name: Option<String>,
    pub username: Option<String>,
    pub parent_platform_id: Option<String>,
    #[serde(with = "crate::helpers::serde::bigint")]
    pub last_seen: u64,
}

impl Database {
    pub async fn accounts_list_users(&self) -> Result<Arc<[AccountUserInfo]>, DatabaseError> {
        DatabaseRequest::named("accounts:listUsers")
            .query(self)
            .await
    }

    pub async fn accounts_list_places(&self) -> Result<Arc<[AccountPlaceInfo]>, DatabaseError> {
        DatabaseRequest::named("accounts:listPlaces")
            .query(self)
            .await
    }

    pub async fn accounts_get_user(
        &self,
        id: &str,
    ) -> Result<Option<AccountUserInfo>, DatabaseError> {
        DatabaseRequest::named("accounts:getUser")
            .with_arg("id", id)
            .query(self)
            .await
    }

    pub async fn accounts_get_place(
        &self,
        id: &str,
    ) -> Result<Option<AccountPlaceInfo>, DatabaseError> {
        DatabaseRequest::named("accounts:getPlace")
            .with_arg("id", id)
            .query(self)
            .await
    }

    /// Live subscription to the projection backing the admin stream's
    /// name-resolution map.
    pub async fn accounts_watch_for_stream(
        &self,
    ) -> Result<
        impl futures::stream::Stream<Item = Result<AccountStreamSnapshot, ResponseError>>,
        DatabaseError,
    > {
        DatabaseRequest::named("accounts:getAllForStream")
            .watch_query(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStreamSnapshot {
    pub users: Arc<[AccountStreamUser]>,
    pub places: Arc<[AccountStreamPlace]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStreamUser {
    pub platform: Platform,
    pub platform_id: String,
    pub username: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStreamPlace {
    pub platform: Platform,
    pub platform_id: String,
    pub kind: Option<String>,
    pub name: Option<String>,
    pub username: Option<String>,
    pub parent_platform_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUpdateResult {
    pub ok: bool,
}

/// Three-state field for partial updates: omit the key, set it to null (clear),
/// or set it to a value.
#[derive(Debug, Clone, Default)]
pub enum OptionalField<T> {
    #[default]
    Omit,
    Null,
    Value(T),
}

impl<T> OptionalField<T> {
    fn encoded(&self, encode: impl Fn(&T) -> convex::Value) -> Option<convex::Value> {
        match self {
            Self::Omit => None,
            Self::Null => Some(convex::Value::Null),
            Self::Value(v) => Some(encode(v)),
        }
    }
}

impl<'de, T> serde::Deserialize<'de> for OptionalField<T>
where
    T: serde::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let opt = Option::<T>::deserialize(deserializer)?;
        Ok(opt.map_or(Self::Null, Self::Value))
    }
}

#[derive(Debug, Clone, Default)]
pub struct AccountUserPatch {
    pub username: OptionalField<String>,
    pub display_name: OptionalField<String>,
    pub is_bot: OptionalField<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct AccountPlacePatch {
    pub kind: OptionalField<String>,
    pub name: OptionalField<String>,
    pub username: OptionalField<String>,
    pub parent_platform_id: OptionalField<String>,
}

impl Database {
    pub async fn accounts_update_user(
        &self,
        id: &str,
        patch: AccountUserPatch,
    ) -> Result<AccountUpdateResult, DatabaseError> {
        let mut args: std::collections::BTreeMap<String, convex::Value> =
            std::collections::BTreeMap::new();
        args.insert("id".into(), convex::Value::String(id.to_string()));
        if let Some(v) = patch.username.encoded(|s| convex::Value::String(s.clone())) {
            args.insert("username".into(), v);
        }
        if let Some(v) = patch
            .display_name
            .encoded(|s| convex::Value::String(s.clone()))
        {
            args.insert("displayName".into(), v);
        }
        if let Some(v) = patch.is_bot.encoded(|b| convex::Value::Boolean(*b)) {
            args.insert("isBot".into(), v);
        }
        DatabaseRequest::named("accounts:updateUser")
            .with_args(args)
            .mutate(self)
            .await
    }

    pub async fn accounts_update_place(
        &self,
        id: &str,
        patch: AccountPlacePatch,
    ) -> Result<AccountUpdateResult, DatabaseError> {
        let mut args: std::collections::BTreeMap<String, convex::Value> =
            std::collections::BTreeMap::new();
        args.insert("id".into(), convex::Value::String(id.to_string()));
        if let Some(v) = patch.kind.encoded(|s| convex::Value::String(s.clone())) {
            args.insert("kind".into(), v);
        }
        if let Some(v) = patch.name.encoded(|s| convex::Value::String(s.clone())) {
            args.insert("name".into(), v);
        }
        if let Some(v) = patch.username.encoded(|s| convex::Value::String(s.clone())) {
            args.insert("username".into(), v);
        }
        if let Some(v) = patch
            .parent_platform_id
            .encoded(|s| convex::Value::String(s.clone()))
        {
            args.insert("parentPlatformId".into(), v);
        }
        DatabaseRequest::named("accounts:updatePlace")
            .with_args(args)
            .mutate(self)
            .await
    }
}
