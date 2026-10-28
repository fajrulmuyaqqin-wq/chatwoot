use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};

pub const ENCRYPTION_PREFIX: &str = "enc:";

/// Derives a 32-byte AES-256 key from `secret_key_base` using SHA-256
fn derive_aes_key(secret_key_base: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(secret_key_base.as_bytes());
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

/// Encrypts sensitive text (e.g. WABA access token) using AES-256-GCM.
/// Returns formatted string `enc:<base64(12_byte_nonce + ciphertext_with_tag)>`.
pub fn encrypt_sensitive_token(plaintext: &str, secret_key_base: &str) -> Result<String, String> {
    if plaintext.is_empty() {
        return Ok(String::new());
    }

    let key_bytes = derive_aes_key(secret_key_base);
    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| format!("Failed to initialize AES-256-GCM cipher: {}", e))?;

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("AES-256-GCM encryption failure: {}", e))?;

    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(format!("{}{}", ENCRYPTION_PREFIX, BASE64.encode(combined)))
}

/// Decrypts a token encrypted with `encrypt_sensitive_token`.
/// If the token does not begin with `enc:`, it is returned as-is for backward compatibility.
pub fn decrypt_sensitive_token(token: &str, secret_key_base: &str) -> Result<String, String> {
    if !token.starts_with(ENCRYPTION_PREFIX) {
        return Ok(token.to_string());
    }

    let encoded = &token[ENCRYPTION_PREFIX.len()..];
    let raw_bytes = BASE64
        .decode(encoded)
        .map_err(|e| format!("Base64 decoding error: {}", e))?;

    if raw_bytes.len() < 12 {
        return Err("Encrypted payload too short for AES-GCM nonce".to_string());
    }

    let (nonce_bytes, ciphertext) = raw_bytes.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key_bytes = derive_aes_key(secret_key_base);
    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| format!("Failed to initialize AES-256-GCM cipher: {}", e))?;

    let decrypted_bytes = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("AES-256-GCM authentication/decryption failure: {}", e))?;

    String::from_utf8(decrypted_bytes)
        .map_err(|e| format!("UTF-8 decode error: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_roundtrip() {
        let secret = "test_secret_key_base_chatwoot_development_key";
        let original_token = "EAAG_TEST_ACCESS_TOKEN_XYZ123456789";

        let encrypted = encrypt_sensitive_token(original_token, secret).expect("Encryption failed");
        assert!(encrypted.starts_with(ENCRYPTION_PREFIX));
        assert_ne!(encrypted, original_token);

        let decrypted = decrypt_sensitive_token(&encrypted, secret).expect("Decryption failed");
        assert_eq!(decrypted, original_token);
    }

    #[test]
    fn test_random_nonce_produces_different_ciphertexts() {
        let secret = "secure_key_123";
        let token = "my_waba_access_token";

        let enc1 = encrypt_sensitive_token(token, secret).unwrap();
        let enc2 = encrypt_sensitive_token(token, secret).unwrap();

        assert_ne!(enc1, enc2);
        assert_eq!(decrypt_sensitive_token(&enc1, secret).unwrap(), token);
        assert_eq!(decrypt_sensitive_token(&enc2, secret).unwrap(), token);
    }

    #[test]
    fn test_backward_compatibility_unencrypted_token() {
        let secret = "any_secret";
        let legacy_token = "EAABcd12345678_plain_unencrypted";

        let decrypted = decrypt_sensitive_token(legacy_token, secret).unwrap();
        assert_eq!(decrypted, legacy_token);
    }

    #[test]
    fn test_wrong_key_fails_decryption() {
        let secret_a = "secret_a_1234567890";
        let secret_b = "secret_b_0987654321";
        let token = "super_secret_payload";

        let encrypted = encrypt_sensitive_token(token, secret_a).unwrap();
        let res = decrypt_sensitive_token(&encrypted, secret_b);
        assert!(res.is_err());
    }
}
