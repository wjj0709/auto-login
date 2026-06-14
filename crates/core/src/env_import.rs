use anyhow::{Context, Result};
use serde_json::Value;

use crate::models::{AccountInput, SiteInput};
use crate::storage::Storage;

/// 首次启动时从环境变量导入配置到 SQLite
///
/// 检查数据库中的 `env_imported` 标志，如果已存在则跳过。
/// 否则插入内置站点并从 `ANYROUTER_ACCOUNTS` 环境变量读取账户列表。
pub fn import_env_if_needed(storage: &Storage) -> Result<()> {
    // 1. 检查是否已导入
    if storage.get_meta("env_imported")?.is_some() {
        return Ok(());
    }

    // 2. 插入内置站点
    let anyrouter_id = storage.insert_site(&SiteInput {
        name: "AnyRouter".to_string(),
        domain: "https://anyrouter.top".to_string(),
        login_path: "/login".to_string(),
        sign_in_path: Some("/api/user/sign_in".to_string()),
        user_info_path: "/api/user/self".to_string(),
        tokens_path: "/api/token/".to_string(),
        logs_path: "/api/log/self".to_string(),
        chart_path: "/api/data/self".to_string(),
        api_user_key: "new-api-user".to_string(),
    })?;

    let agentrouter_id = storage.insert_site(&SiteInput {
        name: "AgentRouter".to_string(),
        domain: "https://agentrouter.org".to_string(),
        login_path: "/login".to_string(),
        sign_in_path: None,
        user_info_path: "/api/user/self".to_string(),
        tokens_path: "/api/token/".to_string(),
        logs_path: "/api/log/self".to_string(),
        chart_path: "/api/data/self".to_string(),
        api_user_key: "new-api-user".to_string(),
    })?;

    // 3. 读取环境变量 ANYROUTER_ACCOUNTS
    let accounts_json = match std::env::var("ANYROUTER_ACCOUNTS") {
        Ok(val) if !val.is_empty() => val,
        _ => {
            // 无账户配置，标记已导入后返回
            storage.set_meta("env_imported", "true")?;
            return Ok(());
        }
    };

    // 4. 解析 JSON 数组
    let accounts: Vec<Value> = serde_json::from_str(&accounts_json)
        .context("Failed to parse ANYROUTER_ACCOUNTS as JSON array")?;

    // 5. 遍历并插入账户
    for (i, entry) in accounts.iter().enumerate() {
        let provider = entry
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let site_id = if provider.to_lowercase() == "agentrouter" {
            agentrouter_id
        } else {
            anyrouter_id
        };

        let name = entry
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let name = if name.is_empty() {
            format!("Account {}", i + 1)
        } else {
            name.to_string()
        };

        let api_user = entry
            .get("api_user")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let username = entry
            .get("_username")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let password = entry
            .get("_password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let cookies = entry.get("cookies").and_then(|v| {
            if v.is_null() {
                None
            } else {
                let s = serde_json::to_string(v).ok()?;
                if s == "\"\"" || s == "[]" || s == "null" {
                    None
                } else {
                    Some(s)
                }
            }
        });

        let input = AccountInput {
            site_id,
            name,
            api_user,
            username,
            password,
            cookies,
        };

        storage.insert_account(&input)?;
    }

    // 6. 标记已导入
    storage.set_meta("env_imported", "true")?;

    Ok(())
}
