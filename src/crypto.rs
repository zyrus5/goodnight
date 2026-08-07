use std::{collections::HashMap, env};

use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, OsRng, rand_core::RngCore},
};
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

#[derive(Clone)]
pub struct Keyring {
    primary: String,
    keys: HashMap<String, [u8; 32]>,
}

impl Keyring {
    pub fn from_env() -> Result<Self> {
        let raw = env::var("APP_ENCRYPTION_KEYS")
            .context("APP_ENCRYPTION_KEYS is required (version:base64-key,...) ")?;
        let mut keys = HashMap::new();
        let mut primary = None;
        for item in raw.split(',').map(str::trim).filter(|v| !v.is_empty()) {
            let (version, encoded) = item
                .split_once(':')
                .context("APP_ENCRYPTION_KEYS entry must be version:base64-key")?;
            let bytes = STANDARD
                .decode(encoded)
                .context("invalid encryption key base64")?;
            let key: [u8; 32] = bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("encryption key must be 32 bytes"))?;
            if primary.is_none() {
                primary = Some(version.to_owned());
            }
            keys.insert(version.to_owned(), key);
        }
        Ok(Self {
            primary: primary.context("APP_ENCRYPTION_KEYS is empty")?,
            keys,
        })
    }
    pub fn encrypt(&self, plaintext: &str) -> Result<Value> {
        let key = self.keys.get(&self.primary).expect("primary key exists");
        let cipher = Aes256Gcm::new_from_slice(key).expect("key length");
        let mut nonce = [0u8; 12];
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
            .map_err(|_| anyhow::anyhow!("encryption failed"))?;
        Ok(
            json!({"$encrypted":format!("{}:{}:{}",self.primary,STANDARD.encode(nonce),STANDARD.encode(ciphertext))}),
        )
    }
    pub fn decrypt(&self, value: &Value) -> Result<String> {
        let marker = value
            .get("$encrypted")
            .and_then(Value::as_str)
            .context("not an encrypted value")?;
        let mut parts = marker.splitn(3, ':');
        let version = parts.next().unwrap_or_default();
        let nonce = STANDARD.decode(parts.next().unwrap_or_default())?;
        let ciphertext = STANDARD.decode(parts.next().unwrap_or_default())?;
        if nonce.len() != 12 {
            bail!("invalid encrypted nonce")
        }
        let key = self
            .keys
            .get(version)
            .context("encryption key version is unavailable")?;
        let cipher = Aes256Gcm::new_from_slice(key).expect("key length");
        let plain = cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
            .map_err(|_| anyhow::anyhow!("decryption failed"))?;
        String::from_utf8(plain).context("decrypted value is not UTF-8")
    }
    pub fn encrypt_parameters(&self, value: &Value, password_names: &[String]) -> Result<Value> {
        let mut value = value.clone();
        if let Some(map) = value.as_object_mut() {
            for name in password_names {
                if let Some(v) = map.get_mut(name)
                    && let Some(plain) = v.as_str()
                {
                    *v = self.encrypt(plain)?;
                }
            }
        }
        Ok(value)
    }
    pub fn decrypt_parameters(&self, value: &Value) -> Result<Value> {
        let mut value = value.clone();
        if let Some(map) = value.as_object_mut() {
            for v in map.values_mut() {
                if v.get("$encrypted").is_some() {
                    *v = Value::String(self.decrypt(v)?);
                }
            }
        }
        Ok(value)
    }
}

pub fn mask_encrypted(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if map.contains_key("$encrypted") {
                *value = json!({"password_set":true})
            } else {
                for v in map.values_mut() {
                    mask_encrypted(v)
                }
            }
        }
        Value::Array(items) => {
            for v in items {
                mask_encrypted(v)
            }
        }
        _ => {}
    }
}

pub fn password_names(definitions: &Value) -> Vec<String> {
    definitions
        .as_array()
        .into_iter()
        .flatten()
        .filter(|v| {
            v.get("type")
                .or_else(|| v.get("_class"))
                .and_then(Value::as_str)
                .is_some_and(|t| t.contains("Password"))
        })
        .filter_map(|v| v.get("name").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::Keyring;
    use std::collections::HashMap;
    #[test]
    fn roundtrip() {
        let k = Keyring {
            primary: "v1".into(),
            keys: HashMap::from([("v1".into(), [7; 32])]),
        };
        let encrypted = k.encrypt("secret").unwrap();
        assert_ne!(encrypted.to_string(), "secret");
        assert_eq!(k.decrypt(&encrypted).unwrap(), "secret");
    }
}
