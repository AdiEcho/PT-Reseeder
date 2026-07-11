use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};
use rand::RngCore;

use crate::error::CryptoError;

/// Derive a 32-byte Key Encryption Key from a password and salt using Argon2id.
pub fn derive_kek(password: &[u8], salt: &[u8]) -> Result<[u8; 32], CryptoError> {
    let params = Params::new(19 * 1024, 2, 1, Some(32))
        .map_err(|e| CryptoError::KdfFailed(e.to_string()))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut kek = [0u8; 32];
    argon2
        .hash_password_into(password, salt, &mut kek)
        .map_err(|e| CryptoError::KdfFailed(e.to_string()))?;
    Ok(kek)
}

/// Hash a password producing a PHC-format string (includes salt, params, hash).
pub fn hash_password(password: &str) -> Result<String, CryptoError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default());
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| CryptoError::KdfFailed(e.to_string()))?;
    Ok(hash.to_string())
}

/// Verify a password against a PHC-format hash string.
pub fn verify_password(password: &str, phc_hash: &str) -> Result<bool, CryptoError> {
    let parsed = PasswordHash::new(phc_hash).map_err(|e| CryptoError::KdfFailed(e.to_string()))?;
    let argon2 = Argon2::default();
    Ok(argon2.verify_password(password.as_bytes(), &parsed).is_ok())
}

/// Generate a random 16-byte salt.
pub fn generate_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    salt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_salt_returns_16_bytes() {
        let salt = generate_salt();
        assert_eq!(salt.len(), 16);
    }

    #[test]
    fn generate_salt_produces_unique_values() {
        let salt1 = generate_salt();
        let salt2 = generate_salt();
        assert_ne!(salt1, salt2, "two consecutive salts should differ");
    }

    #[test]
    fn derive_kek_returns_32_byte_key() {
        let salt = generate_salt();
        let kek = derive_kek(b"test-password", &salt).unwrap();
        assert_eq!(kek.len(), 32);
    }

    #[test]
    fn derive_kek_is_deterministic_for_same_inputs() {
        let salt = generate_salt();
        let kek1 = derive_kek(b"password", &salt).unwrap();
        let kek2 = derive_kek(b"password", &salt).unwrap();
        assert_eq!(kek1, kek2);
    }

    #[test]
    fn derive_kek_differs_for_different_passwords() {
        let salt = generate_salt();
        let kek1 = derive_kek(b"password-a", &salt).unwrap();
        let kek2 = derive_kek(b"password-b", &salt).unwrap();
        assert_ne!(kek1, kek2);
    }

    #[test]
    fn derive_kek_differs_for_different_salts() {
        let salt1 = generate_salt();
        let salt2 = generate_salt();
        let kek1 = derive_kek(b"same-password", &salt1).unwrap();
        let kek2 = derive_kek(b"same-password", &salt2).unwrap();
        assert_ne!(kek1, kek2);
    }

    #[test]
    fn hash_password_produces_phc_format_string() {
        let hash = hash_password("my-password").unwrap();
        // PHC format starts with $argon2id$
        assert!(
            hash.starts_with("$argon2id$"),
            "expected PHC format, got: {}",
            hash
        );
    }

    #[test]
    fn hash_password_produces_unique_hashes_for_same_input() {
        let hash1 = hash_password("same-password").unwrap();
        let hash2 = hash_password("same-password").unwrap();
        // Different salts should produce different hashes
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn verify_password_returns_true_for_correct_password() {
        let hash = hash_password("correct").unwrap();
        assert!(verify_password("correct", &hash).unwrap());
    }

    #[test]
    fn verify_password_returns_false_for_wrong_password() {
        let hash = hash_password("correct").unwrap();
        assert!(!verify_password("wrong", &hash).unwrap());
    }

    #[test]
    fn verify_password_returns_error_for_invalid_hash_string() {
        let result = verify_password("any", "not-a-valid-phc-string");
        assert!(result.is_err());
    }

    #[test]
    fn derive_kek_works_with_empty_password() {
        let salt = generate_salt();
        let kek = derive_kek(b"", &salt).unwrap();
        assert_eq!(kek.len(), 32);
    }
}
