use ring::aead::{Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};

use crate::error::AppError;

const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

pub fn generate_master_key() -> Result<[u8; KEY_LEN], AppError> {
    let rng = SystemRandom::new();
    let mut key = [0u8; KEY_LEN];
    rng.fill(&mut key)
        .map_err(|e| AppError::Tunnel(format!("Key generation failed: {:?}", e)))?;
    Ok(key)
}

pub fn encrypt(plaintext: &[u8], key: &[u8; KEY_LEN]) -> Result<Vec<u8>, AppError> {
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut nonce_bytes)
        .map_err(|e| AppError::Tunnel(format!("Nonce generation failed: {:?}", e)))?;

    let sealing_key = ring::aead::LessSafeKey::new(
        UnboundKey::new(&AES_256_GCM, key)
            .map_err(|e| AppError::Tunnel(format!("Invalid key: {:?}", e)))?
    );
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut in_out = plaintext.to_vec();
    sealing_key.seal_in_place_append_tag(nonce, ring::aead::Aad::empty(), &mut in_out)
        .map_err(|e| AppError::Tunnel(format!("Encryption failed: {:?}", e)))?;

    // Prepend nonce to ciphertext
    let mut result = nonce_bytes.to_vec();
    result.extend_from_slice(&in_out);
    Ok(result)
}

pub fn decrypt(ciphertext: &[u8], key: &[u8; KEY_LEN]) -> Result<Vec<u8>, AppError> {
    if ciphertext.len() < NONCE_LEN + 16 {
        // nonce + minimum tag
        return Err(AppError::Tunnel("Invalid ciphertext".to_string()));
    }

    let nonce_bytes = &ciphertext[..NONCE_LEN];
    let encrypted_data = &ciphertext[NONCE_LEN..];

    let opening_key = ring::aead::LessSafeKey::new(
        UnboundKey::new(&AES_256_GCM, key)
            .map_err(|e| AppError::Tunnel(format!("Invalid key: {:?}", e)))?
    );
    let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
        .map_err(|e| AppError::Tunnel(format!("Invalid nonce: {:?}", e)))?;

    let mut in_out = encrypted_data.to_vec();
    let plaintext = opening_key.open_in_place(nonce, ring::aead::Aad::empty(), &mut in_out)
        .map_err(|e| AppError::Tunnel(format!("Decryption failed: {:?}", e)))?;

    Ok(plaintext.to_vec())
}