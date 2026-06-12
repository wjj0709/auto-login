//! 余额哈希检测：记录上次余额快照以判断是否变化（用于决定是否推送通知）。
//! 该子系统尚未接入 GPUI 流程，暂以 `#![allow(dead_code)]` 整体保留，待后续集成。
#![allow(dead_code)]

use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::fs;

use crate::log;

const BALANCE_HASH_FILE: &str = "balance_hash.txt";

/// 加载余额 hash
pub fn load_balance_hash() -> Option<String> {
    log::debug(&format!("Reading balance hash from file: {}", BALANCE_HASH_FILE));
    match fs::read_to_string(BALANCE_HASH_FILE) {
        Ok(content) => {
            let hash = content.trim().to_string();
            if hash.is_empty() {
                log::info("Balance hash file exists but is empty (no previous data)");
                None
            } else {
                log::info(&format!("Previous balance hash loaded: {}...{}", &hash[..4.min(hash.len())], &hash[hash.len().saturating_sub(4)..]));
                Some(hash)
            }
        }
        Err(_) => {
            log::info("No previous balance hash file found (first run or file deleted)");
            None
        }
    }
}

/// 保存余额 hash
pub fn save_balance_hash(balance_hash: &str) {
    log::debug(&format!("Saving balance hash to file: {}", BALANCE_HASH_FILE));
    match fs::write(BALANCE_HASH_FILE, balance_hash) {
        Ok(_) => log::success(&format!("Balance hash saved: {}...{}", &balance_hash[..4.min(balance_hash.len())], &balance_hash[balance_hash.len().saturating_sub(4)..])),
        Err(e) => log::warn(&format!("Failed to save balance hash: {}", e)),
    }
}

/// 生成余额数据的 hash
pub fn generate_balance_hash(balances: &HashMap<String, f64>) -> String {
    // 使用 BTreeMap 保证排序一致
    let sorted: std::collections::BTreeMap<&String, &f64> = balances.iter().collect();
    let json = serde_json::to_string(&sorted).unwrap_or_default();
    log::debug(&format!("Balance hash input JSON: {}", json));
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let result = hasher.finalize();
    let hex = hex_encode(&result);
    let hash = hex[..16].to_string();
    log::info(&format!("Balance hash generated: {}...{} (from {} account(s))", &hash[..4], &hash[12..], balances.len()));
    hash
}

/// 十六进制编码
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
