//! Tokens aleatorios, hashes y cifrado de credenciales (AES-256-GCM, D-014).

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use sha2::{Digest, Sha256};

/// Token opaco de 32 bytes en base64url (sesiones, CSRF, state OAuth).
pub fn random_token() -> String {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).expect("CSPRNG del sistema");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

pub fn sha256(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}

/// Comparación en tiempo constante.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Clone)]
pub struct Cipher(Aes256Gcm);

impl Cipher {
    pub fn new(key: &[u8; 32]) -> Self {
        Cipher(Aes256Gcm::new_from_slice(key).expect("clave de 32 bytes"))
    }

    /// nonce(12) ‖ ciphertext.
    pub fn encrypt(&self, plaintext: &[u8]) -> anyhow::Result<Vec<u8>> {
        let mut nonce = [0u8; 12];
        getrandom::fill(&mut nonce).map_err(|e| anyhow::anyhow!("rng: {e}"))?;
        let ct = self
            .0
            .encrypt(&Nonce::from(nonce), plaintext)
            .map_err(|_| anyhow::anyhow!("no se pudo cifrar"))?;
        Ok([nonce.as_slice(), &ct].concat())
    }

    pub fn decrypt(&self, data: &[u8]) -> anyhow::Result<Vec<u8>> {
        if data.len() < 13 {
            anyhow::bail!("dato cifrado inválido");
        }
        let (nonce, ct) = data.split_at(12);
        let nonce = Nonce::try_from(nonce).map_err(|_| anyhow::anyhow!("nonce inválido"))?;
        self.0
            .decrypt(&nonce, ct)
            .map_err(|_| anyhow::anyhow!("no se pudo descifrar"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_tamper_detection() {
        let c = Cipher::new(&[1u8; 32]);
        let mut enc = c.encrypt(b"refresh-token").unwrap();
        assert_eq!(c.decrypt(&enc).unwrap(), b"refresh-token");
        let last = enc.len() - 1;
        enc[last] ^= 1;
        assert!(c.decrypt(&enc).is_err());
        assert!(
            Cipher::new(&[2u8; 32])
                .decrypt(&c.encrypt(b"x").unwrap())
                .is_err()
        );
    }

    #[test]
    fn tokens_are_unique() {
        assert_ne!(random_token(), random_token());
        assert!(ct_eq(b"abc", b"abc") && !ct_eq(b"abc", b"abd") && !ct_eq(b"a", b"ab"));
    }
}
