use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use anyhow::{Context, Result};
use rand::RngCore;

const SERVICE_NAME: &str = "anyrouter-checkin";
const KEY_NAME: &str = "master-key";

/// AES-256-GCM 加密器，主密钥由系统 keyring 管理
pub struct Crypto {
    cipher: Aes256Gcm,
}

impl Crypto {
    /// 加载或创建主密钥，初始化 cipher
    pub fn new() -> Result<Self> {
        let key_bytes = Self::load_or_create_key()?;
        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        Ok(Self { cipher })
    }

    /// 从 keyring 加载密钥，不存在则随机生成并存入
    fn load_or_create_key() -> Result<Vec<u8>> {
        let entry = keyring::Entry::new(SERVICE_NAME, KEY_NAME)
            .context("Failed to create keyring entry")?;

        // 尝试从 keyring 读取已有密钥
        match entry.get_password() {
            Ok(hex_key) => {
                let key = hex::decode(&hex_key)
                    .context("Failed to decode master key from keyring")?;
                if key.len() != 32 {
                    anyhow::bail!(
                        "Invalid master key length in keyring: expected 32 bytes, got {}",
                        key.len()
                    );
                }
                Ok(key)
            }
            Err(keyring::Error::NoEntry) => {
                // 生成新的 256-bit 随机密钥
                let mut key = vec![0u8; 32];
                rand::thread_rng().fill_bytes(&mut key);
                let hex_key = hex::encode(&key);
                entry
                    .set_password(&hex_key)
                    .context("Failed to store master key in keyring")?;
                Ok(key)
            }
            Err(e) => {
                Err(anyhow::anyhow!("Failed to access keyring: {}", e))
            }
        }
    }

    /// 加密：返回 nonce(12) || ciphertext || tag(16)
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

        // 拼接：nonce(12) || ciphertext+tag
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    /// 解密：输入格式为 nonce(12) || ciphertext || tag(16)
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 12 + 16 {
            anyhow::bail!(
                "Ciphertext too short: expected at least 28 bytes, got {}",
                data.len()
            );
        }

        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

        Ok(plaintext)
    }

    /// 便捷方法：加密字符串
    pub fn encrypt_string(&self, plaintext: &str) -> Result<Vec<u8>> {
        self.encrypt(plaintext.as_bytes())
    }

    /// 便捷方法：解密为字符串
    pub fn decrypt_string(&self, data: &[u8]) -> Result<String> {
        let plaintext = self.decrypt(data)?;
        String::from_utf8(plaintext).context("Decrypted data is not valid UTF-8")
    }
}
