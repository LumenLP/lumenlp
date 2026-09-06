use {
    anyhow::{anyhow, Context, Result},
    base64::{engine::general_purpose, Engine as _},
    ed25519_dalek::{Signature, Verifier, VerifyingKey},
    rand::{rngs::OsRng, RngCore},
    sha2::{Digest, Sha256},
    stellar_strkey::ed25519::PublicKey,
};

const SEP53_PREFIX: &[u8] = b"Stellar Signed Message:\n";

pub fn random_opaque_value() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn challenge_message(address: &str, nonce: &str, expires_at: i64) -> String {
    format!(
        "Sign in to LumenLP\n\nAddress: {address}\nNonce: {nonce}\nExpires at: {expires_at}\n\nThis request does not submit a transaction."
    )
}

pub fn verify_sep53(address: &str, message: &str, encoded_signature: &str) -> Result<()> {
    let public_key = PublicKey::from_string(address).map_err(|_| anyhow!("invalid Stellar account address"))?;
    let signature_bytes = decode_signature(encoded_signature)?;
    let signature = Signature::from_slice(&signature_bytes).context("invalid Ed25519 signature length")?;
    let verifying_key = VerifyingKey::from_bytes(&public_key.0).context("invalid Ed25519 public key")?;

    let mut payload = Vec::with_capacity(SEP53_PREFIX.len() + message.len());
    payload.extend_from_slice(SEP53_PREFIX);
    payload.extend_from_slice(message.as_bytes());
    let digest = Sha256::digest(payload);
    verifying_key
        .verify(&digest, &signature)
        .context("signature verification failed")
}

fn decode_signature(value: &str) -> Result<Vec<u8>> {
    general_purpose::STANDARD
        .decode(value)
        .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(value))
        .context("signature must be base64 encoded")
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        ed25519_dalek::{Signer, SigningKey},
    };

    #[test]
    fn verifies_sep53_signature_and_rejects_changed_message() {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let address = PublicKey(signing_key.verifying_key().to_bytes()).to_string();
        let message = challenge_message(&address, "nonce", 123);
        let mut payload = SEP53_PREFIX.to_vec();
        payload.extend_from_slice(message.as_bytes());
        let signature = signing_key.sign(&Sha256::digest(payload));
        let encoded = general_purpose::STANDARD.encode(signature.to_bytes());

        verify_sep53(&address, &message, &encoded).expect("valid SEP-53 signature");
        assert!(verify_sep53(&address, "changed", &encoded).is_err());
    }

    #[test]
    fn hashes_tokens_without_storing_the_secret() {
        assert_eq!(token_hash("secret"), token_hash("secret"));
        assert_ne!(token_hash("secret"), token_hash("other"));
        assert!(!token_hash("secret").contains("secret"));
    }
}
