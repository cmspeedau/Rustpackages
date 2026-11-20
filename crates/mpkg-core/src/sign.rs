use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SignError {
    #[error("invalid key or signature encoding: {0}")]
    Encoding(String),
}

/// Generate an ed25519 keypair. Returns (secret_b64, public_b64).
pub fn generate_keypair() -> (String, String) {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let sk = SigningKey::from_bytes(&bytes);
    let vk = sk.verifying_key();
    (STANDARD.encode(sk.to_bytes()), STANDARD.encode(vk.to_bytes()))
}

/// Sign `data` with a base64-encoded secret key. Returns base64 signature.
pub fn sign(secret_b64: &str, data: &[u8]) -> Result<String, SignError> {
    let bytes = STANDARD
        .decode(secret_b64)
        .map_err(|e| SignError::Encoding(e.to_string()))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| SignError::Encoding("secret key must be 32 bytes".into()))?;
    let sk = SigningKey::from_bytes(&arr);
    Ok(STANDARD.encode(sk.sign(data).to_bytes()))
}

/// Verify a base64 signature over `data` with a base64 public key.
pub fn verify(public_b64: &str, data: &[u8], sig_b64: &str) -> Result<bool, SignError> {
    let pk = STANDARD
        .decode(public_b64)
        .map_err(|e| SignError::Encoding(e.to_string()))?;
    let vk = VerifyingKey::from_bytes(
        &pk.try_into()
            .map_err(|_| SignError::Encoding("public key must be 32 bytes".into()))?,
    )
    .map_err(|e| SignError::Encoding(e.to_string()))?;
    let sig_bytes = STANDARD
        .decode(sig_b64)
        .map_err(|e| SignError::Encoding(e.to_string()))?;
    let sig = Signature::from_slice(&sig_bytes)
        .map_err(|e| SignError::Encoding(e.to_string()))?;
    Ok(vk.verify(data, &sig).is_ok())
}
