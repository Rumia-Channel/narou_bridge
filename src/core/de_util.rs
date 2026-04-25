//! Lenient serde deserializers used when reading legacy Python-era JSON payloads.
//!
//! Real production data stores numeric-looking fields (such as work id, author id, and
//! episode id) as JSON numbers in some sites and as JSON strings in others. The Rust
//! runtime normalises them into strings; these helpers smooth over the input variance.

use serde::Deserialize;
use serde::de::{Deserializer, Error};
use serde_json::Value;

pub fn de_string_or_num<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    match Value::deserialize(deserializer)? {
        Value::String(s) => Ok(s),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Null => Ok(String::new()),
        other => Err(D::Error::custom(format!(
            "expected string or number, got {other}"
        ))),
    }
}

pub fn de_opt_string_or_num<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    match Value::deserialize(deserializer)? {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s)),
        Value::Number(n) => Ok(Some(n.to_string())),
        Value::Bool(b) => Ok(Some(b.to_string())),
        other => Err(D::Error::custom(format!(
            "expected string, number or null, got {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Probe {
        #[serde(deserialize_with = "de_string_or_num")]
        id: String,
        #[serde(default, deserialize_with = "de_opt_string_or_num")]
        chapter: Option<String>,
    }

    #[test]
    fn accepts_integer_and_string() {
        let from_int: Probe = serde_json::from_str(r#"{"id": 1, "chapter": null}"#).unwrap();
        assert_eq!(from_int.id, "1");
        assert!(from_int.chapter.is_none());

        let from_str: Probe = serde_json::from_str(r#"{"id": "abc", "chapter": "2"}"#).unwrap();
        assert_eq!(from_str.id, "abc");
        assert_eq!(from_str.chapter.as_deref(), Some("2"));

        let opt_int: Probe = serde_json::from_str(r#"{"id": "abc", "chapter": 3}"#).unwrap();
        assert_eq!(opt_int.chapter.as_deref(), Some("3"));
    }
}
