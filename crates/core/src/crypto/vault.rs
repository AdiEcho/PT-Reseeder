use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;
use zeroize::Zeroizing;

use super::kdf;
use crate::error::CryptoError;

/// Data returned when creating a new vault (user registration).
#[derive(Debug)]
pub struct RegistrationData {
    pub password_hash: String,
    pub kdf_salt: Vec<u8>,
    pub wrapped_dek: Vec<u8>,
    pub dek_nonce: Vec<u8>,
}

/// Data returned when re-wrapping the DEK with a new password.
#[derive(Debug)]
pub struct RewrapData {
    pub new_password_hash: String,
    pub new_kdf_salt: Vec<u8>,
    pub new_wrapped_dek: Vec<u8>,
    pub new_dek_nonce: Vec<u8>,
}

/// Envelope-encryption vault. Holds a zeroizing DEK in memory.
pub struct Vault {
    dek: Zeroizing<[u8; 32]>,
}

impl Clone for Vault {
    fn clone(&self) -> Self {
        Self {
            dek: Zeroizing::new(*self.dek),
        }
    }
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault").field("dek", &"[REDACTED]").finish()
    }
}

impl Vault {
    /// Create a brand-new vault: generates a random DEK, wraps it with a
    /// password-derived KEK, and hashes the password separately.
    pub fn create(password: &str) -> Result<(Vault, RegistrationData), CryptoError> {
        // Generate random DEK
        let mut dek = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut dek);

        // Generate KDF salt and derive KEK
        let kdf_salt = kdf::generate_salt();
        let kek = kdf::derive_kek(password.as_bytes(), &kdf_salt)?;

        // Wrap DEK with AES-256-GCM(KEK, DEK)
        let cipher = Aes256Gcm::new_from_slice(&kek).map_err(|_| CryptoError::InvalidKey)?;
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let wrapped_dek = cipher
            .encrypt(nonce, dek.as_ref())
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

        // Hash password separately
        let password_hash = kdf::hash_password(password)?;

        let vault = Vault {
            dek: Zeroizing::new(dek),
        };
        let reg = RegistrationData {
            password_hash,
            kdf_salt: kdf_salt.to_vec(),
            wrapped_dek,
            dek_nonce: nonce_bytes.to_vec(),
        };
        Ok((vault, reg))
    }

    /// Unlock an existing vault: verify password, derive KEK, unwrap DEK.
    pub fn unlock(
        password: &str,
        kdf_salt: &[u8],
        wrapped_dek: &[u8],
        dek_nonce: &[u8],
        password_hash: &str,
    ) -> Result<Vault, CryptoError> {
        // Verify password
        if !kdf::verify_password(password, password_hash)? {
            return Err(CryptoError::DecryptionFailed(
                "invalid password".to_string(),
            ));
        }

        // Derive KEK and unwrap DEK
        let kek = kdf::derive_kek(password.as_bytes(), kdf_salt)?;
        let cipher = Aes256Gcm::new_from_slice(&kek).map_err(|_| CryptoError::InvalidKey)?;

        let nonce_arr: [u8; 12] = dek_nonce
            .try_into()
            .map_err(|_| CryptoError::DecryptionFailed("invalid nonce length".to_string()))?;
        let nonce = Nonce::from_slice(&nonce_arr);

        let dek_bytes = cipher
            .decrypt(nonce, wrapped_dek)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;

        let dek: [u8; 32] = dek_bytes
            .try_into()
            .map_err(|_| CryptoError::DecryptionFailed("invalid DEK length".to_string()))?;

        Ok(Vault {
            dek: Zeroizing::new(dek),
        })
    }

    /// Encrypt plaintext with AES-256-GCM using the vault DEK.
    /// Returns (ciphertext, nonce).
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<(Vec<u8>, [u8; 12]), CryptoError> {
        let cipher =
            Aes256Gcm::new_from_slice(self.dek.as_ref()).map_err(|_| CryptoError::InvalidKey)?;
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

        Ok((ciphertext, nonce_bytes))
    }

    /// Decrypt ciphertext with AES-256-GCM using the vault DEK.
    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8; 12]) -> Result<Vec<u8>, CryptoError> {
        let cipher =
            Aes256Gcm::new_from_slice(self.dek.as_ref()).map_err(|_| CryptoError::InvalidKey)?;
        let n = Nonce::from_slice(nonce);

        cipher
            .decrypt(n, ciphertext)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }

    /// Re-wrap the DEK with a new password. Unlocks with the old password,
    /// then wraps the same DEK under a new KEK derived from the new password.
    pub fn rewrap(
        old_password: &str,
        new_password: &str,
        kdf_salt: &[u8],
        wrapped_dek: &[u8],
        dek_nonce: &[u8],
        password_hash: &str,
    ) -> Result<RewrapData, CryptoError> {
        // Unlock with old password to get the DEK
        let vault = Self::unlock(
            old_password,
            kdf_salt,
            wrapped_dek,
            dek_nonce,
            password_hash,
        )?;

        // Generate new KDF salt and derive new KEK
        let new_kdf_salt = kdf::generate_salt();
        let new_kek = kdf::derive_kek(new_password.as_bytes(), &new_kdf_salt)?;

        // Wrap DEK with new KEK
        let cipher = Aes256Gcm::new_from_slice(&new_kek).map_err(|_| CryptoError::InvalidKey)?;
        let mut new_nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut new_nonce_bytes);
        let nonce = Nonce::from_slice(&new_nonce_bytes);
        let new_wrapped_dek = cipher
            .encrypt(nonce, vault.dek.as_ref())
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

        // Hash new password
        let new_password_hash = kdf::hash_password(new_password)?;

        Ok(RewrapData {
            new_password_hash,
            new_kdf_salt: new_kdf_salt.to_vec(),
            new_wrapped_dek,
            new_dek_nonce: new_nonce_bytes.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let (vault, _reg) = Vault::create("test-password").unwrap();
        let plaintext = b"hello world, this is secret data";

        let (ciphertext, nonce) = vault.encrypt(plaintext).unwrap();
        let decrypted = vault.decrypt(&ciphertext, &nonce).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn rewrap_preserves_dek() {
        let password = "original-password";
        let (vault, reg) = Vault::create(password).unwrap();

        // Encrypt some data with the original vault
        let plaintext = b"data encrypted before rewrap";
        let (ciphertext, nonce) = vault.encrypt(plaintext).unwrap();

        // Rewrap with a new password
        let new_password = "new-password";
        let rewrap = Vault::rewrap(
            password,
            new_password,
            &reg.kdf_salt,
            &reg.wrapped_dek,
            &reg.dek_nonce,
            &reg.password_hash,
        )
        .unwrap();

        // Unlock with the new password
        let new_vault = Vault::unlock(
            new_password,
            &rewrap.new_kdf_salt,
            &rewrap.new_wrapped_dek,
            &rewrap.new_dek_nonce,
            &rewrap.new_password_hash,
        )
        .unwrap();

        // Old ciphertext should still decrypt (DEK unchanged)
        let decrypted = new_vault.decrypt(&ciphertext, &nonce).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_password_fails_unlock() {
        let (_, reg) = Vault::create("correct-password").unwrap();

        let result = Vault::unlock(
            "wrong-password",
            &reg.kdf_salt,
            &reg.wrapped_dek,
            &reg.dek_nonce,
            &reg.password_hash,
        );

        assert!(result.is_err());
    }

    #[test]
    fn create_unlock_roundtrip() {
        let password = "my-secure-password";
        let (original_vault, reg) = Vault::create(password).unwrap();

        // Encrypt with original
        let plaintext = b"roundtrip test data";
        let (ciphertext, nonce) = original_vault.encrypt(plaintext).unwrap();

        // Unlock and decrypt
        let unlocked_vault = Vault::unlock(
            password,
            &reg.kdf_salt,
            &reg.wrapped_dek,
            &reg.dek_nonce,
            &reg.password_hash,
        )
        .unwrap();

        let decrypted = unlocked_vault.decrypt(&ciphertext, &nonce).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn create_produces_non_empty_registration_data() {
        let (_vault, reg) = Vault::create("pw").unwrap();
        assert!(!reg.password_hash.is_empty());
        assert!(!reg.kdf_salt.is_empty());
        assert!(!reg.wrapped_dek.is_empty());
        assert!(!reg.dek_nonce.is_empty());
    }

    #[test]
    fn encrypt_produces_different_ciphertext_each_time() {
        let (vault, _) = Vault::create("pw").unwrap();
        let plaintext = b"same data";
        let (ct1, _n1) = vault.encrypt(plaintext).unwrap();
        let (ct2, _n2) = vault.encrypt(plaintext).unwrap();
        // Different nonces mean different ciphertexts
        assert_ne!(ct1, ct2);
    }

    #[test]
    fn decrypt_fails_with_corrupted_ciphertext() {
        let (vault, _) = Vault::create("pw").unwrap();
        let (mut ciphertext, nonce) = vault.encrypt(b"secret").unwrap();
        // Flip a byte
        if let Some(byte) = ciphertext.first_mut() {
            *byte ^= 0xff;
        }
        let result = vault.decrypt(&ciphertext, &nonce);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_fails_with_wrong_nonce() {
        let (vault, _) = Vault::create("pw").unwrap();
        let (ciphertext, _nonce) = vault.encrypt(b"secret").unwrap();
        let wrong_nonce = [0u8; 12];
        let result = vault.decrypt(&ciphertext, &wrong_nonce);
        assert!(result.is_err());
    }

    #[test]
    fn encrypt_empty_plaintext_succeeds() {
        let (vault, _) = Vault::create("pw").unwrap();
        let (ciphertext, nonce) = vault.encrypt(b"").unwrap();
        let decrypted = vault.decrypt(&ciphertext, &nonce).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn encrypt_large_plaintext_roundtrips() {
        let (vault, _) = Vault::create("pw").unwrap();
        let plaintext = vec![0xABu8; 1024 * 64]; // 64 KB
        let (ciphertext, nonce) = vault.encrypt(&plaintext).unwrap();
        let decrypted = vault.decrypt(&ciphertext, &nonce).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn unlock_fails_with_invalid_nonce_length() {
        let (_vault, reg) = Vault::create("pw").unwrap();
        let result = Vault::unlock(
            "pw",
            &reg.kdf_salt,
            &reg.wrapped_dek,
            &[0u8; 5], // wrong nonce length
            &reg.password_hash,
        );
        assert!(result.is_err());
    }

    #[test]
    fn rewrap_with_wrong_old_password_fails() {
        let (_vault, reg) = Vault::create("original").unwrap();
        let result = Vault::rewrap(
            "wrong-old",
            "new",
            &reg.kdf_salt,
            &reg.wrapped_dek,
            &reg.dek_nonce,
            &reg.password_hash,
        );
        assert!(result.is_err());
    }

    #[test]
    fn vault_debug_redacts_dek() {
        let (vault, _) = Vault::create("pw").unwrap();
        let debug_str = format!("{:?}", vault);
        assert!(debug_str.contains("REDACTED"));
        assert!(!debug_str.contains("dek: ["));
    }

    #[test]
    fn vault_clone_can_decrypt_same_data() {
        let (vault, _) = Vault::create("pw").unwrap();
        let (ciphertext, nonce) = vault.encrypt(b"clone test").unwrap();
        let cloned = vault.clone();
        let decrypted = cloned.decrypt(&ciphertext, &nonce).unwrap();
        assert_eq!(decrypted, b"clone test");
    }
}
