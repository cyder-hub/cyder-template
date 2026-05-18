use std::{fmt, num::ParseIntError, str::FromStr};

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ApiId(i64);

impl ApiId {
    pub fn new(id: i64) -> Self {
        Self(id)
    }

    pub fn into_i64(self) -> i64 {
        self.0
    }
}

impl From<i64> for ApiId {
    fn from(id: i64) -> Self {
        Self::new(id)
    }
}

impl From<ApiId> for i64 {
    fn from(id: ApiId) -> Self {
        id.into_i64()
    }
}

impl FromStr for ApiId {
    type Err = ParseIntError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        source.parse::<i64>().map(Self)
    }
}

impl fmt::Display for ApiId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Serialize for ApiId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for ApiId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(ApiIdVisitor)
    }
}

struct ApiIdVisitor;

impl Visitor<'_> for ApiIdVisitor {
    type Value = ApiId;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a signed 64-bit integer encoded as a string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        value.parse::<ApiId>().map_err(E::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_id_serializes_as_json_string() {
        let serialized =
            serde_json::to_string(&ApiId::new(9_007_199_254_740_993)).expect("id should serialize");

        assert_eq!(serialized, "\"9007199254740993\"");
    }

    #[test]
    fn api_id_deserializes_from_string() {
        let id: ApiId =
            serde_json::from_str("\"65819694067617792\"").expect("id string should deserialize");

        assert_eq!(id.into_i64(), 65_819_694_067_617_792);
    }
}
