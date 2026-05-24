//! USM crypto primitives — RFC 3414 §A.2 key derivation, RFC 7860 HMAC,
//! RFC 3826 AES-CFB. Used by the v3 message handler.

use cfb_mode::cipher::{AsyncStreamCipher, KeyIvInit};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

// Hard-coded for Tier 1: HMAC-SHA-256 auth + AES-128 priv. Matches the
// gateway's default snmp3 profile (`Snmpv3AuthProtocol::Sha256` +
// `Snmpv3PrivProtocol::Aes128`). Widen later when more variants ship.

/// AES-128 key length — the leading bytes of the localized auth-key are
/// reused as the priv key per RFC 3826 §3.1.2.1.
pub const PRIV_KEY_LEN: usize = 16;
/// RFC 7860 HMAC-SHA-256-128 — auth tag truncated to 16 bytes.
pub const AUTH_TRUNC_LEN: usize = 16;

/// RFC 3414 §A.2 — hash 1MB of stretched password, then localize against
/// the engine id with `H(key || engineID || key)`.
pub fn derive_localized_key(password: &[u8], engine_id: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    let mut password_index = 0;
    let mut buf = vec![0_u8; 64];
    for _ in 0..16_384 {
        for byte in &mut buf {
            *byte = password[password_index];
            password_index = (password_index + 1) % password.len();
        }
        hasher.update(&buf);
    }
    let key = hasher.finalize();
    let mut localize = Sha256::new();
    localize.update(key);
    localize.update(engine_id);
    localize.update(key);
    localize.finalize().to_vec()
}

/// Compute the RFC 7860 HMAC-SHA-256-128 over a message and return the
/// truncated 16-byte tag.
pub fn hmac_sign(auth_key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(auth_key).expect("HMAC any key length");
    mac.update(message);
    mac.finalize().into_bytes()[..AUTH_TRUNC_LEN].to_vec()
}

/// RFC 3826 §3.1.2 — AES-128 CFB128 with IV = engineBoots(4B) || engineTime(4B) || priv_params(8B).
pub fn aes_cfb_decrypt(
    ciphertext: &[u8],
    priv_key: &[u8],
    engine_boots: u32,
    engine_time: u32,
    priv_params: &[u8],
) -> Vec<u8> {
    let iv = build_iv(engine_boots, engine_time, priv_params);
    let mut out = ciphertext.to_vec();
    cfb_mode::Decryptor::<aes::Aes128>::new_from_slices(&priv_key[..PRIV_KEY_LEN], &iv)
        .expect("AES-128 key/iv length")
        .decrypt(&mut out);
    out
}

/// RFC 3826 §3.1.1.1 — encrypt with the same IV scheme. Returns
/// (ciphertext, priv_params) where priv_params is the random 8-byte salt
/// that gets shipped in the message security parameters.
pub fn aes_cfb_encrypt(
    plaintext: &[u8],
    priv_key: &[u8],
    engine_boots: u32,
    engine_time: u32,
) -> (Vec<u8>, Vec<u8>) {
    use rand::RngCore;
    let mut priv_params = [0_u8; 8];
    rand::rng().fill_bytes(&mut priv_params);
    let iv = build_iv(engine_boots, engine_time, &priv_params);
    let mut out = plaintext.to_vec();
    cfb_mode::Encryptor::<aes::Aes128>::new_from_slices(&priv_key[..PRIV_KEY_LEN], &iv)
        .expect("AES-128 key/iv length")
        .encrypt(&mut out);
    (out, priv_params.to_vec())
}

/// IV = engineBoots(4B BE) || engineTime(4B BE) || priv_params(8B salt).
fn build_iv(engine_boots: u32, engine_time: u32, priv_params: &[u8]) -> [u8; 16] {
    let mut iv = [0_u8; 16];
    iv[0..4].copy_from_slice(&engine_boots.to_be_bytes());
    iv[4..8].copy_from_slice(&engine_time.to_be_bytes());
    iv[8..16].copy_from_slice(&priv_params[..8]);
    iv
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke check on the well-defined §A.2 algorithm: same inputs → same key,
    /// different engine id → different key (localization works).
    #[test]
    fn derive_key_is_deterministic_and_engine_id_localized() {
        let pw = b"mypassphrase1234";
        let eid_a = [0x80_u8, 0x00, 0x86, 0x05, 5, 1, 2, 3, 4, 5, 6, 7, 8];
        let eid_b = [0x80_u8, 0x00, 0x86, 0x05, 5, 9, 9, 9, 9, 9, 9, 9, 9];
        // Arrange / Act
        let key_a1 = derive_localized_key(pw, &eid_a);
        let key_a2 = derive_localized_key(pw, &eid_a);
        let key_b = derive_localized_key(pw, &eid_b);
        // Assert — deterministic + localized
        assert_eq!(key_a1, key_a2);
        assert_ne!(key_a1, key_b);
        assert_eq!(key_a1.len(), 32, "SHA-256 digest length");
    }

    /// Round-trip: encrypt with random salt then decrypt with same salt
    /// recovers the plaintext bit-for-bit.
    #[test]
    fn aes_cfb_round_trip() {
        let key = [0xAA_u8; 16];
        let pt = b"the scoped pdu would go here, 32B";
        let (ct, salt) = aes_cfb_encrypt(pt, &key, 1, 12_345);
        let recovered = aes_cfb_decrypt(&ct, &key, 1, 12_345, &salt);
        assert_eq!(recovered, pt);
    }
}
