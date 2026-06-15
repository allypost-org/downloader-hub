#[derive(::serde::Deserialize)]
#[serde(untagged)]
enum StringOrU64 {
    Str(String),
    U64(u64),
}

pub mod serde_maybe {
    use ::serde::{Deserialize, Deserializer, de::Error};
    use serde::Serialize;

    use super::StringOrU64;

    type Value = Option<u64>;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        let Some(s): Option<StringOrU64> = Option::deserialize(deserializer)? else {
            return Ok(None);
        };

        let res = match s {
            StringOrU64::Str(s) => {
                parse_size::parse_size(s).map_err(|e| Error::custom(e.to_string()))?
            }
            StringOrU64::U64(x) => x,
        };

        Ok(Some(res))
    }

    pub fn serialize<S>(val: &Value, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        val.serialize(serializer)
    }
}

pub mod serde {
    use ::serde::{Deserialize, Deserializer, de::Error};
    use serde::Serialize;

    use super::StringOrU64;

    type Value = u64;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = StringOrU64::deserialize(deserializer)?;

        let res = match s {
            StringOrU64::Str(s) => {
                parse_size::parse_size(s).map_err(|e| Error::custom(e.to_string()))?
            }
            StringOrU64::U64(x) => x,
        };

        Ok(res)
    }

    pub fn serialize<S>(val: &Value, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        val.serialize(serializer)
    }
}

#[must_use]
pub fn parse_file_size_static(s: &'static str) -> u64 {
    try_parse_file_size(s).expect("Failed to parse file size")
}

pub fn try_parse_file_size(s: &str) -> Result<u64, parse_size::Error> {
    parse_size::parse_size(s)
}
