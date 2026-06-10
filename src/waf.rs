use std::collections::HashMap;
use std::time::Instant;
use reqwest::Client;

use crate::log;

/// 通过 Node.js 解决 WAF JS challenge (acw_sc__v2)
/// 返回计算出的 WAF cookies
pub async fn solve_waf_challenge(domain: &str, label: &str) -> Option<HashMap<String, String>> {
    let start = Instant::now();
    log::info_f(label, &format!("Fetching WAF challenge from: {}", domain));

    // 1. 获取 challenge 页面
    let client = match Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::error_f(label, &format!("Failed to create HTTP client: {}", e));
            return None;
        }
    };

    let challenge_url = format!("{}/api/user/sign_in", domain);
    let html = match client.post(&challenge_url)
        .header("Content-Type", "application/json")
        .send().await
    {
        Ok(resp) => match resp.text().await {
            Ok(text) => text,
            Err(e) => {
                log::error_f(label, &format!("Failed to read response: {}", e));
                return None;
            }
        },
        Err(e) => {
            log::error_f(label, &format!("Failed to fetch challenge: {}", e));
            return None;
        }
    };

    // 2. 检查是否为 WAF challenge 页面
    if !html.contains("acw_sc__v2") && !html.contains("arg1") {
        log::info_f(label, "Response does not appear to be a WAF challenge page");
        return Some(HashMap::new());
    }

    // 3. 提取 <script> 内容
    let script_start = match html.find("<script>") {
        Some(pos) => pos + 8,
        None => {
            log::error_f(label, "No <script> tag found in challenge HTML");
            return None;
        }
    };
    let script_end = match html.find("</script>") {
        Some(pos) => pos,
        None => {
            log::error_f(label, "No </script> tag found in challenge HTML");
            return None;
        }
    };
    let original_js = &html[script_start..script_end];
    log::debug_f(label, &format!("Extracted JS: {} chars", original_js.len()));

    // 4. 构建 Node.js 脚本：stub DOM + 原始 JS + 输出 cookie
    let node_script = build_node_script(original_js);
    log::debug_f(label, &format!("Node script: {} chars", node_script.len()));

    // 5. 用 Node.js 执行
    log::info_f(label, "Executing WAF challenge JS in Node.js...");
    let cookie_value = match execute_node_js(&node_script, label).await {
        Some(val) => val,
        None => {
            log::error_f(label, "Failed to execute WAF challenge JS");
            return None;
        }
    };

    if cookie_value.is_empty() {
        log::error_f(label, "WAF challenge produced empty cookie value");
        return None;
    }

    log::success_f(label, &format!("WAF challenge solved! acw_sc__v2={}... ({} chars) in {:.1?}",
        &cookie_value[..cookie_value.len().min(16)],
        cookie_value.len(),
        start.elapsed()));

    let mut cookies = HashMap::new();
    cookies.insert("acw_sc__v2".to_string(), cookie_value);
    Some(cookies)
}

/// 构建 Node.js 脚本：stub DOM API 并捕获 cookie 值
fn build_node_script(original_js: &str) -> String {
    let mut script = String::new();

    // DOM stub：捕获 acw_sc__v2 cookie
    script.push_str(r#"
var __acw_result = '';
var document = {};
Object.defineProperty(document, 'cookie', {
  set: function(val) {
    var parts = String(val).split(';')[0].split('=');
    if (parts[0] === 'acw_sc__v2') { __acw_result = parts[1]; }
  },
  get: function() { return ''; },
  configurable: true
});
document.location = { href: 'https://anyrouter.top/', reload: function(){} };
var window = globalThis;
var navigator = { userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36' };
"#);

    // 添加原始 JS
    script.push_str(original_js);

    // 输出 cookie 值
    script.push_str("\n;process.stdout.write(__acw_result);\n");

    script
}

/// 用 Node.js 执行 JS 并提取 acw_sc__v2 cookie 值
async fn execute_node_js(js_code: &str, label: &str) -> Option<String> {
    use tokio::process::Command;

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        Command::new("node")
            .arg("-e")
            .arg(js_code)
            .output()
    ).await;

    match result {
        Ok(Ok(output)) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                log::debug_f(label, &format!("Node.js stdout: {} chars, value: {}...",
                    stdout.len(),
                    &stdout[..stdout.len().min(40)]));
                if stdout.is_empty() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    log::error_f(label, &format!("Node.js returned empty result. stderr: {}", stderr));
                    None
                } else {
                    Some(stdout)
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                log::error_f(label, &format!("Node.js exited with code {:?}: {}",
                    output.status.code(), stderr));
                None
            }
        }
        Ok(Err(e)) => {
            log::error_f(label, &format!("Failed to run Node.js: {}", e));
            None
        }
        Err(_) => {
            log::error_f(label, "Node.js execution timed out (30s)");
            None
        }
    }
}
