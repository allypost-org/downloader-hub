use std::sync::Arc;

use serde::{Deserialize, Serializer, de::Deserializer};

pub fn deserialize<'de, D>(deserializer: D) -> Result<Arc<[u8]>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    let str = String::deserialize(deserializer)?;

    data_encoding::BASE64
        .decode(str.as_bytes())
        .map(Arc::from)
        .map_err(D::Error::custom)
}

pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let str = data_encoding::BASE64.encode(bytes);
    serializer.serialize_str(&str)
}
