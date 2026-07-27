//! `FONDBKP1` — authenticated device-backup archive format (issue #112, ADR-023).
//!
//! A device backup is a single portable archive that wraps everything a restore
//! needs to rebuild a fond install: the user's `.cook` recipe files (the source
//! of truth), content-addressed photos, and the authored overlay sidecars. The
//! derived `fond.db` is **not** captured — it is rebuilt by `fond reindex` after
//! a restore.
//!
//! This is a **new, self-describing, authenticated format**. It deliberately
//! does *not* reuse [`crate::crypto`]'s `FONDENC1` envelope, which only seals a
//! single [`crate::overlay::OverlayBundle`] and cannot authenticate a multi-file
//! archive. `FONDBKP1` carries a manifest of per-file content hashes plus a
//! single **archive root**, anchored by an **explicitly named trust anchor**.
//!
//! ## Modes
//!
//! * **Plaintext** (default, [`BackupMode::Plaintext`]) — an integrity-checked,
//!   *unencrypted* archive. The trust anchor is an **unkeyed BLAKE3** archive
//!   root ([`TrustAnchor::Blake3Integrity`]). Verifying and extracting need **no
//!   key and no network** — the ownership guarantee that a user can always get
//!   their recipes back. Honest limit: an adversary who fully controls the
//!   archive can recompute the root, so plaintext detects *corruption and
//!   accidental tampering*, not a forging attacker. Use `--encrypt` on untrusted
//!   media.
//! * **Encrypted** ([`BackupMode::Encrypted`]) — the manifest and body are sealed
//!   with XChaCha20-Poly1305 via [`crate::crypto::seal_blob`]. The Poly1305 tag,
//!   keyed by the keychain/passphrase key (and, once ADR-020 lands, the Vault
//!   Key), is the named trust anchor ([`TrustAnchor::XChaCha20Poly1305`]). The
//!   cleartext header is bound in as AEAD associated data, so any tampering fails
//!   the open. File paths and titles are inside the ciphertext, so they do not
//!   leak on untrusted media.
//!
//! ## Wire layout
//!
//! ```text
//! ┌─ Header (cleartext; authenticated as AEAD AAD in encrypted mode) ─┐
//! │ magic     "FONDBKP1"   8 bytes                                    │
//! │ version   u8           1  (currently 1)                           │
//! │ mode      u8           0 = plaintext, 1 = encrypted               │
//! │ anchor    u8           0 = blake3-integrity,                      │
//! │                        1 = xchacha20poly1305-aead                 │
//! │                        (2 = ed25519 signature — reserved, ADR-020)│
//! ├─ Payload ─────────────────────────────────────────────────────────┤
//! │ Plaintext mode: the payload bytes verbatim.                       │
//! │ Encrypted mode: crypto::seal_blob(payload, aad = header).         │
//! │                                                                   │
//! │ payload = manifest_len (u32 LE)                                   │
//! │         ‖ manifest_json (UTF-8 JSON, see `BackupManifest`)        │
//! │         ‖ body (raw file bytes concatenated in manifest order)    │
//! └───────────────────────────────────────────────────────────────────┘
//! ```
//!
//! * **Per-file hash** — BLAKE3-256 of the file's plaintext bytes, hex-encoded in
//!   the manifest entry.
//! * **Archive root** — BLAKE3-256 over the canonical serialization of the sorted
//!   manifest entries (`path \0 kind \0 size_le \0 blake3 \n` per entry). One
//!   value anchoring the whole manifest; recomputed and compared on verify.
//! * Entries are sorted by path and de-duplicated, so an archive is deterministic
//!   for a given set of files.
//!
//! ## Fail-closed verification
//!
//! [`verify_archive`] and [`decode_archive`] check the anchor first (AEAD open in
//! encrypted mode), then recompute **every** per-file hash from the body and the
//! archive root from the manifest. Any byte flip, reordering, missing file, wrong
//! key, or truncation yields an [`Err`] — nothing partial is ever returned, and
//! [`restore_backup`] writes nothing until verification passes.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::StoreError;
use crate::crypto::{self, CryptoError, KeyMaterial, KeyMode};

/// File magic identifying a fond device-backup archive.
const MAGIC: &[u8; 8] = b"FONDBKP1";
/// Archive format version.
const VERSION: u8 = 1;
/// Length of the cleartext outer header: magic(8) + version(1) + mode(1) + anchor(1).
const HEADER_LEN: usize = MAGIC.len() + 3;

const MODE_PLAINTEXT: u8 = 0;
const MODE_ENCRYPTED: u8 = 1;

const ANCHOR_BLAKE3: u8 = 0;
const ANCHOR_XCHACHA: u8 = 1;

/// Top-level data-dir subdirectory names captured by [`collect_backup_files`].
const RECIPES_DIR: &str = "recipes";
const PHOTOS_DIR: &str = "photos";
const OVERLAY_DIR: &str = "overlay";

/// Errors from encoding, verifying, decoding, or restoring a backup archive.
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    /// The bytes are not a recognizable `FONDBKP1` archive, or are truncated.
    #[error("not a valid FONDBKP1 archive: {0}")]
    Malformed(String),

    /// The archive was written by a newer, unsupported format version.
    #[error("unsupported backup archive version {0} (this build supports {VERSION})")]
    UnsupportedVersion(u8),

    /// An encrypted archive was opened without supplying key material.
    #[error("this archive is encrypted; key material is required to verify or restore it")]
    KeyRequired,

    /// A file's recomputed content hash does not match its manifest entry.
    #[error("integrity check failed: content hash mismatch for {path}")]
    HashMismatch {
        /// Archive-relative path of the offending file.
        path: String,
    },

    /// The recomputed archive root does not match the manifest.
    #[error("integrity check failed: archive root mismatch (manifest was tampered with)")]
    RootMismatch,

    /// A file listed in the manifest is missing or truncated in the body.
    #[error("integrity check failed: file missing or truncated in archive: {path}")]
    MissingFile {
        /// Archive-relative path of the missing file.
        path: String,
    },

    /// A manifest entry carries an unsafe path (absolute or containing `..`).
    #[error("refusing unsafe archive path (traversal or absolute): {path}")]
    PathTraversal {
        /// The rejected path.
        path: String,
    },

    /// There were no files to archive.
    #[error("nothing to back up: no files were collected")]
    Empty,

    /// The manifest JSON failed to (de)serialize.
    #[error("manifest (de)serialization failed: {0}")]
    Serde(String),

    /// A cryptographic operation failed (wrong key, tampered ciphertext, KDF).
    #[error(transparent)]
    Crypto(#[from] CryptoError),

    /// Filesystem I/O failed while collecting, writing, or reading an archive.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<BackupError> for StoreError {
    fn from(e: BackupError) -> Self {
        StoreError::Crypto {
            message: e.to_string(),
        }
    }
}

/// Whether an archive is a plaintext (integrity-only) or encrypted archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackupMode {
    /// Unencrypted, integrity-checked. No key required to verify or restore.
    Plaintext,
    /// Encrypted with XChaCha20-Poly1305 (keychain/passphrase key).
    Encrypted,
}

/// The named authenticator that anchors an archive's integrity.
///
/// Recorded explicitly in the header so the format is self-describing and can
/// gain a signed variant without changing its identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustAnchor {
    /// Unkeyed BLAKE3-256 archive root (plaintext mode integrity).
    Blake3Integrity,
    /// XChaCha20-Poly1305 AEAD tag keyed by the vault/keychain/passphrase key.
    XChaCha20Poly1305,
    // Reserved: `Ed25519` detached signature over the archive root, once the
    // ADR-020 device signing key exists. The `anchor` header byte makes this a
    // non-breaking addition.
}

/// Category of a file in the archive, used for restore placement and reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// A `.cook` recipe file (source of truth).
    Recipe,
    /// A content-addressed photo blob.
    Photo,
    /// An authored-overlay sidecar (notes, ratings, cook logs, pantry, plans).
    Overlay,
    /// Any other captured file.
    Other,
}

impl EntryKind {
    /// Stable lowercase label, used in the archive-root canonicalization.
    fn as_str(self) -> &'static str {
        match self {
            EntryKind::Recipe => "recipe",
            EntryKind::Photo => "photo",
            EntryKind::Overlay => "overlay",
            EntryKind::Other => "other",
        }
    }

    /// Classify a file from its archive-relative path (top-level directory).
    fn from_path(rel: &str) -> Self {
        match rel.split('/').next() {
            Some(RECIPES_DIR) => EntryKind::Recipe,
            Some(PHOTOS_DIR) => EntryKind::Photo,
            Some(OVERLAY_DIR) => EntryKind::Overlay,
            _ => EntryKind::Other,
        }
    }
}

/// A single file to be archived or restored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupFile {
    /// Archive-relative path, using `/` separators (e.g. `recipes/adobo.cook`).
    pub path: String,
    /// The file's category.
    pub kind: EntryKind,
    /// The file's raw bytes.
    pub data: Vec<u8>,
}

/// One manifest row: a file's path, kind, size, and content hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Archive-relative path (`/` separators).
    pub path: String,
    /// File category.
    pub kind: EntryKind,
    /// Byte length of the file.
    pub size: u64,
    /// Hex-encoded BLAKE3-256 hash of the file's bytes.
    pub blake3: String,
}

/// The archive manifest: metadata plus the per-file hash list and archive root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupManifest {
    /// Format tag, always `"FONDBKP1"`.
    pub format: String,
    /// Format version.
    pub version: u8,
    /// RFC 3339 timestamp of when the archive was created.
    pub created_at: String,
    /// The `fond-store` version that wrote the archive.
    pub tool_version: String,
    /// Files in the archive, sorted by path.
    pub entries: Vec<ManifestEntry>,
    /// Hex-encoded BLAKE3-256 root over the canonicalized entries.
    pub archive_root: String,
}

/// The self-describing header read from an archive without needing a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupHeader {
    /// Format version.
    pub version: u8,
    /// Plaintext or encrypted.
    pub mode: BackupMode,
    /// The named trust anchor.
    pub anchor: TrustAnchor,
    /// The key mode of an encrypted archive (`None` for plaintext).
    pub key_mode: Option<KeyMode>,
}

/// Summary of a completed restore.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RestoreReport {
    /// Total files written.
    pub restored: usize,
    /// Recipe (`.cook`) files written.
    pub recipes: usize,
    /// Photo blobs written.
    pub photos: usize,
    /// Overlay sidecar files written.
    pub overlay: usize,
    /// Other files written.
    pub other: usize,
}

// ═══════════════════════════════════════════════════════════════════
// Core file-set engine
// ═══════════════════════════════════════════════════════════════════

/// Encode a set of files into a `FONDBKP1` archive.
///
/// In [`BackupMode::Plaintext`] any `key` is ignored. In
/// [`BackupMode::Encrypted`] a `key` is required, or [`BackupError::KeyRequired`]
/// is returned. Files are sorted by path and de-duplicated; the returned bytes
/// are deterministic for a given input set and mode (modulo the random
/// nonce/salt in encrypted mode and the `created_at` timestamp).
pub fn encode_archive(
    files: &[BackupFile],
    mode: BackupMode,
    key: Option<&KeyMaterial>,
) -> Result<Vec<u8>, BackupError> {
    if files.is_empty() {
        return Err(BackupError::Empty);
    }

    // Build entries paired with their source bytes, then sort by path.
    let mut items: Vec<(&BackupFile, ManifestEntry)> = files
        .iter()
        .map(|f| {
            let hash = blake3::hash(&f.data).to_hex().to_string();
            let entry = ManifestEntry {
                path: f.path.clone(),
                kind: f.kind,
                size: f.data.len() as u64,
                blake3: hash,
            };
            (f, entry)
        })
        .collect();
    items.sort_by(|a, b| a.1.path.cmp(&b.1.path));

    for pair in items.windows(2) {
        if pair[0].1.path == pair[1].1.path {
            return Err(BackupError::Malformed(format!(
                "duplicate path in archive: {}",
                pair[0].1.path
            )));
        }
    }

    let entries: Vec<ManifestEntry> = items.iter().map(|(_, e)| e.clone()).collect();
    let archive_root = compute_root(&entries);

    let manifest = BackupManifest {
        format: String::from_utf8_lossy(MAGIC).into_owned(),
        version: VERSION,
        created_at: chrono::Utc::now().to_rfc3339(),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        entries,
        archive_root,
    };
    let manifest_json =
        serde_json::to_vec(&manifest).map_err(|e| BackupError::Serde(e.to_string()))?;

    let mut body: Vec<u8> = Vec::new();
    for (f, _) in &items {
        body.extend_from_slice(&f.data);
    }

    let mut payload = build_payload(&manifest_json, &body)?;

    let (mode_byte, anchor_byte) = match mode {
        BackupMode::Plaintext => (MODE_PLAINTEXT, ANCHOR_BLAKE3),
        BackupMode::Encrypted => (MODE_ENCRYPTED, ANCHOR_XCHACHA),
    };
    let mut header = Vec::with_capacity(HEADER_LEN);
    header.extend_from_slice(MAGIC);
    header.push(VERSION);
    header.push(mode_byte);
    header.push(anchor_byte);

    let out = match mode {
        BackupMode::Plaintext => {
            let mut out = header;
            out.extend_from_slice(&payload);
            out
        }
        BackupMode::Encrypted => {
            let key = key.ok_or(BackupError::KeyRequired)?;
            let sealed = crypto::seal_blob(&payload, &header, key)?;
            payload.zeroize();
            let mut out = header;
            out.extend_from_slice(&sealed);
            out
        }
    };
    Ok(out)
}

/// Read the self-describing header of an archive without needing a key.
///
/// Lets a caller learn the mode and (for encrypted archives) the key mode so it
/// can fetch the right key material before attempting to open the archive.
pub fn read_header(bytes: &[u8]) -> Result<BackupHeader, BackupError> {
    if bytes.len() < MAGIC.len() || &bytes[..MAGIC.len()] != MAGIC {
        return Err(BackupError::Malformed("missing FONDBKP1 magic".into()));
    }
    let mut pos = MAGIC.len();
    let version = *bytes
        .get(pos)
        .ok_or_else(|| BackupError::Malformed("truncated header (version)".into()))?;
    if version != VERSION {
        return Err(BackupError::UnsupportedVersion(version));
    }
    pos += 1;
    let mode_byte = *bytes
        .get(pos)
        .ok_or_else(|| BackupError::Malformed("truncated header (mode)".into()))?;
    pos += 1;
    let anchor_byte = *bytes
        .get(pos)
        .ok_or_else(|| BackupError::Malformed("truncated header (anchor)".into()))?;

    let mode = match mode_byte {
        MODE_PLAINTEXT => BackupMode::Plaintext,
        MODE_ENCRYPTED => BackupMode::Encrypted,
        other => {
            return Err(BackupError::Malformed(format!("unknown mode byte {other}")));
        }
    };
    let anchor = match anchor_byte {
        ANCHOR_BLAKE3 => TrustAnchor::Blake3Integrity,
        ANCHOR_XCHACHA => TrustAnchor::XChaCha20Poly1305,
        other => {
            return Err(BackupError::Malformed(format!(
                "unknown trust-anchor byte {other}"
            )));
        }
    };

    // The mode and its anchor must agree; a mismatch is a malformed/forged header.
    let consistent = matches!(
        (mode, anchor),
        (BackupMode::Plaintext, TrustAnchor::Blake3Integrity)
            | (BackupMode::Encrypted, TrustAnchor::XChaCha20Poly1305)
    );
    if !consistent {
        return Err(BackupError::Malformed(
            "mode and trust anchor disagree".into(),
        ));
    }

    let key_mode = match mode {
        BackupMode::Plaintext => None,
        BackupMode::Encrypted => Some(crypto::peek_blob_key_mode(&bytes[HEADER_LEN..])?),
    };

    Ok(BackupHeader {
        version,
        mode,
        anchor,
        key_mode,
    })
}

/// Verify an archive's authentication and every per-file hash, returning the
/// manifest. Writes nothing. Fails closed on any mismatch or missing key.
pub fn verify_archive(
    bytes: &[u8],
    key: Option<&KeyMaterial>,
) -> Result<BackupManifest, BackupError> {
    let (manifest, _files) = open_inner(bytes, key)?;
    Ok(manifest)
}

/// Verify an archive and return its files in memory (fail-closed).
pub fn decode_archive(
    bytes: &[u8],
    key: Option<&KeyMaterial>,
) -> Result<Vec<BackupFile>, BackupError> {
    let (_manifest, files) = open_inner(bytes, key)?;
    Ok(files)
}

/// Shared verify path: authenticate, then check every hash and the root.
fn open_inner(
    bytes: &[u8],
    key: Option<&KeyMaterial>,
) -> Result<(BackupManifest, Vec<BackupFile>), BackupError> {
    let header = read_header(bytes)?;

    let mut payload: Vec<u8> = match header.mode {
        BackupMode::Plaintext => bytes[HEADER_LEN..].to_vec(),
        BackupMode::Encrypted => {
            let key = key.ok_or(BackupError::KeyRequired)?;
            let header_bytes = &bytes[..HEADER_LEN];
            crypto::open_blob(&bytes[HEADER_LEN..], header_bytes, key)?
        }
    };

    let result = parse_and_check(&payload);
    payload.zeroize();
    result
}

/// Split the payload into (manifest, body) and verify every hash and the root.
fn parse_and_check(payload: &[u8]) -> Result<(BackupManifest, Vec<BackupFile>), BackupError> {
    let len_bytes = payload
        .get(0..4)
        .ok_or_else(|| BackupError::Malformed("truncated payload (manifest length)".into()))?;
    let manifest_len =
        u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]) as usize;
    let manifest_end = 4usize
        .checked_add(manifest_len)
        .ok_or_else(|| BackupError::Malformed("manifest length overflow".into()))?;
    let manifest_bytes = payload
        .get(4..manifest_end)
        .ok_or_else(|| BackupError::Malformed("truncated payload (manifest)".into()))?;
    let body = &payload[manifest_end..];

    let manifest: BackupManifest =
        serde_json::from_slice(manifest_bytes).map_err(|e| BackupError::Serde(e.to_string()))?;

    if manifest.version != VERSION {
        return Err(BackupError::UnsupportedVersion(manifest.version));
    }

    // Reconstruct each file from the body and verify its content hash.
    let mut files = Vec::with_capacity(manifest.entries.len());
    let mut offset = 0usize;
    for entry in &manifest.entries {
        let size = entry.size as usize;
        let end = offset
            .checked_add(size)
            .ok_or_else(|| BackupError::Malformed("file size overflow".into()))?;
        let slice = body
            .get(offset..end)
            .ok_or_else(|| BackupError::MissingFile {
                path: entry.path.clone(),
            })?;
        let actual = blake3::hash(slice).to_hex().to_string();
        if actual != entry.blake3 {
            return Err(BackupError::HashMismatch {
                path: entry.path.clone(),
            });
        }
        files.push(BackupFile {
            path: entry.path.clone(),
            kind: entry.kind,
            data: slice.to_vec(),
        });
        offset = end;
    }
    if offset != body.len() {
        return Err(BackupError::Malformed(
            "archive body has unexpected trailing bytes".into(),
        ));
    }

    // Recompute the archive root over the (sorted) entries.
    let root = compute_root(&manifest.entries);
    if root != manifest.archive_root {
        return Err(BackupError::RootMismatch);
    }

    Ok((manifest, files))
}

/// Frame the manifest and body into a payload with a u32-LE length prefix.
fn build_payload(manifest_json: &[u8], body: &[u8]) -> Result<Vec<u8>, BackupError> {
    if manifest_json.len() > u32::MAX as usize {
        return Err(BackupError::Malformed("manifest too large".into()));
    }
    let mut p = Vec::with_capacity(4 + manifest_json.len() + body.len());
    p.extend_from_slice(&(manifest_json.len() as u32).to_le_bytes());
    p.extend_from_slice(manifest_json);
    p.extend_from_slice(body);
    Ok(p)
}

/// Compute the BLAKE3-256 archive root over the canonicalized manifest entries.
fn compute_root(entries: &[ManifestEntry]) -> String {
    let mut hasher = blake3::Hasher::new();
    for e in entries {
        hasher.update(e.path.as_bytes());
        hasher.update(&[0]);
        hasher.update(e.kind.as_str().as_bytes());
        hasher.update(&[0]);
        hasher.update(&e.size.to_le_bytes());
        hasher.update(&[0]);
        hasher.update(e.blake3.as_bytes());
        hasher.update(b"\n");
    }
    hasher.finalize().to_hex().to_string()
}

// ═══════════════════════════════════════════════════════════════════
// Data-directory collection and restore
// ═══════════════════════════════════════════════════════════════════

/// Collect the backup-worthy files from a fond data directory.
///
/// Walks `recipes/` (top-level `.cook` files), `photos/` (recursively), and
/// `overlay/` (recursively). The derived `fond.db` and everything else is
/// skipped — a restore rebuilds the database with `fond reindex`.
///
/// The authored overlay lives in `fond.db`; it only appears on disk as `overlay/`
/// sidecars once exported. Ensuring those sidecars are current (via
/// `fond overlay export`) before a backup is the caller's responsibility (the
/// `fond backup` CLI, issue #113).
///
/// Returned files carry `/`-separated, data-dir-relative paths and are sorted for
/// determinism.
pub fn collect_backup_files(data_dir: &Path) -> Result<Vec<BackupFile>, BackupError> {
    let mut rel_paths: Vec<String> = Vec::new();

    let recipes = data_dir.join(RECIPES_DIR);
    if recipes.is_dir() {
        for entry in std::fs::read_dir(&recipes)?.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("cook") {
                rel_paths.push(rel_path(data_dir, &path)?);
            }
        }
    }

    for sub in [PHOTOS_DIR, OVERLAY_DIR] {
        let dir = data_dir.join(sub);
        if dir.is_dir() {
            walk_files(data_dir, &dir, &mut rel_paths)?;
        }
    }

    rel_paths.sort();
    rel_paths.dedup();

    let mut files = Vec::with_capacity(rel_paths.len());
    for rel in rel_paths {
        let abs = data_dir.join(&rel);
        let data = std::fs::read(&abs)?;
        let kind = EntryKind::from_path(&rel);
        files.push(BackupFile {
            path: rel,
            kind,
            data,
        });
    }
    Ok(files)
}

/// Create a backup archive from a data directory and write it to `dest`.
///
/// Returns the manifest of what was archived. Encrypted mode requires `key`.
pub fn create_backup(
    data_dir: &Path,
    dest: &Path,
    mode: BackupMode,
    key: Option<&KeyMaterial>,
) -> Result<BackupManifest, BackupError> {
    let files = collect_backup_files(data_dir)?;
    let bytes = encode_archive(&files, mode, key)?;
    let manifest = verify_archive(&bytes, key)?;
    if let Some(parent) = dest.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dest, &bytes)?;
    Ok(manifest)
}

/// Restore a backup archive into `target_dir`, verifying it **first**.
///
/// Verification (authentication + every per-file hash + the archive root) runs to
/// completion before a single byte is written; a failure leaves `target_dir`
/// untouched. Unsafe archive paths (absolute or containing `..`) are rejected.
/// The derived database is **not** restored — the caller runs `fond reindex`
/// afterwards to rebuild `fond.db`.
pub fn restore_backup(
    archive: &Path,
    target_dir: &Path,
    key: Option<&KeyMaterial>,
) -> Result<RestoreReport, BackupError> {
    let bytes = std::fs::read(archive)?;
    let files = decode_archive(&bytes, key)?;

    // Resolve and validate every destination path before writing anything.
    let mut planned: Vec<(PathBuf, &BackupFile)> = Vec::with_capacity(files.len());
    for f in &files {
        let dest = safe_join(target_dir, &f.path)?;
        planned.push((dest, f));
    }

    let mut report = RestoreReport::default();
    for (dest, f) in planned {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, &f.data)?;
        report.restored += 1;
        match f.kind {
            EntryKind::Recipe => report.recipes += 1,
            EntryKind::Photo => report.photos += 1,
            EntryKind::Overlay => report.overlay += 1,
            EntryKind::Other => report.other += 1,
        }
    }
    Ok(report)
}

/// Recursively collect file paths under `dir`, relative to `data_dir`.
fn walk_files(data_dir: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), BackupError> {
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            walk_files(data_dir, &path, out)?;
        } else if file_type.is_file() {
            out.push(rel_path(data_dir, &path)?);
        }
    }
    Ok(())
}

/// Compute a `/`-separated, `base`-relative path, rejecting non-UTF-8 names.
fn rel_path(base: &Path, path: &Path) -> Result<String, BackupError> {
    let rel = path.strip_prefix(base).map_err(|_| {
        BackupError::Malformed(format!(
            "path {} escapes the data directory",
            path.display()
        ))
    })?;
    let mut parts = Vec::new();
    for comp in rel.components() {
        match comp {
            Component::Normal(os) => {
                let s = os.to_str().ok_or_else(|| {
                    BackupError::Malformed(format!(
                        "non-UTF-8 path component in {}",
                        path.display()
                    ))
                })?;
                parts.push(s);
            }
            _ => {
                return Err(BackupError::Malformed(format!(
                    "unexpected path component in {}",
                    path.display()
                )));
            }
        }
    }
    Ok(parts.join("/"))
}

/// Join an archive-relative path onto `base`, rejecting traversal and absolute
/// paths. Only plain, forward-relative components are allowed.
fn safe_join(base: &Path, rel: &str) -> Result<PathBuf, BackupError> {
    if rel.is_empty() {
        return Err(BackupError::PathTraversal { path: rel.into() });
    }
    let candidate = Path::new(rel);
    let mut out = base.to_path_buf();
    for comp in candidate.components() {
        match comp {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(BackupError::PathTraversal { path: rel.into() });
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::generate_key;

    fn sample_files() -> Vec<BackupFile> {
        vec![
            BackupFile {
                path: "recipes/adobo.cook".into(),
                kind: EntryKind::Recipe,
                data: b">> title: Chicken Adobo\n\nBraise the @chicken{1%kg}.\n".to_vec(),
            },
            BackupFile {
                path: "photos/ab/cd1234.jpg".into(),
                kind: EntryKind::Photo,
                data: vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46],
            },
            BackupFile {
                path: "overlay/shared/pantry.jsonl".into(),
                kind: EntryKind::Overlay,
                data: b"{\"name\":\"soy sauce\",\"present\":true}\n".to_vec(),
            },
        ]
    }

    #[test]
    fn plaintext_round_trip() {
        let files = sample_files();
        let bytes = encode_archive(&files, BackupMode::Plaintext, None).unwrap();

        let header = read_header(&bytes).unwrap();
        assert_eq!(header.mode, BackupMode::Plaintext);
        assert_eq!(header.anchor, TrustAnchor::Blake3Integrity);
        assert_eq!(header.key_mode, None);

        let mut decoded = decode_archive(&bytes, None).unwrap();
        decoded.sort_by(|a, b| a.path.cmp(&b.path));
        let mut expected = files;
        expected.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(decoded, expected);
    }

    #[test]
    fn plaintext_needs_no_key() {
        // The ownership guarantee: verify + extract with no key material at all.
        let bytes = encode_archive(&sample_files(), BackupMode::Plaintext, None).unwrap();
        assert!(verify_archive(&bytes, None).is_ok());
        assert!(decode_archive(&bytes, None).is_ok());
    }

    #[test]
    fn encrypted_round_trip_keychain() {
        let files = sample_files();
        let key = KeyMaterial::Raw(generate_key().unwrap());
        let bytes = encode_archive(&files, BackupMode::Encrypted, Some(&key)).unwrap();

        let header = read_header(&bytes).unwrap();
        assert_eq!(header.mode, BackupMode::Encrypted);
        assert_eq!(header.anchor, TrustAnchor::XChaCha20Poly1305);
        assert_eq!(header.key_mode, Some(KeyMode::Keychain));

        let mut decoded = decode_archive(&bytes, Some(&key)).unwrap();
        decoded.sort_by(|a, b| a.path.cmp(&b.path));
        let mut expected = files;
        expected.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(decoded, expected);
    }

    #[test]
    fn encrypted_round_trip_passphrase() {
        let files = sample_files();
        let key = KeyMaterial::Passphrase("correct horse battery staple".into());
        let bytes = encode_archive(&files, BackupMode::Encrypted, Some(&key)).unwrap();

        assert_eq!(
            read_header(&bytes).unwrap().key_mode,
            Some(KeyMode::Passphrase)
        );

        let key2 = KeyMaterial::Passphrase("correct horse battery staple".into());
        let decoded = decode_archive(&bytes, Some(&key2)).unwrap();
        assert_eq!(decoded.len(), 3);
    }

    #[test]
    fn encrypted_requires_key() {
        let key = KeyMaterial::Raw(generate_key().unwrap());
        let bytes = encode_archive(&sample_files(), BackupMode::Encrypted, Some(&key)).unwrap();

        // Encode without a key fails closed.
        assert!(matches!(
            encode_archive(&sample_files(), BackupMode::Encrypted, None),
            Err(BackupError::KeyRequired)
        ));
        // Verify/decode without a key fails closed.
        assert!(matches!(
            verify_archive(&bytes, None),
            Err(BackupError::KeyRequired)
        ));
        assert!(matches!(
            decode_archive(&bytes, None),
            Err(BackupError::KeyRequired)
        ));
    }

    #[test]
    fn wrong_key_fails_closed() {
        let key = KeyMaterial::Raw(generate_key().unwrap());
        let bytes = encode_archive(&sample_files(), BackupMode::Encrypted, Some(&key)).unwrap();

        let wrong_raw = KeyMaterial::Raw(generate_key().unwrap());
        assert!(matches!(
            decode_archive(&bytes, Some(&wrong_raw)),
            Err(BackupError::Crypto(CryptoError::Decrypt))
        ));

        let pass_bytes = encode_archive(
            &sample_files(),
            BackupMode::Encrypted,
            Some(&KeyMaterial::Passphrase("right".into())),
        )
        .unwrap();
        assert!(matches!(
            decode_archive(&pass_bytes, Some(&KeyMaterial::Passphrase("wrong".into()))),
            Err(BackupError::Crypto(CryptoError::Decrypt))
        ));
    }

    #[test]
    fn tamper_body_byte_detected_plaintext() {
        let mut bytes = encode_archive(&sample_files(), BackupMode::Plaintext, None).unwrap();
        // Flip the final byte, which lives in the last file's body.
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        assert!(matches!(
            decode_archive(&bytes, None),
            Err(BackupError::HashMismatch { .. })
        ));
    }

    #[test]
    fn tamper_body_byte_detected_encrypted() {
        let key = KeyMaterial::Raw(generate_key().unwrap());
        let mut bytes = encode_archive(&sample_files(), BackupMode::Encrypted, Some(&key)).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        assert!(matches!(
            decode_archive(&bytes, Some(&key)),
            Err(BackupError::Crypto(CryptoError::Decrypt))
        ));
    }

    #[test]
    fn tamper_manifest_detected_plaintext() {
        let files = sample_files();
        let bytes = encode_archive(&files, BackupMode::Plaintext, None).unwrap();

        // Find a size digit inside the manifest JSON and bump it, so the manifest
        // disagrees with the body layout. The root is computed over the entries,
        // so a manifest edit must be caught (root mismatch or body-length check).
        let needle = b"\"size\":8"; // the photo entry is 8 bytes
        let start = bytes
            .windows(needle.len())
            .position(|w| w == needle)
            .expect("size field present");
        let mut tampered = bytes.clone();
        tampered[start + needle.len() - 1] = b'9'; // 8 -> 9
        assert!(decode_archive(&tampered, None).is_err());
    }

    #[test]
    fn tamper_header_anchor_detected_encrypted() {
        let key = KeyMaterial::Raw(generate_key().unwrap());
        let mut bytes = encode_archive(&sample_files(), BackupMode::Encrypted, Some(&key)).unwrap();
        // The header is AEAD associated data; flipping the anchor byte (index 10)
        // must break the open. It first trips the mode/anchor consistency check.
        bytes[10] = ANCHOR_BLAKE3;
        assert!(decode_archive(&bytes, Some(&key)).is_err());
    }

    #[test]
    fn missing_file_detected() {
        let bytes = encode_archive(&sample_files(), BackupMode::Plaintext, None).unwrap();
        // Truncate into the body so the last manifest entry has no bytes.
        let truncated = &bytes[..bytes.len() - 4];
        assert!(matches!(
            decode_archive(truncated, None),
            Err(BackupError::MissingFile { .. }) | Err(BackupError::Malformed(_))
        ));
    }

    #[test]
    fn encrypted_hides_paths_and_content() {
        let key = KeyMaterial::Raw(generate_key().unwrap());
        let bytes = encode_archive(&sample_files(), BackupMode::Encrypted, Some(&key)).unwrap();
        let haystack = String::from_utf8_lossy(&bytes);
        assert!(!haystack.contains("adobo.cook"));
        assert!(!haystack.contains("Chicken Adobo"));
        assert!(!haystack.contains("soy sauce"));
        // The manifest key names must not leak either.
        assert!(!haystack.contains("archive_root"));
    }

    #[test]
    fn bad_magic_rejected() {
        assert!(matches!(
            read_header(b"NOTFOND1\x01\x00\x00"),
            Err(BackupError::Malformed(_))
        ));
    }

    #[test]
    fn unsupported_version_rejected() {
        let mut bytes = encode_archive(&sample_files(), BackupMode::Plaintext, None).unwrap();
        bytes[MAGIC.len()] = VERSION + 1;
        assert!(matches!(
            read_header(&bytes),
            Err(BackupError::UnsupportedVersion(_))
        ));
    }

    #[test]
    fn empty_input_rejected() {
        assert!(matches!(
            encode_archive(&[], BackupMode::Plaintext, None),
            Err(BackupError::Empty)
        ));
    }

    #[test]
    fn duplicate_paths_rejected() {
        let dupes = vec![
            BackupFile {
                path: "recipes/a.cook".into(),
                kind: EntryKind::Recipe,
                data: b"one".to_vec(),
            },
            BackupFile {
                path: "recipes/a.cook".into(),
                kind: EntryKind::Recipe,
                data: b"two".to_vec(),
            },
        ];
        assert!(matches!(
            encode_archive(&dupes, BackupMode::Plaintext, None),
            Err(BackupError::Malformed(_))
        ));
    }

    #[test]
    fn safe_join_rejects_traversal() {
        let base = Path::new("/tmp/restore");
        assert!(safe_join(base, "recipes/ok.cook").is_ok());
        assert!(matches!(
            safe_join(base, "../evil"),
            Err(BackupError::PathTraversal { .. })
        ));
        assert!(matches!(
            safe_join(base, "/etc/passwd"),
            Err(BackupError::PathTraversal { .. })
        ));
        assert!(matches!(
            safe_join(base, "a/../../b"),
            Err(BackupError::PathTraversal { .. })
        ));
    }

    #[test]
    fn collect_is_deterministic_and_scoped() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path();
        std::fs::create_dir_all(data.join("recipes")).unwrap();
        std::fs::create_dir_all(data.join("photos/ab")).unwrap();
        std::fs::create_dir_all(data.join("overlay/shared")).unwrap();
        std::fs::write(data.join("recipes/b.cook"), b"Boil @water{1%l}.").unwrap();
        std::fs::write(data.join("recipes/a.cook"), b"Fry @egg{1}.").unwrap();
        std::fs::write(data.join("recipes/notes.txt"), b"ignored").unwrap();
        std::fs::write(data.join("photos/ab/x.jpg"), b"img").unwrap();
        std::fs::write(data.join("overlay/shared/pantry.jsonl"), b"{}").unwrap();
        // The derived DB must never be captured.
        std::fs::write(data.join("fond.db"), b"sqlite").unwrap();

        let files = collect_backup_files(data).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "overlay/shared/pantry.jsonl",
                "photos/ab/x.jpg",
                "recipes/a.cook",
                "recipes/b.cook",
            ]
        );
        assert!(!paths.iter().any(|p| p.contains("fond.db")));
        assert!(!paths.iter().any(|p| p.contains("notes.txt")));
    }

    #[test]
    fn restore_rejects_traversal_archive_without_writing() {
        // Hand-craft a plaintext archive with a single traversal path.
        let evil = vec![BackupFile {
            path: "../escaped.cook".into(),
            kind: EntryKind::Recipe,
            data: b"pwned".to_vec(),
        }];
        let bytes = encode_archive(&evil, BackupMode::Plaintext, None).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let archive_path = tmp.path().join("evil.fondbkp");
        std::fs::write(&archive_path, &bytes).unwrap();
        let target = tmp.path().join("restore");
        std::fs::create_dir_all(&target).unwrap();

        assert!(matches!(
            restore_backup(&archive_path, &target, None),
            Err(BackupError::PathTraversal { .. })
        ));
        assert!(!tmp.path().join("escaped.cook").exists());
    }

    #[test]
    fn create_restore_round_trip_files() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(src.join("recipes")).unwrap();
        std::fs::create_dir_all(src.join("photos/ab")).unwrap();
        std::fs::create_dir_all(src.join("overlay/shared")).unwrap();
        std::fs::write(
            src.join("recipes/adobo.cook"),
            b">> title: Adobo\n\nBraise @chicken{1}.",
        )
        .unwrap();
        std::fs::write(src.join("photos/ab/pic.jpg"), [0u8, 1, 2, 3, 4]).unwrap();
        std::fs::write(
            src.join("overlay/shared/pantry.jsonl"),
            b"{\"name\":\"salt\"}\n",
        )
        .unwrap();

        let dest = tmp.path().join("backup.fondbkp");
        let manifest = create_backup(&src, &dest, BackupMode::Plaintext, None).unwrap();
        assert_eq!(manifest.entries.len(), 3);
        assert_eq!(manifest.format, "FONDBKP1");

        let restored = tmp.path().join("restored");
        let report = restore_backup(&dest, &restored, None).unwrap();
        assert_eq!(report.restored, 3);
        assert_eq!(report.recipes, 1);
        assert_eq!(report.photos, 1);
        assert_eq!(report.overlay, 1);

        // Every file is byte-identical after the round-trip.
        for rel in [
            "recipes/adobo.cook",
            "photos/ab/pic.jpg",
            "overlay/shared/pantry.jsonl",
        ] {
            assert_eq!(
                std::fs::read(src.join(rel)).unwrap(),
                std::fs::read(restored.join(rel)).unwrap(),
                "mismatch for {rel}"
            );
        }
    }

    #[test]
    fn restore_then_reindex_rebuilds_db() {
        use crate::FondDb;
        use crate::reindex::reindex;

        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(src.join("recipes")).unwrap();
        std::fs::write(
            src.join("recipes/adobo.cook"),
            b">> title: Chicken Adobo\n\nBraise the @chicken{1%kg} in @soy sauce{}.",
        )
        .unwrap();
        std::fs::write(
            src.join("recipes/toast.cook"),
            b">> title: Toast\n\nToast the @bread{2%slices}.",
        )
        .unwrap();

        // Back up, then restore into a pristine data dir.
        let dest = tmp.path().join("backup.fondbkp");
        create_backup(&src, &dest, BackupMode::Plaintext, None).unwrap();
        let restored = tmp.path().join("restored");
        restore_backup(&dest, &restored, None).unwrap();

        // A fresh (disposable) DB is rebuilt purely from the restored files.
        let db = FondDb::open_memory().unwrap();
        let report = reindex(&db, &restored.join("recipes")).unwrap();
        assert_eq!(report.indexed, 2, "reindex should rebuild both recipes");
        assert!(
            report.errors.is_empty(),
            "no parse errors: {:?}",
            report.errors
        );

        let conn = db.conn();
        let mut stmt = conn
            .prepare("SELECT title FROM recipes ORDER BY title")
            .unwrap();
        let titles: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            titles,
            vec!["Chicken Adobo".to_string(), "Toast".to_string()]
        );
    }
}
