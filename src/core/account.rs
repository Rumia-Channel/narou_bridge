use crate::core::atomic_io::atomic_write;
use crate::core::model::{AccountFile, AccountRecord};
use anyhow::{Result, anyhow};
use serde::Deserialize;
use serde::de::{self, Deserializer};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

impl<'de> Deserialize<'de> for AccountFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        account_file_from_value(value).map_err(de::Error::custom)
    }
}

pub fn load_account_file(path: &Path) -> Result<AccountFile> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn validate_account_file(_site: &str, _account: &AccountFile) -> Result<()> {
    Ok(())
}

pub fn rewrite_cookie_site_mirror(
    cookie_root: &Path,
    site: &str,
    accounts: &[AccountRecord],
) -> Result<()> {
    let dir = cookie_root.join(site);
    fs::create_dir_all(&dir)?;
    let mut desired_files = BTreeMap::new();
    for account in accounts {
        let json = serde_json::to_vec_pretty(&account.account)?;
        desired_files.insert(account_file_name(&account.name), json.clone());
        if account.active {
            desired_files.insert("login.json".to_string(), json);
        }
    }

    let existing_json_files = fs::read_dir(&dir)?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            (entry.file_type().ok()?.is_file()
                && path.extension().and_then(|ext| ext.to_str()) == Some("json"))
            .then(|| entry.file_name().to_string_lossy().to_string())
        })
        .collect::<HashSet<_>>();

    for (file_name, payload) in &desired_files {
        atomic_write(&dir.join(file_name), payload.as_slice())?;
    }

    let desired_names = desired_files.keys().cloned().collect::<HashSet<_>>();
    for stale_file in existing_json_files.difference(&desired_names) {
        fs::remove_file(dir.join(stale_file))?;
    }

    Ok(())
}

fn account_file_from_value(value: Value) -> Result<AccountFile> {
    match value {
        Value::Object(map) => {
            let has_account_fields = map.contains_key("cookies")
                || map.contains_key("user_agent")
                || map.contains_key("ua")
                || map.contains_key("display_name");
            if has_account_fields {
                Ok(AccountFile {
                    cookies: normalize_cookies(map.get("cookies").cloned().unwrap_or(Value::Null))?,
                    user_agent: optional_string(map.get("user_agent").or_else(|| map.get("ua"))),
                    display_name: optional_string(map.get("display_name")),
                })
            } else {
                Ok(AccountFile {
                    cookies: normalize_cookies(Value::Object(map))?,
                    user_agent: None,
                    display_name: None,
                })
            }
        }
        Value::Array(items) => Ok(AccountFile {
            cookies: normalize_cookies(Value::Array(items))?,
            user_agent: None,
            display_name: None,
        }),
        other => Err(anyhow!(
            "unsupported account JSON root: {}",
            json_type_name(&other)
        )),
    }
}

fn normalize_cookies(value: Value) -> Result<Value> {
    match value {
        Value::Null => Ok(Value::Object(Map::new())),
        Value::Object(map) => Ok(Value::Object(
            map.into_iter()
                .filter_map(|(name, value)| cookie_value(value).map(|value| (name, value)))
                .collect(),
        )),
        Value::Array(items) => Ok(Value::Object(
            items
                .into_iter()
                .filter_map(|item| match item {
                    Value::Object(mut cookie) => {
                        let name = cookie
                            .remove("name")
                            .and_then(|value| optional_string(Some(&value)))?;
                        let value = cookie.remove("value").and_then(cookie_value)?;
                        Some((name, value))
                    }
                    _ => None,
                })
                .collect(),
        )),
        _ => Err(anyhow!("cookies must be an object or list")),
    }
}

fn optional_string(value: Option<&Value>) -> Option<String> {
    value.and_then(|value| match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    })
}

fn cookie_value(value: Value) -> Option<Value> {
    match value {
        Value::String(_) => Some(value),
        Value::Number(number) => Some(Value::String(number.to_string())),
        Value::Bool(flag) => Some(Value::String(flag.to_string())),
        _ => None,
    }
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn account_file_name(name: &str) -> String {
    format!("{name}.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deserializes_login_helper_account_payload() {
        let account: AccountFile = serde_json::from_str(
            r#"{
              "cookies": {
                "PHPSESSID": "12345_abcdefXYZ",
                "device_token": "token"
              },
              "user_agent": "helper-ua",
              "display_name": "helper-user"
            }"#,
        )
        .unwrap();

        assert_eq!(
            account.cookies.get("PHPSESSID").and_then(Value::as_str),
            Some("12345_abcdefXYZ")
        );
        assert_eq!(account.user_agent.as_deref(), Some("helper-ua"));
        assert_eq!(account.display_name.as_deref(), Some("helper-user"));
    }

    #[test]
    fn deserializes_cookie_list_and_ua_alias() {
        let account: AccountFile = serde_json::from_str(
            r#"{
              "cookies": [
                {"name": "PHPSESSID", "value": "12345_abcdefXYZ"},
                {"name": "device_token", "value": "token"}
              ],
              "ua": "legacy-ua"
            }"#,
        )
        .unwrap();

        assert_eq!(
            account.cookies,
            json!({
                "PHPSESSID": "12345_abcdefXYZ",
                "device_token": "token"
            })
        );
        assert_eq!(account.user_agent.as_deref(), Some("legacy-ua"));
        assert_eq!(account.display_name, None);
    }
}
