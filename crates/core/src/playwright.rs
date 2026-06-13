use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Playwright 执行的动作类型
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaywrightAction {
    Checkin,
    Login,
    #[serde(rename = "fetch_detail")]
    FetchDetail,
}

/// 传给 Playwright 脚本的完整载荷
#[derive(Debug, Serialize)]
pub struct PlaywrightPayload {
    pub action: PlaywrightAction,
    pub headless: bool,
    pub timeout_ms: u64,
    pub accounts: Vec<PlaywrightAccountInput>,
}

/// 单个账户的输入信息
#[derive(Debug, Serialize)]
pub struct PlaywrightAccountInput {
    pub name: String,
    pub provider: String,
    pub domain: String,
    pub login_path: String,
    pub sign_in_path: Option<String>,
    pub user_info_path: String,
    pub tokens_path: Option<String>,
    pub logs_path: Option<String>,
    pub chart_path: Option<String>,
    pub api_user_key: String,
    pub api_user: String,
    pub cookies: serde_json::Value,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Playwright 脚本的 stdout 输出
#[derive(Debug, Deserialize)]
pub struct PlaywrightOutput {
    #[serde(default)]
    pub results: Vec<PlaywrightResult>,
    #[serde(default)]
    pub error: Option<String>,
}

/// 单个账户的执行结果
#[derive(Debug, Deserialize, Clone)]
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
    pub tokens: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub logs: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub chart: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub used_login: bool,
    #[serde(default)]
    pub cookies: Vec<CookieInfo>,
}

/// 额度信息
#[derive(Debug, Deserialize, Clone)]
pub struct QuotaInfo {
    pub quota: f64,
    pub used_quota: f64,
}

/// Cookie 信息
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CookieInfo {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub expires: Option<f64>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

/// 定位 Python 脚本路径
///
/// 搜索顺序：
/// 1. 环境变量 `PLAYWRIGHT_SCRIPT`
/// 2. 从当前可执行文件向上搜索 `scripts/playwright_checkin.py`
/// 3. 回退到相对路径 `scripts/playwright_checkin.py`
fn locate_script() -> PathBuf {
    // 1. 环境变量
    if let Ok(path) = std::env::var("PLAYWRIGHT_SCRIPT") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return p;
        }
    }

    // 2. 从当前可执行文件向上搜索
    if let Ok(exe_path) = std::env::current_exe() {
        let mut dir = exe_path.parent().map(|p| p.to_path_buf());
        while let Some(d) = dir {
            let candidate = d.join("scripts").join("playwright_checkin.py");
            if candidate.exists() {
                return candidate;
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }

    // 3. 回退到相对路径
    PathBuf::from("scripts/playwright_checkin.py")
}

/// 定位 Python 命令（Windows 用 "python"，其他用 "python3"）
fn locate_python() -> String {
    if cfg!(target_os = "windows") {
        "python".to_string()
    } else {
        "python3".to_string()
    }
}

/// 执行 Playwright 子进程
///
/// 流程：
/// 1. 序列化 payload 为 JSON
/// 2. 定位脚本和 python 命令
/// 3. spawn 子进程（stdin/stdout/stderr piped）
/// 4. 写入 payload 到 stdin 并关闭
/// 5. 等待完成，检查 exit code
/// 6. 解析 stdout JSON
/// 7. 检查 error 字段
pub async fn run_playwright(payload: &PlaywrightPayload) -> Result<PlaywrightOutput> {
    // 1. 序列化 payload
    let payload_json =
        serde_json::to_string(payload).context("Failed to serialize playwright payload")?;

    // 2. 定位脚本和 python 命令
    let script_path = locate_script();
    let python_cmd = locate_python();

    // 3. spawn 子进程
    let mut child = Command::new(&python_cmd)
        .arg(&script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "Failed to spawn playwright process: {} {}",
                python_cmd,
                script_path.display()
            )
        })?;

    // 4. 写入 payload 到 stdin 并关闭
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(payload_json.as_bytes())
            .await
            .context("Failed to write payload to playwright stdin")?;
        // stdin 在 drop 时自动关闭
    }

    // 5. 等待完成
    let output = child
        .wait_with_output()
        .await
        .context("Failed to wait for playwright process")?;

    // 检查 exit code
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        anyhow::bail!(
            "Playwright process exited with code {:?}\nstderr: {}\nstdout: {}",
            output.status.code(),
            stderr.trim(),
            stdout.trim()
        );
    }

    // 6. 解析 stdout JSON
    let stdout_str = String::from_utf8(output.stdout)
        .context("Playwright stdout is not valid UTF-8")?;

    let pw_output: PlaywrightOutput = serde_json::from_str(&stdout_str).with_context(|| {
        format!(
            "Failed to parse playwright JSON output: {}",
            &stdout_str[..stdout_str.len().min(200)]
        )
    })?;

    // 7. 检查 error 字段
    if let Some(ref err) = pw_output.error {
        anyhow::bail!("Playwright script error: {}", err);
    }

    Ok(pw_output)
}
