use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    Database, DatabaseError, DatabaseRequest,
    api::accounts::{place_ref_value, user_ref_value},
    entity::{
        accounts::{AccountPlaceRef, AccountUserRef},
        restrictions::{RestrictionRow, Rule},
    },
    error::ResponseError,
};

fn rule_value(rule: &Rule) -> convex::Value {
    let mut obj: std::collections::BTreeMap<String, convex::Value> =
        std::collections::BTreeMap::new();
    match rule {
        Rule::Ban { reason, ends_at } => {
            obj.insert("Type".into(), convex::Value::String("ban".into()));
            obj.insert("reason".into(), convex::Value::String(reason.clone()));
            if let Some(ends_at) = ends_at {
                obj.insert(
                    "endsAt".into(),
                    convex::Value::Int64(ends_at.as_millisecond()),
                );
            }
        }
        Rule::Limit { count, timeframe } => {
            obj.insert("Type".into(), convex::Value::String("limit".into()));
            obj.insert(
                "count".into(),
                convex::Value::Int64(i64::try_from(*count).unwrap_or(i64::MAX)),
            );
            let ms = timeframe.total(jiff::Unit::Millisecond).map_or(0, |f| {
                #[allow(clippy::cast_possible_truncation)]
                {
                    f as i64
                }
            });
            obj.insert("timeframeMs".into(), convex::Value::Int64(ms));
        }
    }
    convex::Value::Object(obj)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestrictionCreateInfo {
    pub id: Arc<str>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum RestrictionRemoveResult {
    NotFound,
    Ok,
}

impl Database {
    pub async fn restrictions_list_bans(&self) -> Result<Arc<[RestrictionRow]>, DatabaseError> {
        DatabaseRequest::named("restrictions:listBans")
            .query(self)
            .await
    }

    pub async fn restrictions_list_limits(&self) -> Result<Arc<[RestrictionRow]>, DatabaseError> {
        DatabaseRequest::named("restrictions:listLimits")
            .query(self)
            .await
    }

    /// Live subscription to all restriction rows (bans + limits). Consumed by
    /// central's in-memory mirror.
    pub async fn restrictions_watch_all(
        &self,
    ) -> Result<
        impl futures::stream::Stream<Item = Result<Arc<[RestrictionRow]>, ResponseError>>,
        DatabaseError,
    > {
        DatabaseRequest::named("restrictions:getAll")
            .watch_query(self)
            .await
    }

    pub async fn restriction_create(
        &self,
        user: Option<&AccountUserRef>,
        place: Option<&AccountPlaceRef>,
        rule: &Rule,
    ) -> Result<RestrictionCreateInfo, DatabaseError> {
        let mut req =
            DatabaseRequest::named("restrictions:create").with_arg("rule", rule_value(rule));
        if let Some(user) = user {
            req = req.with_arg("user", user_ref_value(user));
        }
        if let Some(place) = place {
            req = req.with_arg("place", place_ref_value(place));
        }
        req.mutate(self).await
    }

    /// Full-row replace (edit). `user`/`place` are cleared when `None`.
    pub async fn restriction_replace(
        &self,
        id: Arc<str>,
        user: Option<&AccountUserRef>,
        place: Option<&AccountPlaceRef>,
        rule: &Rule,
    ) -> Result<RestrictionRemoveResult, DatabaseError> {
        let mut req = DatabaseRequest::named("restrictions:replace")
            .with_arg("id", id.as_ref())
            .with_arg("rule", rule_value(rule));
        if let Some(user) = user {
            req = req.with_arg("user", user_ref_value(user));
        }
        if let Some(place) = place {
            req = req.with_arg("place", place_ref_value(place));
        }
        req.mutate(self).await
    }

    pub async fn restriction_remove(
        &self,
        id: Arc<str>,
    ) -> Result<RestrictionRemoveResult, DatabaseError> {
        DatabaseRequest::named("restrictions:remove")
            .with_arg("id", id.as_ref())
            .mutate(self)
            .await
    }
}
