use ring::aead::{LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
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

    pub fn encrypt(&self, key: &[u8], data: &[u8]) -> Result<Vec<u8>, &'static str> {
        let unbound_key = UnboundKey::new(&AES_256_GCM, key).map_err(|_| "Invalid key")?;
        let key = LessSafeKey::new(unbound_key);
        let mut nonce_bytes = [0u8; 12];
        self.rng.fill(&mut nonce_bytes).map_err(|_| "RNG error")?;
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut in_out = data.to_vec();
        key.seal_in_place_append_tag(nonce, ring::aead::Aad::empty(), &mut in_out)
            .map_err(|_| "Encryption failed")?;
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&in_out);
        Ok(result)
    }

    pub fn decrypt(&self, key: &[u8], data: &[u8]) -> Result<Vec<u8>, &'static str> {
        if data.len() < 12 {
            return Err("Invalid data");
        }
        let unbound_key = UnboundKey::new(&AES_256_GCM, key).map_err(|_| "Invalid key")?;
        let key = LessSafeKey::new(unbound_key);
        let nonce = Nonce::assume_unique_for_key(data[..12].try_into().unwrap());
        let mut in_out = data[12..].to_vec();
        let decrypted = key
            .open_in_place(nonce, ring::aead::Aad::empty(), &mut in_out)
            .map_err(|_| "Decryption failed")?;
        Ok(decrypted.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let crypto = Crypto::new();
        let key = [0u8; 32]; // Dummy key for testing (32 bytes for AES-256)
        let data = b"Hello, world!";
        let encrypted = crypto.encrypt(&key, data).unwrap();
        let decrypted = crypto.decrypt(&key, &encrypted).unwrap();
        assert_eq!(data.to_vec(), decrypted);
    }

    #[test]
    fn test_encrypt_decrypt_empty_data() {
        let crypto = Crypto::new();
        let key = [0u8; 32];
        let data = b"";
        let encrypted = crypto.encrypt(&key, data).unwrap();
        let decrypted = crypto.decrypt(&key, &encrypted).unwrap();
        assert_eq!(data.to_vec(), decrypted);
    }

    #[test]
    fn test_decrypt_invalid_key() {
        let crypto = Crypto::new();
        let key = [0u8; 32];
        let wrong_key = [1u8; 32];
        let data = b"Hello, world!";
        let encrypted = crypto.encrypt(&key, data).unwrap();
        let result = crypto.decrypt(&wrong_key, &encrypted);
        assert!(result.is_err());
    }
}