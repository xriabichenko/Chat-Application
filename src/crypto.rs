// src/crypto.rs
use anyhow::{Result, anyhow};
use ring::aead::{LessSafeKey, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};

pub struct Crypto {
    rng: SystemRandom,
}

impl Crypto {
    pub fn new() -> Self {
        Crypto {
            rng: SystemRandom::new(),
        }
    }

    pub fn encrypt(&self, key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
        // Create an AEAD key from the provided key
        let unbound_key = UnboundKey::new(&ring::aead::AES_256_GCM, key)
            .map_err(|e| anyhow!("Failed to create key: {}", e))?;
        let less_safe_key = LessSafeKey::new(unbound_key);

        // Generate a random nonce
        let mut nonce_bytes = [0u8; 12];
        self.rng
            .fill(&mut nonce_bytes)
            .map_err(|e| anyhow!("Failed to generate nonce: {}", e))?;
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        // Encrypt the data
        let mut in_out = data.to_vec();
        let tag = less_safe_key
            .seal_in_place_append_tag(nonce, ring::aead::Aad::empty(), &mut in_out)
            .map_err(|e| anyhow!("Encryption failed: {}", e))?;

        // Append the nonce and tag to the output
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&in_out);
        result.extend_from_slice(tag.as_ref());
        Ok(result)
    }

    pub fn decrypt(&self, key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 12 {
            return Err(anyhow!("Invalid data length: too short"));
        }

        // Extract nonce (first 12 bytes)
        let nonce_bytes = &data[..12];
        let nonce = Nonce::assume_unique_for_key(nonce_bytes.try_into().map_err(|_| anyhow!("Invalid nonce length"))?);

        // Extract ciphertext and tag
        let ciphertext_and_tag = &data[12..];

        // Create an AEAD key
        let unbound_key = UnboundKey::new(&ring::aead::AES_256_GCM, key)
            .map_err(|e| anyhow!("Failed to create key: {}", e))?;
        let less_safe_key = LessSafeKey::new(unbound_key);

        // Decrypt the data
        let mut in_out = ciphertext_and_tag.to_vec();
        let plaintext = less_safe_key
            .open_in_place(nonce, ring::aead::Aad::empty(), &mut in_out)
            .map_err(|e| anyhow!("Decryption failed: {}", e))?;

        Ok(plaintext.to_vec())
    }
}