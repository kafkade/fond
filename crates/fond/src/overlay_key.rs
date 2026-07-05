//! Key acquisition for the encrypted authored-overlay sidecar (issue #103).
//!
//! `fond-store` performs the cryptography but is deliberately platform-free: it
//! never touches the OS keychain or a terminal. This module is where the CLI
//! resolves the *key material* the store needs — either a raw 32-byte key kept
//! in the OS keychain (the default) or a key derived from a user passphrase.
//!
//! Principles: the key is always user-controlled and offline. It is never
//! hardcoded, and never fetched from the network (principles #1/#2).

use anyhow::{Context, Result, bail};
use fond_scrape::CredentialStore;
use fond_store::crypto::{KEY_LEN, KeyMaterial, KeyMode};

/// Keychain service namespace for fond's overlay key.
const KEYCHAIN_SERVICE: &str = "fond-overlay";
/// Keychain entry name for the single household overlay key.
const KEYCHAIN_ENTRY: &str = "sidecar-key";
/// Environment variable that supplies a passphrase non-interactively (CI,
/// scripts, headless devices). Takes precedence over an interactive prompt.
const PASSPHRASE_ENV: &str = "FOND_OVERLAY_PASSPHRASE";

/// Acquire the key for an **encrypted export**.
///
/// * Passphrase mode: read `FOND_OVERLAY_PASSPHRASE`, or prompt twice (with
///   confirmation) on a TTY.
/// * Keychain mode (default): fetch the stored key, generating and storing a new
///   random one on first use (printing a one-time notice so the user knows a key
///   now guards their synced overlay).
pub fn acquire_export_key(passphrase: bool) -> Result<KeyMaterial> {
    if passphrase {
        return Ok(KeyMaterial::Passphrase(resolve_passphrase(true)?));
    }
    let (key, created) = keychain_get_or_create()?;
    if created {
        eprintln!(
            "Generated a new overlay encryption key and stored it in your OS keychain \
             (service \"{KEYCHAIN_SERVICE}\").\n\
             To decrypt on another device, either sync your keychain or re-export with \
             `--passphrase`. Without the key, the sidecar cannot be read."
        );
    }
    Ok(KeyMaterial::Raw(key))
}

/// Acquire the key for **decrypting** a sealed bundle whose mode is already known
/// from its header. Fails closed when the key/passphrase cannot be obtained.
pub fn acquire_import_key(mode: KeyMode) -> Result<KeyMaterial> {
    match mode {
        KeyMode::Passphrase => Ok(KeyMaterial::Passphrase(resolve_passphrase(false)?)),
        KeyMode::Keychain => {
            let key = keychain_get()?.ok_or_else(|| {
                anyhow::anyhow!(
                    "this overlay is encrypted with a key stored in the OS keychain, but no \
                     key was found (service \"{KEYCHAIN_SERVICE}\"). Import fails closed — no \
                     plaintext is written. Restore the key to your keychain (or re-export the \
                     overlay with `--passphrase`) and try again."
                )
            })?;
            Ok(KeyMaterial::Raw(key))
        }
    }
}

/// Fetch the stored keychain key without creating one. `Ok(None)` if absent.
///
/// Used by `fond reindex` so it can silently merge a keychain-encrypted overlay
/// but skip (never block on) one that is missing its key.
pub fn keychain_get() -> Result<Option<[u8; KEY_LEN]>> {
    let stored = CredentialStore::load(KEYCHAIN_SERVICE, KEYCHAIN_ENTRY)
        .context("failed to read the overlay key from the OS keychain")?;
    match stored {
        Some(hex) => Ok(Some(decode_key(&hex)?)),
        None => Ok(None),
    }
}

/// Fetch the keychain key, generating and storing a fresh random one if absent.
/// Returns the key and whether it was newly created.
fn keychain_get_or_create() -> Result<([u8; KEY_LEN], bool)> {
    if let Some(key) = keychain_get()? {
        return Ok((key, false));
    }
    let key = fond_store::crypto::generate_key()
        .map_err(|e| anyhow::anyhow!("failed to generate an overlay key: {e}"))?;
    CredentialStore::store(KEYCHAIN_SERVICE, KEYCHAIN_ENTRY, &encode_key(&key)).context(
        "failed to store the new overlay key in the OS keychain. If your platform has no \
         keychain, use `--passphrase` instead.",
    )?;
    Ok((key, true))
}

/// Resolve a passphrase: env var first, else an interactive (hidden) prompt.
///
/// `confirm` asks twice and checks they match — used when *creating* a sealed
/// bundle so a typo can't lock the data away.
fn resolve_passphrase(confirm: bool) -> Result<String> {
    if let Ok(p) = std::env::var(PASSPHRASE_ENV) {
        if p.is_empty() {
            bail!("{PASSPHRASE_ENV} is set but empty; provide a non-empty passphrase");
        }
        return Ok(p);
    }

    if !atty::is(atty::Stream::Stdin) {
        bail!(
            "a passphrase is required but stdin is not a terminal. Set {PASSPHRASE_ENV} to \
             provide it non-interactively."
        );
    }

    let first =
        rpassword::prompt_password("Overlay passphrase: ").context("failed to read passphrase")?;
    if first.is_empty() {
        bail!("passphrase must not be empty");
    }
    if confirm {
        let again = rpassword::prompt_password("Confirm passphrase: ")
            .context("failed to read passphrase confirmation")?;
        if again != first {
            bail!("passphrases did not match");
        }
    }
    Ok(first)
}

/// Lowercase-hex encode a raw key for keychain string storage.
fn encode_key(key: &[u8; KEY_LEN]) -> String {
    let mut s = String::with_capacity(KEY_LEN * 2);
    for b in key {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        s.push(char::from_digit((b & 0x0f) as u32, 16).unwrap());
    }
    s
}

/// Decode a lowercase-hex keychain string back into raw key bytes.
fn decode_key(hex: &str) -> Result<[u8; KEY_LEN]> {
    let hex = hex.trim();
    if hex.len() != KEY_LEN * 2 {
        bail!("stored overlay key is malformed (unexpected length)");
    }
    let mut out = [0u8; KEY_LEN];
    let bytes = hex.as_bytes();
    for (i, slot) in out.iter_mut().enumerate() {
        let hi = (bytes[i * 2] as char)
            .to_digit(16)
            .context("stored overlay key is malformed (non-hex)")?;
        let lo = (bytes[i * 2 + 1] as char)
            .to_digit(16)
            .context("stored overlay key is malformed (non-hex)")?;
        *slot = ((hi << 4) | lo) as u8;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_hex_round_trips() {
        let key = fond_store::crypto::generate_key().unwrap();
        let encoded = encode_key(&key);
        assert_eq!(encoded.len(), KEY_LEN * 2);
        assert_eq!(decode_key(&encoded).unwrap(), key);
    }

    #[test]
    fn decode_rejects_bad_length() {
        assert!(decode_key("abcd").is_err());
    }

    #[test]
    fn decode_rejects_non_hex() {
        let bad = "z".repeat(KEY_LEN * 2);
        assert!(decode_key(&bad).is_err());
    }
}
