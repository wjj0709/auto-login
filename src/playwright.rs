// ============================================================================
// playwright.rs — Playwright 子进程调用模块
// ============================================================================
// 功能：通过启动 Python 子进程（scripts/playwright_checkin.py）执行浏览器自动化签到
// 通信协议：
//   Rust → Python: 通过 stdin 写入 JSON {headless, timeout_ms, accounts: [...]}
//   Python → Rust: 通过 stdout 返回 JSON {results: [{name, success, before, after, cookie_report?, ...}]}
//   Python 日志: 通过 stderr 输出到 Rust 主进程终端
// ============================================================================

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::config::{AccountConfig, ProviderConfig, SsoEmailConfig};
use crate::log;

const DEFAULT_CONFIG_FILE: &str = "conf.json";

/// 传入 Python 子进程的账号数据结构
/// 是 AccountConfig 与 ProviderConfig 的合并，包含签到所需的全部信息
#[derive(Debug, Serialize)]
pub struct PlaywrightAccount {
    /// 账号显示名称
    name: String,
    /// 所属 Provider 名称（如 "anyrouter"、"agentrouter"）
    provider: String,
    /// 站点域名（如 "https://anyrouter.top"）
    domain: String,
    /// 登录页路径（如 "/login"）
    login_path: String,
    /// 签到 API 路径，None 表示自动签到
    sign_in_path: Option<String>,
    /// 用户信息 API 路径（如 "/api/user/self"）
    user_info_path: String,
    /// 用户标识请求头键名（如 "new-api-user"）
    api_user_key: String,
    /// 用户标识值（对应 api_user_key 请求头）
    api_user: String,
    /// Cookie 数据（JSON 对象或字符串格式）
    cookies: serde_json::Value,
    /// 登录用户名（可选，Cookie 失效时回退登录用）
    username: Option<String>,
    /// 登录密码（可选，Cookie 失效时回退登录用）
    password: Option<String>,
    /// SSO 平台名称（github / linuxdo）
    sso_provider: Option<String>,
    /// SSO 平台用户名或邮箱
    sso_username: Option<String>,
    /// SSO 平台密码
    sso_password: Option<String>,
    /// 读取设备验证码的邮箱 IMAP 配置（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    sso_email: Option<SsoEmailConfig>,
}

impl_ref_accessors!(PlaywrightAccount {
    name: String => name, set_name;
    provider: String => provider, set_provider;
    domain: String => domain, set_domain;
    login_path: String => login_path, set_login_path;
    sign_in_path: Option<String> => sign_in_path, set_sign_in_path;
    user_info_path: String => user_info_path, set_user_info_path;
    api_user_key: String => api_user_key, set_api_user_key;
    api_user: String => api_user, set_api_user;
    cookies: serde_json::Value => cookies, set_cookies;
    username: Option<String> => username, set_username;
    password: Option<String> => password, set_password;
    sso_provider: Option<String> => sso_provider, set_sso_provider;
    sso_username: Option<String> => sso_username, set_sso_username;
    sso_password: Option<String> => sso_password, set_sso_password;
});

/// Cookie 过期分析结果
#[derive(Debug, Deserialize, Default, Clone)]
pub struct CookieReport {
    /// 登录请求中携带的 Cookie 名称
    #[serde(default)]
    request_cookie_names: Vec<String>,
    /// 登录响应中 Set-Cookie 下发的 Cookie 名称
    #[serde(default)]
    response_cookie_names: Vec<String>,
    /// 参与分析的 Cookie 详情
    #[serde(default)]
    cookies: Vec<CookieReportEntry>,
    /// 额外说明
    #[serde(default)]
    note: Option<String>,
}

impl_ref_accessors!(CookieReport {
    request_cookie_names: Vec<String> => request_cookie_names, set_request_cookie_names;
    response_cookie_names: Vec<String> => response_cookie_names, set_response_cookie_names;
    cookies: Vec<CookieReportEntry> => cookies, set_cookies;
    note: Option<String> => note, set_note;
});

/// 单个 Cookie 的分析详情
#[derive(Debug, Deserialize, Default, Clone)]
pub struct CookieReportEntry {
    name: String,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
    #[serde(default)]
    expires_in: Option<String>,
    #[serde(default)]
    max_age_seconds: Option<i64>,
    #[serde(default)]
    same_site: Option<String>,
    #[serde(default)]
    secure: bool,
    #[serde(default)]
    http_only: bool,
    #[serde(default)]
    is_session: bool,
}

impl_ref_accessors!(CookieReportEntry {
    name: String => name, set_name;
    domain: Option<String> => domain, set_domain;
    path: Option<String> => path, set_path;
    source: Option<String> => source, set_source;
    expires_at: Option<String> => expires_at, set_expires_at;
    expires_in: Option<String> => expires_in, set_expires_in;
    max_age_seconds: Option<i64> => max_age_seconds, set_max_age_seconds;
    same_site: Option<String> => same_site, set_same_site;
});

impl_copy_accessors!(CookieReportEntry {
    secure: bool => secure, set_secure;
    http_only: bool => http_only, set_http_only;
    is_session: bool => is_session, set_is_session;
});

/// Python 子进程返回的单个账号签到结果
#[derive(Debug, Deserialize, Default, Clone)]
pub struct PlaywrightResult {
    /// 账号名称
    name: String,
    /// 签到是否成功
    #[serde(default)]
    success: bool,
    /// 签到前余额信息
    #[serde(default)]
    before: Option<QuotaInfo>,
    /// 签到后余额信息
    #[serde(default)]
    after: Option<QuotaInfo>,
    /// 用户信息（从 /api/user/self 获取）
    #[serde(default)]
    user_info: Option<serde_json::Value>,
    /// 错误信息（签到失败时）
    #[serde(default)]
    error: Option<String>,
    /// 是否使用了账密登录（Cookie 失效时回退）
    #[serde(default)]
    used_login: bool,
    /// 登录后拿到的 Cookie 信息及过期时间分析
    #[serde(default)]
    cookie_report: Option<CookieReport>,
}

impl_ref_accessors!(PlaywrightResult {
    name: String => name, set_name;
    before: Option<QuotaInfo> => before, set_before;
    after: Option<QuotaInfo> => after, set_after;
    user_info: Option<serde_json::Value> => user_info, set_user_info;
    error: Option<String> => error, set_error;
    cookie_report: Option<CookieReport> => cookie_report, set_cookie_report;
});

impl_copy_accessors!(PlaywrightResult {
    success: bool => success, set_success;
    used_login: bool => used_login, set_used_login;
});

/// 余额信息（从 Python 子进程返回的 before/after 字段）
#[derive(Debug, Deserialize, Clone)]
pub struct QuotaInfo {
    /// 当前余额（美元单位，已由 Python 端原始值/500000 转换）
    quota: f64,
    /// 累计消耗（美元单位）
    used_quota: f64,
}

impl_copy_accessors!(QuotaInfo {
    quota: f64 => quota, set_quota;
    used_quota: f64 => used_quota, set_used_quota;
});

/// Python 子进程的完整输出结构
#[derive(Debug, Deserialize)]
struct PlaywrightOutput {
    /// 各账号的签到结果列表
    #[serde(default)]
    results: Vec<PlaywrightResult>,
    /// 整体错误信息（如有）
    #[serde(default)]
    error: Option<String>,
    /// 错误描述消息（如有）
    #[serde(default)]
    message: Option<String>,
}

/// 将 AccountConfig + ProviderConfig 合并为 PlaywrightAccount
/// 提取所有子进程需要的信息，包括显式账密字段和兼容旧版 cookies._username/_password
fn build_account(
    account: &AccountConfig,
    provider: &ProviderConfig,
    index: usize,
) -> PlaywrightAccount {
    PlaywrightAccount {
        name: account.get_display_name(index),
        provider: account.provider().clone(),
        domain: provider.domain().clone(),
        login_path: provider.login_path().clone(),
        sign_in_path: provider.sign_in_path().clone(),
        user_info_path: provider.user_info_path().clone(),
        api_user_key: provider.api_user_key().clone(),
        api_user: account.api_user().clone(),
        cookies: account.cookies().clone(),
        // 优先使用显式字段 username/password，兼容旧版 cookies._username/_password，
        // 最后回退读取环境变量 ANYROUTER_USERNAME_N / ANYROUTER_PASSWORD_N
        username: account
            .resolved_username()
            .or_else(|| trimmed_env_var(&format!("ANYROUTER_USERNAME_{}", index + 1))),
        password: account
            .resolved_password()
            .or_else(|| trimmed_env_var(&format!("ANYROUTER_PASSWORD_{}", index + 1))),
        sso_provider: account
            .resolved_sso_provider()
            .or_else(|| trimmed_env_var(&format!("ANYROUTER_SSO_PROVIDER_{}", index + 1))),
        sso_username: account
            .resolved_sso_username()
            .or_else(|| trimmed_env_var(&format!("ANYROUTER_SSO_USERNAME_{}", index + 1))),
        sso_password: account
            .resolved_sso_password()
            .or_else(|| trimmed_env_var(&format!("ANYROUTER_SSO_PASSWORD_{}", index + 1))),
        sso_email: account.sso_email().clone(),
    }
}

fn trimmed_env_var(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// 定位 Playwright Python 签到脚本的路径
/// 按以下优先级查找：
/// 1. 环境变量 PLAYWRIGHT_SCRIPT 显式指定
/// 2. CARGO_MANIFEST_DIR（编译时项目根目录）下的 scripts/playwright_checkin.py（最可靠）
/// 3. 当前可执行文件所在目录及其祖先目录（适配部署场景）
/// 4. 当前工作目录下的 scripts/playwright_checkin.py（兜底）
fn locate_script() -> PathBuf {
    // 1) 运行时配置文件覆盖
    if let Some(runtime) = load_runtime_file_config() {
        if let Some(path) = runtime.playwright_script {
            let candidate = PathBuf::from(path);
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    // 2) 环境变量显式覆盖
    if let Ok(p) = std::env::var("PLAYWRIGHT_SCRIPT") {
        return PathBuf::from(p);
    }

    let rel = PathBuf::from("scripts/playwright_checkin.py");

    // 3) 编译时项目根目录（CARGO_MANIFEST_DIR）— cargo run/test 时最可靠
    //    这个路径在编译时就已确定，不依赖运行时的工作目录
    let manifest_candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(&rel);
    if manifest_candidate.is_file() {
        return manifest_candidate;
    }

    // 4) 当前可执行文件所在目录及其父目录
    //    适配编译后二进制文件部署到其他位置的场景
    //    最多向上查找 5 层目录
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors().take(5) {
            let candidate = ancestor.join(&rel);
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    // 5) 当前工作目录（最后兜底，跟历史行为一致）
    rel
}

/// 获取 Python 解释器路径
/// 优先使用 PYTHON_BIN 环境变量，默认使用 "python3"
fn locate_python() -> String {
    if let Some(runtime) = load_runtime_file_config() {
        if let Some(python_bin) = runtime.python_bin.filter(|value| !value.trim().is_empty()) {
            return python_bin;
        }
    }

    std::env::var("PYTHON_BIN").unwrap_or_else(|_| "python3".to_string())
}

/// 读取无头模式配置
/// PLAYWRIGHT_HEADLESS 设为 "0"、"false" 或 "False" 时为非无头模式（显示浏览器窗口）
/// 默认为 true（无头模式，不显示浏览器窗口）
fn headless_flag() -> bool {
    if let Some(runtime) = load_runtime_file_config() {
        if let Some(headless) = runtime.playwright_headless {
            return headless;
        }
    }

    !matches!(
        std::env::var("PLAYWRIGHT_HEADLESS").ok().as_deref(),
        Some("0") | Some("false") | Some("False")
    )
}

#[derive(Debug, Default, Deserialize)]
struct RuntimeFileConfig {
    #[serde(default)]
    python_bin: Option<String>,
    #[serde(default)]
    playwright_script: Option<String>,
    #[serde(default)]
    playwright_headless: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct RootRuntimeConfig {
    #[serde(default)]
    runtime: Option<RuntimeFileConfig>,
}

fn load_runtime_file_config() -> Option<RuntimeFileConfig> {
    let path = discover_config_file_path()?;
    let content = fs::read_to_string(path).ok()?;
    let parsed = parse_root_runtime_config(&content)?;
    parsed.runtime
}

fn parse_root_runtime_config(content: &str) -> Option<RootRuntimeConfig> {
    serde_json::from_str(content)
        .ok()
        .or_else(|| toml::from_str(content).ok())
}

fn discover_config_file_path() -> Option<PathBuf> {
    if let Ok(raw_path) = std::env::var("ANYROUTER_CONFIG_FILE") {
        let trimmed = raw_path.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    [
        DEFAULT_CONFIG_FILE,
        "anyrouter-config.json",
        "anyrouter-config.toml",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_runtime_config_from_json() {
        let parsed = parse_root_runtime_config(
            r#"{
                "runtime": {
                    "python_bin": "python3",
                    "playwright_script": "scripts/playwright_checkin.py",
                    "playwright_headless": false
                }
            }"#,
        )
        .unwrap();

        let runtime = parsed.runtime.unwrap();
        assert_eq!(runtime.python_bin.as_deref(), Some("python3"));
        assert_eq!(runtime.playwright_headless, Some(false));
    }

    #[test]
    fn parses_runtime_config_from_toml() {
        let parsed = parse_root_runtime_config(
            r#"
            [runtime]
            python_bin = ".venv/bin/python3"
            playwright_script = "scripts/playwright_checkin.py"
            playwright_headless = true
            "#,
        )
        .unwrap();

        let runtime = parsed.runtime.unwrap();
        assert_eq!(runtime.python_bin.as_deref(), Some(".venv/bin/python3"));
        assert_eq!(runtime.playwright_headless, Some(true));
    }
}

/// 调用 Playwright 子进程，对所有账号一次性完成签到流程
///
/// 流程：
/// 1. 将账号数据与 Provider 配置合并，构建 PlaywrightAccount 列表
/// 2. 组装 JSON payload（包含 headless、timeout_ms、accounts）
/// 3. 定位 Python 脚本和解释器
/// 4. 启动 Python 子进程，通过 stdin 传入 JSON
/// 5. 等待子进程完成，解析 stdout 中的 JSON 结果
///
/// # 参数
/// - accounts: 账号配置列表
/// - providers: Provider 配置映射
///
/// # 返回
/// - Ok(Vec<PlaywrightResult>): 各账号的签到结果
/// - Err(String): 错误信息
pub async fn run_checkin(
    accounts: &[AccountConfig],
    providers: &std::collections::HashMap<String, ProviderConfig>,
) -> Result<Vec<PlaywrightResult>, String> {
    let start = Instant::now();

    // 合并账号配置与 Provider 配置，构建 PlaywrightAccount 列表
    let mut payload_accounts: Vec<PlaywrightAccount> = Vec::with_capacity(accounts.len());
    for (i, account) in accounts.iter().enumerate() {
        // 根据账号的 provider 字段查找对应的 Provider 配置
        let provider = match providers.get(account.provider()) {
            Some(p) => p,
            None => {
                // Provider 不存在，跳过该账号
                log::warn(&format!(
                    "Provider \"{}\" not found, skipping account {}",
                    account.provider(),
                    account.get_display_name(i)
                ));
                continue;
            }
        };
        payload_accounts.push(build_account(account, provider, i));
    }

    // 没有有效账号可处理
    if payload_accounts.is_empty() {
        return Err("No valid accounts to process".to_string());
    }

    // 组装发送给 Python 子进程的 JSON payload
    let payload = json!({
        "headless": headless_flag(),    // 是否无头模式
        "timeout_ms": 30000,            // 超时时间（毫秒）
        "accounts": payload_accounts,   // 账号列表
    });
    // 序列化为 JSON 字符串
    let payload_str =
        serde_json::to_string(&payload).map_err(|e| format!("serialize payload: {}", e))?;

    // 定位脚本和 Python 解释器
    let script = locate_script();
    let python = locate_python();

    // 验证脚本文件存在
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

    // 启动 Python 子进程
    // stdin: piped — 用于传入 JSON payload
    // stdout: piped — 用于读取返回的 JSON 结果
    // stderr: inherit — Python 日志直接输出到终端（不经过 Rust）
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

    // 通过 stdin 将 JSON payload 写入子进程
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(payload_str.as_bytes())
            .await
            .map_err(|e| format!("Failed to write stdin: {}", e))?;
        // 关闭 stdin，通知 Python 子进程输入已结束
        stdin
            .shutdown()
            .await
            .map_err(|e| format!("Failed to close stdin: {}", e))?;
    }

    // 等待子进程执行完毕并获取输出
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

    // 检查子进程退出码
    if !output.status.success() {
        let stdout_text = String::from_utf8_lossy(&output.stdout).to_string();
        return Err(format!(
            "Playwright runner exited with code {:?}: {}",
            output.status.code(),
            // 截取前 500 字符避免过长的错误信息
            stdout_text.chars().take(500).collect::<String>()
        ));
    }

    // 解析 Python 子进程返回的 JSON 结果
    let parsed: PlaywrightOutput = serde_json::from_slice(&output.stdout).map_err(|e| {
        let snippet: String = String::from_utf8_lossy(&output.stdout)
            .chars()
            .take(500)
            .collect();
        format!(
            "Failed to parse runner stdout JSON: {} | stdout: {}",
            e, snippet
        )
    })?;

    // 检查整体错误
    if let Some(err) = parsed.error {
        return Err(format!(
            "Playwright runner error: {} ({})",
            err,
            parsed.message.unwrap_or_default()
        ));
    }

    Ok(parsed.results)
}
