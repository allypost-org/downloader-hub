use serde::{Deserialize, Deserializer, Serializer};

pub mod timestamp {
    use super::{Deserialize, Deserializer, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<jiff::Timestamp, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let s = String::deserialize(deserializer)?;
        let ms: i64 = s.parse().map_err(D::Error::custom)?;
        jiff::Timestamp::from_millisecond(ms).map_err(D::Error::custom)
    }

    pub fn serialize<S>(value: &jiff::Timestamp, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.as_millisecond().to_string())
    }

    pub mod option {
        use super::{Deserializer, Serializer};

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<jiff::Timestamp>, D::Error>
        where
            D: Deserializer<'de>,
        {
            Ok(Some(super::deserialize(deserializer)?))
        }

        pub fn serialize<S>(
            value: &Option<jiff::Timestamp>,
            serializer: S,
        ) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            match value {
                Some(v) => super::serialize(v, serializer),
                None => serializer.serialize_none(),
            }
        }
    }
}

pub mod span {
    use super::{Deserialize, Deserializer, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<jiff::Span, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let s = String::deserialize(deserializer)?;
        let ms: i64 = s.parse().map_err(D::Error::custom)?;
        jiff::Span::new()
            .try_milliseconds(ms)
            .map_err(D::Error::custom)
    }

    pub fn serialize<S>(value: &jiff::Span, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::Error;
        let ms = value
            .total(jiff::Unit::Millisecond)
            .map_err(S::Error::custom)?;
        serializer.serialize_str(&format!("{ms:.0}"))
    }
}
