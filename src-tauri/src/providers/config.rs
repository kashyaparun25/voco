use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub api_key: Option<String>, // Prefixed with "enc:" if encrypted in database
    pub api_url: Option<String>,
    pub provider_type: String, // e.g. "openai", "groq", "nvidia", "ollama", "custom"
    #[serde(default)]
    pub default_model: String,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub is_default: bool,
}

impl ProviderConfig {
    /// Encrypts the api_key if it is not already encrypted
    pub fn encrypt_key(&mut self) {
        if let Some(ref key) = self.api_key {
            if !key.starts_with("enc:") && !key.trim().is_empty() {
                let enc = encrypt_api_key(key);
                self.api_key = Some(format!("enc:{}", enc));
            }
        }
    }

    /// Decrypts the api_key if it is currently encrypted
    pub fn decrypt_key(&mut self) {
        if let Some(ref key) = self.api_key {
            if key.starts_with("enc:") {
                let clean_key = &key[4..];
                if let Ok(dec) = decrypt_api_key(clean_key) {
                    self.api_key = Some(dec);
                }
            }
        }
    }

    /// Normalizes `api_url` to a clean BASE url. Users often paste a full
    /// endpoint (e.g. `…/v1/audio/transcriptions`); the clients append their
    /// own paths, so strip known endpoint suffixes and trailing slashes.
    pub fn normalize_url(&mut self) {
        if let Some(url) = &self.api_url {
            self.api_url = Some(normalize_base_url(url));
        }
    }
}

/// Strip known endpoint suffixes + trailing slashes from a pasted URL so the
/// stored value is always a base URL the clients can safely append to.
pub fn normalize_base_url(url: &str) -> String {
    let mut u = url.trim().trim_end_matches('/').to_string();
    const SUFFIXES: &[&str] = &[
        "/audio/transcriptions",
        "/audio/translations",
        "/chat/completions",
        "/completions",
        "/embeddings",
        "/models",
        "/api/generate",
        "/api/chat",
        "/api/tags",
    ];
    let mut changed = true;
    while changed {
        changed = false;
        for s in SUFFIXES {
            if u.ends_with(s) {
                u.truncate(u.len() - s.len());
                u = u.trim_end_matches('/').to_string();
                changed = true;
            }
        }
    }
    u
}

// Basic Encryption Helpers
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("Invalid hex string length".to_string());
    }
    let mut bytes = Vec::with_capacity(s.len() / 2);
    for i in (0..s.len()).step_by(2) {
        let byte_str = &s[i..i+2];
        let byte = u8::from_str_radix(byte_str, 16).map_err(|e| e.to_string())?;
        bytes.push(byte);
    }
    Ok(bytes)
}

pub fn encrypt_api_key(key: &str) -> String {
    let xor_key = b"voco-secret-key-1337-encrypt-providers";
    let encrypted: Vec<u8> = key
        .as_bytes()
        .iter()
        .enumerate()
        .map(|(i, &b)| b ^ xor_key[i % xor_key.len()])
        .collect();
    hex_encode(&encrypted)
}

pub fn decrypt_api_key(encrypted_hex: &str) -> Result<String, String> {
    let encrypted = hex_decode(encrypted_hex)?;
    let xor_key = b"voco-secret-key-1337-encrypt-providers";
    let decrypted: Vec<u8> = encrypted
        .iter()
        .enumerate()
        .map(|(i, &b)| b ^ xor_key[i % xor_key.len()])
        .collect();
    String::from_utf8(decrypted).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::normalize_base_url;

    #[test]
    fn strips_full_endpoints_to_base() {
        assert_eq!(
            normalize_base_url("https://api.groq.com/openai/v1/audio/transcriptions"),
            "https://api.groq.com/openai/v1"
        );
        assert_eq!(
            normalize_base_url("https://api.openai.com/v1/chat/completions"),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            normalize_base_url("http://localhost:11434/api/generate"),
            "http://localhost:11434"
        );
        // Already a base URL → unchanged (minus trailing slash).
        assert_eq!(
            normalize_base_url("https://api.groq.com/openai/v1/"),
            "https://api.groq.com/openai/v1"
        );
        assert_eq!(normalize_base_url("http://localhost:1234/v1"), "http://localhost:1234/v1");
    }
}
