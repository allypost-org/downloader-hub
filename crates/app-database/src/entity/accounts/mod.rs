use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Telegram,
    Discord,
}

impl Platform {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Telegram => "telegram",
            Self::Discord => "discord",
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stored on each request: which end-user ordered it. `requester` (the bot's
/// authed id) stays untouched for traceability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct AccountUserRef {
    pub platform: Platform,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct AccountPlaceRef {
    pub platform: Platform,
    pub id: String,
}

/// Snapshot row in `downloader_hub_account_users`, kept fresh by bots via
/// `accounts:upsert`. Unique on `(platform, platformId)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUser {
    pub platform: Platform,
    pub platform_id: String,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub is_bot: Option<bool>,
    #[serde(with = "crate::helpers::serde::bigint")]
    pub last_seen: u64,
}

/// Snapshot row in `downloader_hub_account_places` (chat/channel/server).
/// Unique on `(platform, platformId)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountPlace {
    pub platform: Platform,
    pub platform_id: String,
    pub kind: Option<String>,
    pub name: Option<String>,
    pub username: Option<String>,
    pub parent_platform_id: Option<String>,
    #[serde(with = "crate::helpers::serde::bigint")]
    pub last_seen: u64,
}
