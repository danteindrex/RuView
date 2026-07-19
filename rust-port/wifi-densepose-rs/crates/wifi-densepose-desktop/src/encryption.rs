//! AES-256-GCM authenticated encryption for PHI/PII data at rest.
//!
//! Uses the aws-lc-rs FIPS 140-3 validated implementation.
//! Keys are managed by `auth::vault::VaultManager` (Stronghold-backed).

use aws_lc_rs::{
    aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM},
    rand::{SecureRandom, SystemRandom},
};
use zeroize::Zeroizing;

const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;

pub struct DataEncryptor {
    key: Zeroizing<[u8; 32]>,
    rng: SystemRandom,
}

impl DataEncryptor {
    pub fn new(key: [u8; 32]) -> Self {
        Self { key: Zeroizing::new(key), rng: SystemRandom::new() }
    }

    /// Returns nonce (12 bytes) || ciphertext || tag (16 bytes)
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        self.rng.fill(&mut nonce_bytes).map_err(|e| e.to_string())?;
        let unbound = UnboundKey::new(&AES_256_GCM, &*self.key).map_err(|e| e.to_string())?;
        let key = LessSafeKey::new(unbound);
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut in_out = plaintext.to_vec();
        key.seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
            .map_err(|e| e.to_string())?;
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&in_out);
        Ok(result)
    }

    /// Expects nonce (12 bytes) || ciphertext || tag (16 bytes)
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        if ciphertext.len() < NONCE_LEN + TAG_LEN {
            return Err("ciphertext too short".to_string());
        }
        let (nonce_bytes, rest) = ciphertext.split_at(NONCE_LEN);
        let nonce_arr: [u8; NONCE_LEN] = nonce_bytes.try_into().map_err(|e: std::array::TryFromSliceError| e.to_string())?;
        let unbound = UnboundKey::new(&AES_256_GCM, &*self.key).map_err(|e| e.to_string())?;
        let key = LessSafeKey::new(unbound);
        let nonce = Nonce::assume_unique_for_key(nonce_arr);
        let mut in_out = rest.to_vec();
        let plaintext = key.open_in_place(nonce, Aad::empty(), &mut in_out)
            .map_err(|e| e.to_string())?;
        Ok(plaintext.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let enc = DataEncryptor::new([0u8; 32]);
        let plain = b"PHI data: HR=72 BR=15";
        let cipher = enc.encrypt(plain).unwrap();
        assert_ne!(&cipher[NONCE_LEN..], plain);
        let decrypted = enc.decrypt(&cipher).unwrap();
        assert_eq!(decrypted, plain);
    }
}
