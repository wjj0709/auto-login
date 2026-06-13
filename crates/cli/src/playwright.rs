use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::config::{AccountConfig, ProviderConfig};
use crate::log;

/// Playwright 子进程的入参账号
#[derive(Debug, Serialize)]
pub struct PlaywrightAccount {
    pub name: String,
    pub provider: String,
    pub domain: String,
    pub login_path: String,
    pub sign_in_path: Option<String>,
    pub user_info_path: String,
    pub api_user_key: String,
    pub api_user: String,
    pub cookies: serde_json::Value,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Playwright 子进程返回的账号结果
#[derive(Debug, Deserialize, Default, Clone)]
pub struct PlaywrightResult {
    pub name: String,
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub before: Option<QuotaInfo>,
    #[serde(default)]
    pub after: Option<QuotaInfo>,
    #[serde(default)]
    pub user_info: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub used_login: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct QuotaInfo {
    pub quota: f64,
    pub used_quota: f64,
}

#[derive(Debug, Deserialize)]
struct PlaywrightOutput {
    #[serde(default)]
    results: Vec<PlaywrightResult>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

fn build_account(account: &AccountConfig, provider: &ProviderConfig, index: usize) -> PlaywrightAccount {
    PlaywrightAccount {
        name: account.get_display_name(index),
        provider: account.provider.clone(),
        domain: provider.domain.clone(),
        login_path: provider.login_path.clone(),
        sign_in_path: provider.sign_in_path.clone(),
        user_info_path: provider.user_info_path.clone(),
        api_user_key: provider.api_user_key.clone(),
        api_user: account.api_user.clone(),
        cookies: account.cookies.clone(),
        username: account._username.clone()
            .or_else(|| account_field(&account.cookies, "_username"))
            .or_else(|| std::env::var(format!("ANYROUTER_USERNAME_{}", index + 1)).ok()),
        password: account._password.clone()
            .or_else(|| account_field(&account.cookies, "_password"))
            .or_else(|| std::env::var(format!("ANYROUTER_PASSWORD_{}", index + 1)).ok()),
    }
}

/// 兼容：允许账号 JSON 在 cookies 对象内偷偷夹带 _username / _password
fn account_field(cookies: &serde_json::Value, key: &str) -> Option<String> {
    cookies.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn locate_script() -> PathBuf {
    // 1) 显式覆盖
    if let Ok(p) = std::env::var("PLAYWRIGHT_SCRIPT") {
        return PathBuf::from(p);
    }

    let rel = PathBuf::from("scripts/playwright_checkin.py");

    // 2) 编译时项目根目录（CARGO_MANIFEST_DIR）— 最可靠
    let manifest_candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(&rel);
    if manifest_candidate.is_file() {
        return manifest_candidate;
    }

    // 3) 当前可执行文件所在目录及其父目录
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors().take(5) {
            let candidate = ancestor.join(&rel);
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    // 4) 当前工作目录（最后兜底，跟历史行为一致）
    rel
}

fn locate_python() -> String {
    std::env::var("PYTHON_BIN").unwrap_or_else(|_| {
        if cfg!(windows) {
            "python".to_string()
        } else {
            "python3".to_string()
        }
    })
}

fn headless_flag() -> bool {
    !matches!(
        std::env::var("PLAYWRIGHT_HEADLESS").ok().as_deref(),
        Some("0") | Some("false") | Some("False")
    )
}

/// 调用 Playwright 子进程，对所有账号一次性完成签到流程
pub async fn run_checkin(
    accounts: &[AccountConfig],
    providers: &std::collections::HashMap<String, ProviderConfig>,
) -> Result<Vec<PlaywrightResult>, String> {
    let start = Instant::now();
    let mut payload_accounts: Vec<PlaywrightAccount> = Vec::with_capacity(accounts.len());
    for (i, account) in accounts.iter().enumerate() {
        let provider = match providers.get(&account.provider) {
            Some(p) => p,
            None => {
                log::warn(&format!(
                    "Provider \"{}\" not found, skipping account {}",
                    account.provider,
                    account.get_display_name(i)
                ));
                continue;
            }
        };
        payload_accounts.push(build_account(account, provider, i));
    }

    if payload_accounts.is_empty() {
        return Err("No valid accounts to process".to_string());
    }

    let payload = json!({
        "headless": headless_flag(),
        "timeout_ms": 30000,
        "accounts": payload_accounts,
    });
    let payload_str = serde_json::to_string(&payload).map_err(|e| format!("serialize payload: {}", e))?;

    let script = locate_script();
    let python = locate_python();

    if !script.is_file() {
        return Err(format!(
            "Playwright script not found: {} (cwd={:?}); set PLAYWRIGHT_SCRIPT to override",
            script.display(),
            std::env::current_dir().ok()
        ));
    }

    log::info(&format!(
        "Launching Playwright runner: {} {} (headless={}, accounts={})",
        python,
        script.display(),
        headless_flag(),
        payload_accounts.len()
    ));

    let mut child = Command::new(&python)
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            format!(
                "Failed to spawn python subprocess ({}): {}; set PYTHON_BIN to override",
                python, e
            )
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(payload_str.as_bytes())
            .await
            .map_err(|e| format!("Failed to write stdin: {}", e))?;
        stdin
            .shutdown()
            .await
            .map_err(|e| format!("Failed to close stdin: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("Failed to wait child: {}", e))?;

    let elapsed = start.elapsed();
    log::info(&format!(
        "Playwright runner finished in {:.1?}, exit_code={:?}, stdout={} bytes",
        elapsed,
        output.status.code(),
        output.stdout.len()
    ));

    if !output.status.success() {
        let stdout_text = String::from_utf8_lossy(&output.stdout).to_string();
        return Err(format!(
            "Playwright runner exited with code {:?}: {}",
            output.status.code(),
            stdout_text.chars().take(500).collect::<String>()
        ));
    }

    let parsed: PlaywrightOutput = serde_json::from_slice(&output.stdout).map_err(|e| {
        let snippet: String = String::from_utf8_lossy(&output.stdout)
            .chars()
            .take(500)
            .collect();
        format!("Failed to parse runner stdout JSON: {} | stdout: {}", e, snippet)
    })?;

    if let Some(err) = parsed.error {
        return Err(format!(
            "Playwright runner error: {} ({})",
            err,
            parsed.message.unwrap_or_default()
        ));
    }

    Ok(parsed.results)
}
