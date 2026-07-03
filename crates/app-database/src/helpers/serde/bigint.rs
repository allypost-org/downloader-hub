use serde::{Deserialize, Deserializer, Serializer};

pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    let str = String::deserialize(deserializer)?;

    str.parse().map_err(D::Error::custom)
}

pub fn serialize<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&value.to_string())
}

pub mod option {
    use serde::{Deserializer, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Some(super::deserialize(deserializer)?))
    }

    pub fn serialize<S>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(v) => super::serialize(v, serializer),
            None => serializer.serialize_none(),
        }
    }
}
