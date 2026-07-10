use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize<S>(value: &size::Size, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_i64(value.bytes())
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<size::Size, D::Error>
where
    D: Deserializer<'de>,
{
    i64::deserialize(deserializer).map(size::Size::from_bytes)
}

pub mod option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(value: &Option<size::Size>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Option::<i64>::serialize(&value.as_ref().map(size::Size::bytes), serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<size::Size>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<i64>::deserialize(deserializer).map(|opt| opt.map(size::Size::from_bytes))
    }
}
