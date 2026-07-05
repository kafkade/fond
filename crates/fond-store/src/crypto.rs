//! Sealed-envelope encryption for the authored-overlay sidecar (issue #103, ADR-019).
//!
//! The authored overlay ([`crate::overlay::OverlayBundle`]) is genuine personal
//! data — notes, ratings, cook logs, pantry, meal plans, dietary profiles — that
//! is *designed* to travel over untrusted file-sync (Syncthing/Dropbox/iCloud).
//! Its default plaintext JSONL sidecars are readable by anyone with file access.
//!
//! This module offers **opt-in, authenticated, symmetric encryption** so that
//! slice can cross an untrusted channel safely. It seals an entire bundle into a
//! single self-describing binary envelope and opens it back. It is deliberately
//! **platform-free**: the caller supplies the key material (a raw 32-byte key
//! from the OS keychain, or a user passphrase). No key is ever hardcoded or
//! fetched from the network.
//!
//! ## Guarantees
//!
//! * **Confidentiality + integrity**: XChaCha20-Poly1305 AEAD. The 24-byte nonce
//!   is random per seal (the extended nonce makes random nonces safe). The whole
//!   header (magic, version, key mode, KDF parameters, nonce) is authenticated as
//!   associated data, so any tampering or truncation fails the open.
//! * **Fail closed**: a missing key, wrong key/passphrase, wrong key *mode*, or
//!   any corruption yields an [`Err`] — [`open_bundle`] never returns partial or
//!   plaintext data.
//! * **Passphrase hardening**: passphrase mode derives the key with Argon2id and
//!   a per-file random salt; the salt and cost parameters travel in the header so
//!   any device with the passphrase can re-derive.
//!
//! ## Envelope layout (`FONDENC1`)
//!
//! ```text
//! magic  "FONDENC1"  (8 bytes)
//! version            (1 byte, currently 1)
//! key_mode           (1 byte: 0 = keychain raw-key, 1 = passphrase)
//! ── passphrase mode only ──
//!   salt             (16 bytes)
//!   argon2 m_cost    (4 bytes, little-endian u32, KiB)
//!   argon2 t_cost    (4 bytes, little-endian u32, iterations)
//!   argon2 p_cost    (4 bytes, little-endian u32, lanes)
//! ──────────────────────────
//! nonce              (24 bytes)
//! ciphertext         (remaining bytes: AEAD over the JSON bundle, AAD = header)
//! ```

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, Generate, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use zeroize::Zeroize;

use crate::StoreError;
use crate::overlay::OverlayBundle;

/// File magic identifying a fond sealed overlay bundle.
const MAGIC: &[u8; 8] = b"FONDENC1";
/// Envelope format version.
const VERSION: u8 = 1;

const MODE_KEYCHAIN: u8 = 0;
const MODE_PASSPHRASE: u8 = 1;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
/// XChaCha20-Poly1305 key length.
pub const KEY_LEN: usize = 32;

/// Errors from sealing or opening an encrypted overlay bundle.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    /// The input is not a recognizable `FONDENC1` envelope, or is truncated.
    #[error("not a valid sealed overlay bundle: {0}")]
    Malformed(String),

    /// The envelope was written by a newer, unsupported format version.
    #[error("unsupported sealed-bundle version {0} (this build supports {VERSION})")]
    UnsupportedVersion(u8),

    /// The supplied key material does not match the envelope's key mode
    /// (e.g. a passphrase was given for a keychain-sealed bundle).
    #[error("key mode mismatch: bundle is {expected}, but {provided} material was supplied")]
    KeyModeMismatch {
        expected: &'static str,
        provided: &'static str,
    },

    /// The system RNG failed while generating a salt, nonce, or key.
    #[error("secure random generation failed: {0}")]
    Rng(String),

    /// Argon2id key derivation failed (e.g. invalid parameters).
    #[error("key derivation failed: {0}")]
    Kdf(String),

    /// Authenticated decryption failed: wrong key/passphrase or tampered data.
    ///
    /// This is the fail-closed path — nothing is decoded when it fires.
    #[error("decryption failed: wrong key/passphrase or the bundle was tampered with")]
    Decrypt,

    /// Serializing/deserializing the bundle JSON payload failed.
    #[error("bundle (de)serialization failed: {0}")]
    Serde(String),
}

impl From<CryptoError> for StoreError {
    fn from(e: CryptoError) -> Self {
        StoreError::Crypto {
            message: e.to_string(),
        }
    }
}

/// Which key source an envelope was sealed with (read from its header).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMode {
    /// A raw 32-byte key, typically stored in the OS keychain.
    Keychain,
    /// A key derived from a user passphrase via Argon2id.
    Passphrase,
}

impl KeyMode {
    /// Human-readable label for diagnostics.
    pub fn label(self) -> &'static str {
        match self {
            KeyMode::Keychain => "keychain",
            KeyMode::Passphrase => "passphrase",
        }
    }
}

/// Key material supplied by the caller to seal or open a bundle.
///
/// Raw key bytes are zeroized on drop. The passphrase string is also zeroized on
/// drop; callers should still avoid keeping their own copies around.
pub enum KeyMaterial {
    /// A raw 32-byte symmetric key (e.g. retrieved from the OS keychain).
    Raw([u8; KEY_LEN]),
    /// A user passphrase; the actual key is derived with Argon2id.
    Passphrase(String),
}

impl KeyMaterial {
    fn provided_label(&self) -> &'static str {
        match self {
            KeyMaterial::Raw(_) => "keychain",
            KeyMaterial::Passphrase(_) => "passphrase",
        }
    }
}

impl Drop for KeyMaterial {
    fn drop(&mut self) {
        match self {
            KeyMaterial::Raw(k) => k.zeroize(),
            KeyMaterial::Passphrase(p) => p.zeroize(),
        }
    }
}

impl std::fmt::Debug for KeyMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print secret material.
        match self {
            KeyMaterial::Raw(_) => f.write_str("KeyMaterial::Raw(<redacted>)"),
            KeyMaterial::Passphrase(_) => f.write_str("KeyMaterial::Passphrase(<redacted>)"),
        }
    }
}

/// Returns `true` if `bytes` begins with the sealed-bundle magic.
///
/// Cheap sniff used by the CLI to decide whether an overlay directory holds an
/// encrypted bundle vs. plaintext JSONL sidecars.
pub fn is_sealed(bytes: &[u8]) -> bool {
    bytes.len() >= MAGIC.len() && &bytes[..MAGIC.len()] == MAGIC
}

/// Read the key mode from a sealed envelope without needing the key.
///
/// Lets the CLI decide whether to fetch a keychain key or prompt for a passphrase
/// before it has any secret material.
pub fn peek_key_mode(bytes: &[u8]) -> Result<KeyMode, CryptoError> {
    let header = parse_prefix(bytes)?;
    Ok(header.mode)
}

/// Generate a fresh random 32-byte key for keychain-mode sealing.
pub fn generate_key() -> Result<[u8; KEY_LEN], CryptoError> {
    <[u8; KEY_LEN]>::try_generate().map_err(|e| CryptoError::Rng(e.to_string()))
}

/// Seal an [`OverlayBundle`] into an encrypted, self-describing envelope.
pub fn seal_bundle(bundle: &OverlayBundle, key: &KeyMaterial) -> Result<Vec<u8>, CryptoError> {
    let mut plaintext =
        serde_json::to_vec(bundle).map_err(|e| CryptoError::Serde(e.to_string()))?;

    // Build the authenticated header and derive the working key.
    let mut header: Vec<u8> = Vec::with_capacity(64);
    header.extend_from_slice(MAGIC);
    header.push(VERSION);

    let mut derived: [u8; KEY_LEN] = match key {
        KeyMaterial::Raw(k) => {
            header.push(MODE_KEYCHAIN);
            *k
        }
        KeyMaterial::Passphrase(passphrase) => {
            header.push(MODE_PASSPHRASE);
            let salt: [u8; SALT_LEN] =
                <[u8; SALT_LEN]>::try_generate().map_err(|e| CryptoError::Rng(e.to_string()))?;
            let params = Params::default();
            header.extend_from_slice(&salt);
            header.extend_from_slice(&params.m_cost().to_le_bytes());
            header.extend_from_slice(&params.t_cost().to_le_bytes());
            header.extend_from_slice(&params.p_cost().to_le_bytes());
            derive_key(passphrase, &salt, &params)?
        }
    };

    let nonce_bytes: [u8; NONCE_LEN] =
        <[u8; NONCE_LEN]>::try_generate().map_err(|e| CryptoError::Rng(e.to_string()))?;
    header.extend_from_slice(&nonce_bytes);

    let cipher = XChaCha20Poly1305::new_from_slice(&derived)
        .map_err(|_| CryptoError::Kdf("invalid key length".into()))?;
    derived.zeroize();

    let nonce = XNonce::from(nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: &plaintext,
                aad: &header,
            },
        )
        .map_err(|_| CryptoError::Decrypt)?;
    plaintext.zeroize();

    let mut out = header;
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Open a sealed envelope back into an [`OverlayBundle`].
///
/// Fails closed on any error: a missing/wrong key, a mode mismatch, a tampered
/// or truncated envelope, or malformed JSON all yield [`Err`] and decode nothing.
pub fn open_bundle(bytes: &[u8], key: &KeyMaterial) -> Result<OverlayBundle, CryptoError> {
    let header = parse_prefix(bytes)?;

    let mut derived: [u8; KEY_LEN] = match (header.mode, key) {
        (KeyMode::Keychain, KeyMaterial::Raw(k)) => *k,
        (KeyMode::Passphrase, KeyMaterial::Passphrase(passphrase)) => {
            let kdf = header.kdf.as_ref().ok_or_else(|| {
                CryptoError::Malformed("passphrase envelope missing KDF parameters".into())
            })?;
            let params = Params::new(kdf.m_cost, kdf.t_cost, kdf.p_cost, Some(KEY_LEN))
                .map_err(|e| CryptoError::Kdf(e.to_string()))?;
            derive_key(passphrase, &kdf.salt, &params)?
        }
        (expected, provided) => {
            return Err(CryptoError::KeyModeMismatch {
                expected: expected.label(),
                provided: provided.provided_label(),
            });
        }
    };

    let cipher = XChaCha20Poly1305::new_from_slice(&derived)
        .map_err(|_| CryptoError::Kdf("invalid key length".into()))?;
    derived.zeroize();

    let nonce = XNonce::from(header.nonce);
    let aad = &bytes[..header.header_len];
    let ciphertext = &bytes[header.header_len..];

    let mut plaintext = cipher
        .decrypt(
            &nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Decrypt)?;

    let bundle =
        serde_json::from_slice(&plaintext).map_err(|e| CryptoError::Serde(e.to_string()))?;
    plaintext.zeroize();
    Ok(bundle)
}

/// Argon2id parameters recovered from a passphrase envelope header.
struct KdfHeader {
    salt: [u8; SALT_LEN],
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
}

/// The parsed, fixed-size prefix of an envelope (everything before the ciphertext).
struct ParsedHeader {
    mode: KeyMode,
    kdf: Option<KdfHeader>,
    nonce: [u8; NONCE_LEN],
    /// Number of leading bytes that make up the authenticated header.
    header_len: usize,
}

/// Parse and validate the envelope prefix. Does not touch the ciphertext.
fn parse_prefix(bytes: &[u8]) -> Result<ParsedHeader, CryptoError> {
    if !is_sealed(bytes) {
        return Err(CryptoError::Malformed("missing FONDENC magic".into()));
    }
    // magic(8) + version(1) + mode(1)
    let mut pos = MAGIC.len();
    let version = *bytes
        .get(pos)
        .ok_or_else(|| CryptoError::Malformed("truncated header (version)".into()))?;
    if version != VERSION {
        return Err(CryptoError::UnsupportedVersion(version));
    }
    pos += 1;
    let mode_byte = *bytes
        .get(pos)
        .ok_or_else(|| CryptoError::Malformed("truncated header (key mode)".into()))?;
    pos += 1;

    let (mode, kdf) = match mode_byte {
        MODE_KEYCHAIN => (KeyMode::Keychain, None),
        MODE_PASSPHRASE => {
            let salt = take_array::<SALT_LEN>(bytes, &mut pos, "salt")?;
            let m_cost = take_u32(bytes, &mut pos, "m_cost")?;
            let t_cost = take_u32(bytes, &mut pos, "t_cost")?;
            let p_cost = take_u32(bytes, &mut pos, "p_cost")?;
            (
                KeyMode::Passphrase,
                Some(KdfHeader {
                    salt,
                    m_cost,
                    t_cost,
                    p_cost,
                }),
            )
        }
        other => {
            return Err(CryptoError::Malformed(format!(
                "unknown key mode byte {other}"
            )));
        }
    };

    let nonce = take_array::<NONCE_LEN>(bytes, &mut pos, "nonce")?;

    Ok(ParsedHeader {
        mode,
        kdf,
        nonce,
        header_len: pos,
    })
}

/// Read a fixed-size byte array at `*pos`, advancing it. Bounds-checked.
fn take_array<const N: usize>(
    bytes: &[u8],
    pos: &mut usize,
    field: &str,
) -> Result<[u8; N], CryptoError> {
    let end = *pos + N;
    let slice = bytes
        .get(*pos..end)
        .ok_or_else(|| CryptoError::Malformed(format!("truncated header ({field})")))?;
    let mut out = [0u8; N];
    out.copy_from_slice(slice);
    *pos = end;
    Ok(out)
}

/// Read a little-endian `u32` at `*pos`, advancing it. Bounds-checked.
fn take_u32(bytes: &[u8], pos: &mut usize, field: &str) -> Result<u32, CryptoError> {
    let raw = take_array::<4>(bytes, pos, field)?;
    Ok(u32::from_le_bytes(raw))
}

/// Derive a 32-byte key from a passphrase with Argon2id.
fn derive_key(
    passphrase: &str,
    salt: &[u8; SALT_LEN],
    params: &Params,
) -> Result<[u8; KEY_LEN], CryptoError> {
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params.clone());
    let mut out = [0u8; KEY_LEN];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut out)
        .map_err(|e| CryptoError::Kdf(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::{NoteSidecar, OverlayBundle, PantrySidecar};

    fn sample_bundle() -> OverlayBundle {
        OverlayBundle {
            notes: vec![NoteSidecar {
                id: "0190a1b2-c3d4-7000-8000-000000000001".into(),
                recipe_slug: "adobo".into(),
                user: Some("alice".into()),
                body: "needs more vinegar".into(),
                created_at: "2026-07-01T12:00:00Z".into(),
            }],
            pantry: vec![PantrySidecar {
                name: "soy sauce".into(),
                present: true,
                quantity: Some("1".into()),
                unit: Some("bottle".into()),
                expiry: None,
                par_level: None,
                updated_at: "2026-07-01T12:00:00Z".into(),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn raw_key_round_trip() {
        let bundle = sample_bundle();
        let key = generate_key().unwrap();
        let sealed = seal_bundle(&bundle, &KeyMaterial::Raw(key)).unwrap();

        assert!(is_sealed(&sealed));
        assert_eq!(peek_key_mode(&sealed).unwrap(), KeyMode::Keychain);

        let opened = open_bundle(&sealed, &KeyMaterial::Raw(key)).unwrap();
        assert_eq!(opened, bundle);
    }

    #[test]
    fn passphrase_round_trip() {
        let bundle = sample_bundle();
        let sealed =
            seal_bundle(&bundle, &KeyMaterial::Passphrase("correct horse".into())).unwrap();

        assert_eq!(peek_key_mode(&sealed).unwrap(), KeyMode::Passphrase);

        let opened =
            open_bundle(&sealed, &KeyMaterial::Passphrase("correct horse".into())).unwrap();
        assert_eq!(opened, bundle);
    }

    #[test]
    fn ciphertext_is_not_plaintext() {
        // The recipe slug and note body must not appear in the sealed bytes.
        let bundle = sample_bundle();
        let key = generate_key().unwrap();
        let sealed = seal_bundle(&bundle, &KeyMaterial::Raw(key)).unwrap();
        let haystack = String::from_utf8_lossy(&sealed);
        assert!(!haystack.contains("vinegar"));
        assert!(!haystack.contains("soy sauce"));
    }

    #[test]
    fn wrong_passphrase_fails_closed() {
        let bundle = sample_bundle();
        let sealed = seal_bundle(&bundle, &KeyMaterial::Passphrase("right".into())).unwrap();
        let err = open_bundle(&sealed, &KeyMaterial::Passphrase("wrong".into())).unwrap_err();
        assert!(matches!(err, CryptoError::Decrypt));
    }

    #[test]
    fn wrong_raw_key_fails_closed() {
        let bundle = sample_bundle();
        let sealed = seal_bundle(&bundle, &KeyMaterial::Raw(generate_key().unwrap())).unwrap();
        let err = open_bundle(&sealed, &KeyMaterial::Raw(generate_key().unwrap())).unwrap_err();
        assert!(matches!(err, CryptoError::Decrypt));
    }

    #[test]
    fn tampered_ciphertext_fails_closed() {
        let bundle = sample_bundle();
        let key = generate_key().unwrap();
        let mut sealed = seal_bundle(&bundle, &KeyMaterial::Raw(key)).unwrap();
        // Flip a byte in the ciphertext (the last byte is inside the auth tag).
        let last = sealed.len() - 1;
        sealed[last] ^= 0xFF;
        let err = open_bundle(&sealed, &KeyMaterial::Raw(key)).unwrap_err();
        assert!(matches!(err, CryptoError::Decrypt));
    }

    #[test]
    fn tampered_header_fails_closed() {
        // Mutating an authenticated header byte (the nonce) must break the open.
        let bundle = sample_bundle();
        let key = generate_key().unwrap();
        let mut sealed = seal_bundle(&bundle, &KeyMaterial::Raw(key)).unwrap();
        // Byte 10 is the first nonce byte in keychain mode (magic8+ver1+mode1).
        sealed[10] ^= 0xFF;
        let err = open_bundle(&sealed, &KeyMaterial::Raw(key)).unwrap_err();
        assert!(matches!(err, CryptoError::Decrypt));
    }

    #[test]
    fn key_mode_mismatch_is_reported() {
        let bundle = sample_bundle();
        let sealed = seal_bundle(&bundle, &KeyMaterial::Raw(generate_key().unwrap())).unwrap();
        // Bundle is keychain mode; supply a passphrase.
        let err = open_bundle(&sealed, &KeyMaterial::Passphrase("x".into())).unwrap_err();
        assert!(matches!(err, CryptoError::KeyModeMismatch { .. }));
    }

    #[test]
    fn non_envelope_bytes_rejected() {
        assert!(!is_sealed(b"just some json\n"));
        let err = peek_key_mode(b"nope").unwrap_err();
        assert!(matches!(err, CryptoError::Malformed(_)));
    }

    #[test]
    fn truncated_envelope_rejected() {
        let bundle = sample_bundle();
        let key = generate_key().unwrap();
        let sealed = seal_bundle(&bundle, &KeyMaterial::Raw(key)).unwrap();
        // Keep only the magic + a couple bytes.
        let truncated = &sealed[..10.min(sealed.len())];
        let err = open_bundle(truncated, &KeyMaterial::Raw(key)).unwrap_err();
        assert!(matches!(
            err,
            CryptoError::Malformed(_) | CryptoError::Decrypt
        ));
    }
}
