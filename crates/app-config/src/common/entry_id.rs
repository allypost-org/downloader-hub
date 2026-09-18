use std::{str::FromStr, sync::Arc};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryCategory {
    Action,
    Downloader,
    Extractor,
    Fixer,
}

impl EntryCategory {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Action => "action",
            Self::Downloader => "downloader",
            Self::Extractor => "extractor",
            Self::Fixer => "fixer",
        }
    }
}

impl FromStr for EntryCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "action" => Ok(Self::Action),
            "downloader" => Ok(Self::Downloader),
            "extractor" => Ok(Self::Extractor),
            "fixer" => Ok(Self::Fixer),
            other => Err(format!(
                "Unknown entry category {other:?}. Expected one of: action, downloader, \
                 extractor, fixer"
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub struct EntryId {
    pub category: EntryCategory,
    pub name: String,
}

impl EntryId {
    #[must_use]
    pub fn new(category: EntryCategory, name: impl AsRef<str>) -> Self {
        Self {
            category,
            name: name.as_ref().to_lowercase(),
        }
    }
}

impl FromStr for EntryId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (category, name) = s
            .split_once(':')
            .ok_or_else(|| format!("Invalid entry id. Expected `$CATEGORY:$NAME`, got {s:?}"))?;

        Ok(Self::new(category.parse()?, name))
    }
}

pub trait AsEntryId {
    fn entry_id(&self) -> EntryId;
}

impl AsEntryId for EntryId {
    fn entry_id(&self) -> EntryId {
        self.clone()
    }
}

impl<T> AsEntryId for &T
where
    T: AsEntryId + ?Sized,
{
    fn entry_id(&self) -> EntryId {
        (**self).entry_id()
    }
}

impl<T> AsEntryId for Arc<T>
where
    T: AsEntryId + ?Sized,
{
    fn entry_id(&self) -> EntryId {
        (**self).entry_id()
    }
}

impl<T> AsEntryId for Box<T>
where
    T: AsEntryId + ?Sized,
{
    fn entry_id(&self) -> EntryId {
        (**self).entry_id()
    }
}
