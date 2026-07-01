use std::str::FromStr;

use clap::Args;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Default, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = Some("Action entries"))]
pub struct DisabledEntriesConfig {
    #[clap(
        long = "disable-action-entry",
        value_delimiter = ',',
        env = "DOWNLOADER_HUB_DISABLED_ACTION_ENTRIES"
    )]
    pub entries: Vec<DisableEntry>,
}

type Category = String;
type Name = String;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub struct DisableEntry {
    pub category: Category,
    pub name: Name,
}

impl DisableEntry {
    #[must_use]
    #[inline]
    pub fn new<C, N>(category: C, name: N) -> Self
    where
        C: Into<Category>,
        N: Into<Name>,
    {
        Self {
            category: category.into().to_lowercase(),
            name: name.into().to_lowercase(),
        }
    }
}

impl<C, N> From<(C, N)> for DisableEntry
where
    C: Into<Category>,
    N: Into<Name>,
{
    fn from(value: (C, N)) -> Self {
        Self::new(value.0, value.1)
    }
}

impl FromStr for DisableEntry {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (category, name) = s.split_once(':').ok_or_else(|| {
            format!(
                "Invalid disable entry. Expected `$CATEGORY:$NAME`, got {:?}",
                s
            )
        })?;

        Ok(Self::new(category, name))
    }
}
