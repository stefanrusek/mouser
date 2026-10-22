//! minisign signature verification, as `tauri-plugin-updater` does it: the
//! public key and each signature are the base64 of the `.pub` / `.sig` file.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use blake2::{Blake2b512, Digest};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

const TRUSTED_COMMENT: &str = "trusted comment: ";

/// A key or signature file: the text inside the base64.
fn file_text(file_b64: &str, what: &str) -> Result<String, String> {
    let text = STANDARD
        .decode(file_b64.trim())
        .map_err(|e| format!("{what} is not base64: {e}"))?;
    String::from_utf8(text).map_err(|_| format!("{what} is not text"))
}

/// Line `line` of `text`, decoded from base64 to exactly `len` bytes.
fn payload(text: &str, line: usize, len: usize, what: &str) -> Result<Vec<u8>, String> {
    let bytes = text
        .lines()
        .nth(line)
        .ok_or_else(|| format!("{what} is incomplete"))
        .and_then(|l| {
            STANDARD
                .decode(l.trim())
                .map_err(|e| format!("{what} is not base64: {e}"))
        })?;
    if bytes.len() == len {
        Ok(bytes)
    } else {
        Err(format!("{what} has the wrong length"))
    }
}

/// Check that `signature_b64` is a valid signature of `data` by the key
/// `pubkey_b64`, including the signature over the trusted comment.
pub fn verify(pubkey_b64: &str, signature_b64: &str, data: &[u8]) -> Result<(), String> {
    let key_file = file_text(pubkey_b64, "public key")?;
    let sig_file = file_text(signature_b64, "signature")?;
    let key = payload(&key_file, 1, 42, "public key")?;
    let sig = payload(&sig_file, 1, 74, "signature")?;
    let global = payload(&sig_file, 3, 64, "signature")?;
    let comment = sig_file
        .lines()
        .nth(2)
        .and_then(|l| l.strip_prefix(TRUSTED_COMMENT))
        .ok_or("signature has no trusted comment")?;
    if key[2..10] != sig[2..10] {
        return Err("signed with a different key".into());
    }
    let key = VerifyingKey::from_bytes(key[10..].try_into().expect("32 bytes"))
        .map_err(|_| "public key is not valid".to_string())?;
    let signature = Signature::from_bytes(sig[10..].try_into().expect("64 bytes"));
    let result = match &sig[..2] {
        b"Ed" => key.verify(data, &signature),
        b"ED" => key.verify(&Blake2b512::digest(data), &signature),
        _ => return Err("unknown signature algorithm".into()),
    };
    result.map_err(|_| "signature does not match".to_string())?;
    let mut signed = sig[10..].to_vec();
    signed.extend(comment.as_bytes());
    key.verify(
        &signed,
        &Signature::from_bytes(global.as_slice().try_into().expect("64 bytes")),
    )
    .map_err(|_| "trusted comment does not match".to_string())
}
