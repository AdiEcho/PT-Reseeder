use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2, Algorithm, Params, Version,
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
    let parsed = PasswordHash::new(phc_hash)
        .map_err(|e| CryptoError::KdfFailed(e.to_string()))?;
    let argon2 = Argon2::default();
    Ok(argon2.verify_password(password.as_bytes(), &parsed).is_ok())
}

/// Generate a random 16-byte salt.
pub fn generate_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    salt
}
