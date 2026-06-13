use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;

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
    pub tokens_path: String,
    pub logs_path: String,
    pub chart_path: String,
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
    #[allow(dead_code)]
    pub user_info: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub used_login: bool,
    /// fetch_detail 回传:令牌 / 使用日志 / 消耗图表的原始 JSON(其余 action 为 null)
    #[serde(default)]
    pub tokens: serde_json::Value,
    #[serde(default)]
    pub logs: serde_json::Value,
    #[serde(default)]
    pub chart: serde_json::Value,
    /// 任务结束时站点域名下的全部 cookie(Phase 3 起回传,用于落库与续期)
    #[serde(default)]
    pub cookies: Vec<CookieEntry>,
}

/// Playwright 回传的单条 cookie(对齐 `context.cookies()`)。
#[derive(Debug, Deserialize, Clone)]
pub struct CookieEntry {
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default = "default_cookie_path")]
    pub path: String,
    /// Unix 秒;-1 表示会话期 cookie
    #[serde(default = "default_cookie_expires")]
    pub expires: f64,
    #[serde(default, rename = "httpOnly")]
    pub http_only: bool,
    #[serde(default)]
    pub secure: bool,
}

fn default_cookie_path() -> String {
    "/".to_string()
}
fn default_cookie_expires() -> f64 {
    -1.0
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
        username: account_field(&account.cookies, "_username")
            .or_else(|| std::env::var(format!("ANYROUTER_USERNAME_{}", index + 1)).ok()),
        password: account_field(&account.cookies, "_password")
            .or_else(|| std::env::var(format!("ANYROUTER_PASSWORD_{}", index + 1)).ok()),
        tokens_path: provider.tokens_path.clone(),
        logs_path: provider.logs_path.clone(),
        chart_path: provider.chart_path.clone(),
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
    std::env::var("PYTHON_BIN").unwrap_or_else(|_| "python3".to_string())
}

fn headless_flag() -> bool {
    !matches!(
        std::env::var("PLAYWRIGHT_HEADLESS").ok().as_deref(),
        Some("0") | Some("false") | Some("False")
    )
}

/// 调用 Playwright 子进程执行签到(所有账号一次性完成)。
pub async fn run_checkin(
    accounts: &[AccountConfig],
    providers: &std::collections::HashMap<String, ProviderConfig>,
) -> Result<Vec<PlaywrightResult>, String> {
    run_action("checkin", accounts, providers).await
}

/// 调用 Playwright 子进程执行账密登录(获取 / 刷新 Cookie)。
pub async fn run_login(
    accounts: &[AccountConfig],
    providers: &std::collections::HashMap<String, ProviderConfig>,
) -> Result<Vec<PlaywrightResult>, String> {
    run_action("login", accounts, providers).await
}

/// 调用 Playwright 子进程拉取账户详情(tokens / logs / chart)。
pub async fn run_fetch_detail(
    accounts: &[AccountConfig],
    providers: &std::collections::HashMap<String, ProviderConfig>,
) -> Result<Vec<PlaywrightResult>, String> {
    run_action("fetch_detail", accounts, providers).await
}

/// 通用子进程调用。`action` 透传给 Python 决定任务类型(checkin / login / fetch_detail)。
async fn run_action(
    action: &str,
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
        "action": action,
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

    // 关键:GPUI 的执行器不是 Tokio runtime。绝不能在此 await tokio::process 的子进程 I/O
    // ——其 Windows 实现的 poll_write 会调用 spawn_blocking → Handle::current(),
    // 在没有 Tokio reactor 的前台任务里直接 panic("there is no reactor running"),
    // 且因发生在不可 unwind 的窗口过程回调中,会升级为 STATUS_STACK_BUFFER_OVERRUN 崩溃。
    //
    // 改为:用 std::process 同步驱动子进程,放到独立 OS 线程执行(避免阻塞 UI 线程),
    // 再用「runtime 无关」的 oneshot 把结果桥接回当前 GPUI 任务(oneshot 的 await 不依赖
    // 任何 runtime,由 GPUI 自己的执行器驱动)。
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(run_subprocess_blocking(python, script, payload_str, start));
    });

    rx.await
        .map_err(|_| "Playwright runner thread terminated unexpectedly".to_string())?
}

/// 同步驱动 Playwright 子进程并收集结果。**必须在独立线程中调用**(不依赖 Tokio runtime)。
///
/// 协议:子脚本 `main()` 先 `sys.stdin.read()` 读完整 stdin、再向 stdout 写结果
/// (见 `scripts/playwright_checkin.py`),因此「写完 stdin → drop 关闭 → 读 stdout」的
/// 顺序不会触发管道死锁;stderr 透传到终端(非管道),也不会占满缓冲区。
fn run_subprocess_blocking(
    python: String,
    script: PathBuf,
    payload_str: String,
    start: Instant,
) -> Result<Vec<PlaywrightResult>, String> {
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
            .map_err(|e| format!("Failed to write stdin: {}", e))?;
        // 离开作用域时 drop(stdin) 关闭管道 → 子进程读到 EOF
    }

    let output = child
        .wait_with_output()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    /// 极简的「非 Tokio」执行器:在当前线程把 future 驱动到完成,全程没有任何 Tokio runtime。
    /// 这复刻了 GPUI 前台执行器的环境——正是旧 `tokio::process` 代码 panic 的场景。
    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        struct ThreadWaker(std::thread::Thread);
        impl Wake for ThreadWaker {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
            fn wake_by_ref(self: &Arc<Self>) {
                self.0.unpark();
            }
        }

        let waker: Waker = Arc::new(ThreadWaker(std::thread::current())).into();
        let mut cx = Context::from_waker(&waker);
        let mut fut = std::pin::pin!(fut);
        loop {
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(out) => return out,
                Poll::Pending => std::thread::park(),
            }
        }
    }

    /// 回归保护:把阻塞工作丢到 OS 线程、再用 oneshot 桥接回来,在没有 Tokio runtime 的
    /// 执行器上 await 必须正常完成。旧实现在这一步会 panic("no reactor running")。
    #[test]
    fn bridge_completes_without_tokio_runtime() {
        let got = block_on(async {
            let (tx, rx) = tokio::sync::oneshot::channel::<i32>();
            std::thread::spawn(move || {
                let _ = tx.send(42);
            });
            rx.await.unwrap()
        });
        assert_eq!(got, 42);
    }

    /// 探测可用的 Python 解释器(优先 `PYTHON_BIN`/python3,Windows 回退 python)。
    fn python_available() -> Option<String> {
        for cand in [locate_python(), "python".to_string()] {
            let ok = Command::new(&cand)
                .arg("--version")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return Some(cand);
            }
        }
        None
    }

    /// 端到端跑通签到主链路(`run_checkin` → std::process + 线程 + oneshot),全程无 Tokio runtime。
    /// 用临时桩脚本回写合法 JSON,无需 playwright/chromium;没有 Python 则跳过(不误报)。
    #[test]
    fn run_action_end_to_end_under_non_tokio_executor() {
        let Some(python) = python_available() else {
            eprintln!("skip: 未找到 python/python3,跳过端到端子进程测试");
            return;
        };

        let script = std::env::temp_dir().join("anyrouter_stub_runner.py");
        std::fs::write(
            &script,
            "import sys, json\n\
             req = json.loads(sys.stdin.read())\n\
             results = [{\"name\": a[\"name\"], \"success\": True} for a in req[\"accounts\"]]\n\
             print(json.dumps({\"results\": results}))\n",
        )
        .expect("write stub script");

        std::env::set_var("PYTHON_BIN", &python);
        std::env::set_var("PLAYWRIGHT_SCRIPT", &script);

        let account = AccountConfig {
            cookies: serde_json::json!({ "session": "x" }),
            api_user: "u1".to_string(),
            provider: "anyrouter".to_string(),
            name: Some("7".to_string()),
        };
        let mut providers: HashMap<String, ProviderConfig> = HashMap::new();
        providers.insert(
            "anyrouter".to_string(),
            ProviderConfig {
                name: "anyrouter".to_string(),
                domain: "https://example.com".to_string(),
                login_path: "/login".to_string(),
                sign_in_path: Some("/api/user/sign_in".to_string()),
                user_info_path: "/api/user/self".to_string(),
                api_user_key: "new-api-user".to_string(),
                tokens_path: "/api/token/".to_string(),
                logs_path: "/api/log/self".to_string(),
                chart_path: "/api/data/self".to_string(),
            },
        );

        let results = block_on(run_checkin(&[account], &providers))
            .expect("run_checkin 应在非 Tokio 执行器上成功完成");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "7");
        assert!(results[0].success);

        let _ = std::fs::remove_file(&script);
    }
}
