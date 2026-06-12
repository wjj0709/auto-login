//! 敏感字段加密:主密钥存系统凭据库(Windows 凭据管理器),
//! 值用 AES-256-GCM 加密,存储格式为 nonce(12B) ‖ ciphertext。

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::{anyhow, Context, Result};
use base64::Engine;

#[allow(dead_code)] // Task 3+ storage.rs 接入后使用
const KEYRING_SERVICE: &str = "anyrouter-checkin";
#[allow(dead_code)] // Task 3+ storage.rs 接入后使用
const KEYRING_USER: &str = "master-key";
const NONCE_LEN: usize = 12;

pub struct Crypto {
    cipher: Aes256Gcm,
}

impl Crypto {
    /// 用给定的 32 字节密钥构造(测试与 keyring 加载共用)。
    pub fn from_key(key: &[u8; 32]) -> Self {
        Self {
            cipher: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key)),
        }
    }

    /// 从系统凭据库加载主密钥;不存在则生成 32 字节随机密钥并写入。
    #[allow(dead_code)] // Task 3+ storage.rs 接入后使用(生产入口)
    pub fn from_keyring() -> Result<Self> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .context("无法访问系统凭据库")?;
        let key: [u8; 32] = match entry.get_password() {
            Ok(b64) => base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .ok()
                .and_then(|raw| raw.try_into().ok())
                .ok_or_else(|| anyhow!("系统凭据库中的主密钥格式异常"))?,
            Err(keyring::Error::NoEntry) => {
                let mut key = [0u8; 32];
                use rand::RngCore;
                rand::rngs::OsRng.fill_bytes(&mut key);
                entry
                    .set_password(&base64::engine::general_purpose::STANDARD.encode(key))
                    .context("无法写入系统凭据库")?;
                key
            }
            Err(e) => return Err(anyhow!(e)).context("读取系统凭据库失败"),
        };
        Ok(Self::from_key(&key))
    }

    /// 加密文本,输出 nonce ‖ ciphertext。
    pub fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ct = self
            .cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|e| anyhow!("加密失败: {e}"))?;
        let mut out = nonce.to_vec();
        out.extend_from_slice(&ct);
        Ok(out)
    }

    /// 解密 nonce ‖ ciphertext 格式的数据。
    pub fn decrypt(&self, blob: &[u8]) -> Result<String> {
        if blob.len() <= NONCE_LEN {
            return Err(anyhow!("密文长度异常"));
        }
        let (nonce, ct) = blob.split_at(NONCE_LEN);
        let pt = self
            .cipher
            .decrypt(Nonce::from_slice(nonce), ct)
            .map_err(|e| anyhow!("解密失败: {e}"))?;
        String::from_utf8(pt).context("解密结果不是合法 UTF-8")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: [u8; 32] = [7u8; 32];

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let c = Crypto::from_key(&TEST_KEY);
        let blob = c.encrypt("session=abc; 中文✓").unwrap();
        assert_ne!(blob, b"session=abc");
        assert_eq!(c.decrypt(&blob).unwrap(), "session=abc; 中文✓");
    }

    #[test]
    fn nonce_is_random_each_time() {
        let c = Crypto::from_key(&TEST_KEY);
        assert_ne!(c.encrypt("x").unwrap(), c.encrypt("x").unwrap());
    }

    #[test]
    fn decrypt_garbage_fails() {
        let c = Crypto::from_key(&TEST_KEY);
        assert!(c.decrypt(b"short").is_err());
        assert!(c.decrypt(&[0u8; 40]).is_err());
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let blob = Crypto::from_key(&TEST_KEY).encrypt("secret").unwrap();
        assert!(Crypto::from_key(&[8u8; 32]).decrypt(&blob).is_err());
    }
}
