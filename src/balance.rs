// ============================================================================
// balance.rs — 余额哈希检测模块
// ============================================================================
// 功能：通过 SHA-256 哈希检测各账号的余额是否发生变化
// 用途：仅在余额变化时触发通知，避免重复打扰
//
// 原理：
// 1. 每次签到后，将所有账号的余额数据序列化为 JSON
// 2. 对 JSON 字符串计算 SHA-256 哈希，取前 16 位 hex 作为指纹
// 3. 与上次保存的哈希比对，不同则表示余额有变化
// 4. 将当前哈希保存到文件，供下次比对
//
// 使用 BTreeMap 保证键的顺序一致，确保相同数据生成相同哈希
// ============================================================================

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;

use crate::log;

/// 余额哈希文件的存储路径（当前工作目录下）
const BALANCE_HASH_FILE: &str = "balance_hash.txt";

/// 从文件加载上次保存的余额哈希值
///
/// 读取当前工作目录下的 `balance_hash.txt` 文件
///
/// # 返回
/// - Some(String): 成功读取到的哈希值
/// - None: 文件不存在、为空或读取失败（首次运行时为 None）
pub fn load_balance_hash() -> Option<String> {
    log::debug(&format!(
        "Reading balance hash from file: {}",
        BALANCE_HASH_FILE
    ));
    match fs::read_to_string(BALANCE_HASH_FILE) {
        Ok(content) => {
            let hash = content.trim().to_string();
            if hash.is_empty() {
                // 文件存在但为空
                log::info("Balance hash file exists but is empty (no previous data)");
                None
            } else {
                // 成功读取，只显示首尾 4 位避免泄露完整哈希
                log::info(&format!(
                    "Previous balance hash loaded: {}...{}",
                    &hash[..4.min(hash.len())],
                    &hash[hash.len().saturating_sub(4)..]
                ));
                Some(hash)
            }
        }
        Err(_) => {
            // 文件不存在（首次运行或文件被删除）
            log::info("No previous balance hash file found (first run or file deleted)");
            None
        }
    }
}

/// 将当前余额哈希值保存到文件
///
/// 写入当前工作目录下的 `balance_hash.txt` 文件
/// 下次运行时可通过 load_balance_hash() 读取并比对
pub fn save_balance_hash(balance_hash: &str) {
    log::debug(&format!(
        "Saving balance hash to file: {}",
        BALANCE_HASH_FILE
    ));
    match fs::write(BALANCE_HASH_FILE, balance_hash) {
        Ok(_) => log::success(&format!(
            "Balance hash saved: {}...{}",
            &balance_hash[..4.min(balance_hash.len())],
            &balance_hash[balance_hash.len().saturating_sub(4)..]
        )),
        Err(e) => log::warn(&format!("Failed to save balance hash: {}", e)),
    }
}

/// 根据所有账号的余额数据生成哈希值
///
/// 使用 BTreeMap 对键排序以保证相同数据始终产生相同哈希
/// 算法：SHA-256 → 取前 16 位 hex 字符
///
/// # 参数
/// - balances: 账号标识 → 当前余额 的映射
///   例如 {"account_1": 5.10, "account_2": 3.20}
///
/// # 返回
/// 16 位 hex 字符串哈希值，例如 "a1b2c3d4e5f67890"
pub fn generate_balance_hash(balances: &HashMap<String, f64>) -> String {
    // 使用 BTreeMap 保证键的排序一致，HashMap 的迭代顺序不确定
    let sorted: std::collections::BTreeMap<&String, &f64> = balances.iter().collect();
    // 序列化为 JSON 字符串（键已排序）
    let json = serde_json::to_string(&sorted).unwrap_or_default();
    log::debug(&format!("Balance hash input JSON: {}", json));

    // 计算 SHA-256 哈希
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let result = hasher.finalize();

    // 转为十六进制字符串，取前 16 位
    let hex = hex_encode(&result);
    let hash = hex[..16].to_string();
    log::info(&format!(
        "Balance hash generated: {}...{} (from {} account(s))",
        &hash[..4],
        &hash[12..],
        balances.len()
    ));
    hash
}

/// 将字节数组编码为十六进制字符串
///
/// 例如 [0xAB, 0xCD] → "abcd"
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
