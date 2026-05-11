use crate::core::model::{AccountFile, AccountRecord};
use anyhow::{Result, anyhow};
use regex::Regex;
use serde::Deserialize;
use serde::de::{self, Deserializer};
use serde_json::{Map, Value};
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

pub fn validate_account_file(site: &str, account: &AccountFile) -> Result<()> {
    if site != "pixiv" {
        return Ok(());
    }

    let phpsessid = account
        .cookies
        .as_object()
        .and_then(|cookies| cookies.get("PHPSESSID"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("必須クッキー (PHPSESSID) がありません"))?;

    let re = Regex::new(r"^\d+_[A-Za-z0-9]+$").expect("valid pixiv PHPSESSID regex");
    if !re.is_match(phpsessid) {
        return Err(anyhow!("PHPSESSID が不正な形式です"));
    }

    Ok(())
}

pub fn rewrite_cookie_site_mirror(
    cookie_root: &Path,
    site: &str,
    accounts: &[AccountRecord],
) -> Result<()> {
    let site_dir = cookie_root.join(site);
    fs::create_dir_all(&site_dir)?;

    let mut expected_files: std::collections::HashSet<String> = std::collections::HashSet::new();

    for account in accounts {
        let file_name = format!("{}.json", account.name);
        expected_files.insert(file_name.clone());

        let account_json = serde_json::json!({
            "cookies": account.account.cookies,
            "user_agent": account.account.user_agent,
            "display_name": account.account.display_name,
        });
        let content = serde_json::to_string_pretty(&account_json)?;
        let path = site_dir.join(&file_name);
        fs::write(&path, &content)?;

        if account.active {
            let login_path = site_dir.join("login.json");
            expected_files.insert("login.json".to_string());
            fs::write(&login_path, &content)?;
        }
    }

    let entries = fs::read_dir(&site_dir)?;
    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy().to_string();
        if name.ends_with(".json") && !expected_files.contains(&name) {
            fs::remove_file(entry.path())?;
        }
    }

    Ok(())
}

pub fn write_login_account_mirror(
    cookie_root: &Path,
    site: &str,
    account: &AccountFile,
) -> Result<()> {
    let site_dir = cookie_root.join(site);
    fs::create_dir_all(&site_dir)?;

    let account_json = serde_json::json!({
        "cookies": account.cookies,
        "user_agent": account.user_agent,
        "display_name": account.display_name,
    });
    let content = serde_json::to_string_pretty(&account_json)?;
    let login_path = site_dir.join("login.json");
    fs::write(&login_path, content)?;

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

    #[test]
    fn validates_pixiv_cookie_successfully() {
        let account = AccountFile {
            cookies: json!({"PHPSESSID": "12345_abcdefXYZ"}),
            user_agent: Some("ua".to_string()),
            display_name: Some("pixiv".to_string()),
        };

        assert!(validate_account_file("pixiv", &account).is_ok());
    }

    #[test]
    fn rejects_missing_pixiv_phpsessid() {
        let account = AccountFile {
            cookies: json!({"device_token": "token"}),
            user_agent: None,
            display_name: None,
        };

        let err = validate_account_file("pixiv", &account).unwrap_err();
        assert_eq!(err.to_string(), "必須クッキー (PHPSESSID) がありません");
    }

    #[test]
    fn rejects_invalid_pixiv_phpsessid_format() {
        let account = AccountFile {
            cookies: json!({"PHPSESSID": "invalid"}),
            user_agent: None,
            display_name: None,
        };

        let err = validate_account_file("pixiv", &account).unwrap_err();
        assert_eq!(err.to_string(), "PHPSESSID が不正な形式です");
    }
}
