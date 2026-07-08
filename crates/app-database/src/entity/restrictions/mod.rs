use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::entity::accounts::{AccountPlaceRef, AccountUserRef};

/// Discriminated union stored as the `rule` field of a restriction row.
/// Mirrors `requests.status` (`#[serde(tag = "Type")]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "Type", rename_all = "lowercase")]
pub enum Rule {
    Ban {
        reason: String,
        #[serde(
            with = "crate::helpers::serde::jiff::timestamp::option",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        ends_at: Option<jiff::Timestamp>,
    },
    #[serde(rename_all = "camelCase")]
    Limit {
        #[serde(with = "crate::helpers::serde::bigint")]
        count: u64,
        #[serde(rename = "timeframeMs", with = "crate::helpers::serde::jiff::span")]
        timeframe: jiff::Span,
    },
}

/// Row in `downloader_hub_restrictions`.
///
/// `user` and `place` are both optional: a row matches a request `(user, place)`
/// iff `(row.user == none || row.user == req.user) && (row.place == none || row.place == req.place)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestrictionRow {
    pub id: Arc<str>,
    pub user: Option<AccountUserRef>,
    pub place: Option<AccountPlaceRef>,
    pub rule: Rule,
}
