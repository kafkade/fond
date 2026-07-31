//! `fond backup` — device backup create / restore / verify (issue #113, ADR-023).
//!
//! This is the CLI layer over [`fond_store::backup`]'s authenticated `FONDBKP1`
//! archive format (issue #112). It captures the source of truth — `.cook`
//! recipes, content-addressed photos, and authored-overlay sidecars — into a
//! single portable archive, and restores it **fail-closed**: authentication and
//! every per-file hash are verified before a byte is written, then `fond reindex`
//! rebuilds the derived `fond.db` (never restored directly — principle #2).
//!
//! Plaintext is the default (no key, no network — the ownership guarantee that a
//! user can always get their recipes back). `--encrypt` seals the archive with
//! XChaCha20-Poly1305 using the **same household key** as `fond overlay
//! --encrypt` (OS keychain by default, or a passphrase via
//! `FOND_OVERLAY_PASSPHRASE`) — no separate key system.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use fond_store::FondPaths;
use fond_store::backup as store_backup;
use fond_store::backup::{BackupManifest, BackupMode, EntryKind};
use fond_store::crypto::{KeyMaterial, KeyMode};

use crate::OutputFormat;
use crate::{overlay_key, run_reindex};

/// Create a backup archive from the data directory.
pub fn cmd_backup_create(
    paths: &FondPaths,
    dest: Option<PathBuf>,
    encrypt: bool,
    plaintext: bool,
    passphrase: bool,
    fmt: &OutputFormat,
) -> Result<()> {
    // `--plaintext` is the explicit spelling of the default; `--encrypt` and
    // `--plaintext` are mutually exclusive at the clap layer.
    let _ = plaintext;

    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    let mode = if encrypt {
        BackupMode::Encrypted
    } else {
        BackupMode::Plaintext
    };

    let key = if encrypt {
        Some(overlay_key::acquire_export_key(passphrase)?)
    } else {
        None
    };

    let dest = dest.unwrap_or_else(|| default_dest(paths));

    let manifest = store_backup::create_backup(&paths.data_dir, &dest, mode, key.as_ref())
        .with_context(|| format!("failed to create backup at {}", dest.display()))?;

    let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    let key_mode = if passphrase { "passphrase" } else { "keychain" };

    match fmt {
        OutputFormat::Json => {
            let out = serde_json::json!({
                "dest": dest.display().to_string(),
                "encrypted": encrypt,
                "key_mode": if encrypt { Some(key_mode) } else { None },
                "size_bytes": size,
                "manifest": manifest,
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Table => {
            let how = if encrypt {
                format!("encrypted ({key_mode} key)")
            } else {
                "plaintext".to_string()
            };
            println!("Created {how} backup at {}", dest.display());
            print_kind_counts(&manifest);
            println!("  size:       {}", human_size(size));
            println!("  root:       {}", short_root(&manifest.archive_root));
        }
    }
    Ok(())
}

/// Restore a backup archive into the data directory, verifying it first.
pub fn cmd_backup_restore(
    paths: &FondPaths,
    archive: &Path,
    dry_run: bool,
    fmt: &OutputFormat,
) -> Result<()> {
    let bytes = std::fs::read(archive)
        .with_context(|| format!("failed to read backup archive at {}", archive.display()))?;

    // Peek the self-describing header (no key needed) to learn whether a key is
    // required and, if so, in what mode — so we prompt/fetch before verifying.
    let header = store_backup::read_header(&bytes)
        .with_context(|| format!("{} is not a valid fond backup archive", archive.display()))?;
    let key = resolve_open_key(header.mode, header.key_mode)?;

    if dry_run {
        // Verify (fail-closed) and enumerate what *would* be written — nothing is
        // touched on disk.
        let files = store_backup::decode_archive(&bytes, key.as_ref())
            .context("backup verification failed — refusing to restore (nothing was written)")?;

        let mut counts = KindCounts::default();
        let mut planned = Vec::with_capacity(files.len());
        for f in &files {
            counts.add(f.kind);
            planned.push(serde_json::json!({
                "path": f.path,
                "kind": f.kind,
                "size": f.data.len(),
                "dest": paths.data_dir.join(&f.path).display().to_string(),
            }));
        }

        match fmt {
            OutputFormat::Json => {
                let out = serde_json::json!({
                    "dry_run": true,
                    "archive": archive.display().to_string(),
                    "target_dir": paths.data_dir.display().to_string(),
                    "encrypted": matches!(header.mode, BackupMode::Encrypted),
                    "would_restore": counts.total(),
                    "counts": counts.to_json(),
                    "files": planned,
                });
                println!("{}", serde_json::to_string_pretty(&out)?);
            }
            OutputFormat::Table => {
                println!(
                    "Dry run — {} file(s) would be restored into {} (nothing written):",
                    counts.total(),
                    paths.data_dir.display()
                );
                counts.print_indented();
            }
        }
        return Ok(());
    }

    // Real restore: verify-first, fail-closed, path-traversal-safe (all enforced
    // by the store). On failure nothing is written and the target is untouched.
    let report = store_backup::restore_backup(archive, &paths.data_dir, key.as_ref())
        .context("backup verification failed — refusing to restore (nothing was written)")?;

    // The database is derived (principle #2): rebuild it from the restored files.
    let (reindex, _merge) = run_reindex(paths)?;

    match fmt {
        OutputFormat::Json => {
            let out = serde_json::json!({
                "archive": archive.display().to_string(),
                "target_dir": paths.data_dir.display().to_string(),
                "encrypted": matches!(header.mode, BackupMode::Encrypted),
                "restore": report,
                "reindexed": reindex.indexed,
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Table => {
            println!(
                "Restored {} file(s) into {}",
                report.restored,
                paths.data_dir.display()
            );
            println!("  recipes:    {}", report.recipes);
            println!("  photos:     {}", report.photos);
            println!("  overlay:    {}", report.overlay);
            if report.other > 0 {
                println!("  other:      {}", report.other);
            }
            println!("Rebuilt index: {} recipe(s)", reindex.indexed);
        }
    }
    Ok(())
}

/// Verify a backup archive — the "prove restore works" drill.
pub fn cmd_backup_verify(
    paths: &FondPaths,
    archive: &Path,
    against_source: bool,
    fmt: &OutputFormat,
) -> Result<()> {
    let bytes = std::fs::read(archive)
        .with_context(|| format!("failed to read backup archive at {}", archive.display()))?;

    let header = store_backup::read_header(&bytes)
        .with_context(|| format!("{} is not a valid fond backup archive", archive.display()))?;
    let key = resolve_open_key(header.mode, header.key_mode)?;

    // The pass/fail gate: authentication + every per-file hash + the archive root.
    // Fails closed on any tamper, truncation, missing file, or wrong/missing key.
    let manifest = store_backup::verify_archive(&bytes, key.as_ref())
        .context("backup verification FAILED — this archive would not restore")?;

    let source_diff = if against_source {
        Some(diff_against_source(paths, &bytes, key.as_ref())?)
    } else {
        None
    };

    match fmt {
        OutputFormat::Json => {
            let out = serde_json::json!({
                "status": "pass",
                "archive": archive.display().to_string(),
                "encrypted": matches!(header.mode, BackupMode::Encrypted),
                "entries": manifest.entries.len(),
                "archive_root": manifest.archive_root,
                "source_diff": source_diff.as_ref().map(SourceDiff::to_json),
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Table => {
            println!("PASS — backup verified: {}", archive.display());
            print_kind_counts(&manifest);
            println!("  root:       {}", short_root(&manifest.archive_root));
            if let Some(diff) = &source_diff {
                println!("Compared against current data directory:");
                println!("  unchanged:  {}", diff.matched);
                println!("  changed:    {}", diff.changed);
                println!(
                    "  missing:    {} (in backup, absent from source)",
                    diff.missing
                );
                println!("  new:        {} (in source, not in backup)", diff.extra);
            }
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════

/// Resolve key material for opening an archive whose header we've read. Plaintext
/// needs no key; encrypted fails closed if the key/passphrase can't be obtained.
fn resolve_open_key(mode: BackupMode, key_mode: Option<KeyMode>) -> Result<Option<KeyMaterial>> {
    match mode {
        BackupMode::Plaintext => Ok(None),
        BackupMode::Encrypted => {
            let km = key_mode.context(
                "archive is encrypted but its key mode is missing from the header (malformed)",
            )?;
            Ok(Some(overlay_key::acquire_import_key(km)?))
        }
    }
}

/// Default archive destination: `<data-dir>/backups/fond-backup-<UTC>.fondbkp`.
fn default_dest(paths: &FondPaths) -> PathBuf {
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    paths
        .data_dir
        .join("backups")
        .join(format!("fond-backup-{stamp}.fondbkp"))
}

/// Compare a verified archive's file bytes against the live data directory,
/// reporting drift. All in memory — writes nothing, needs no temp dir.
fn diff_against_source(
    paths: &FondPaths,
    bytes: &[u8],
    key: Option<&KeyMaterial>,
) -> Result<SourceDiff> {
    let archived = store_backup::decode_archive(bytes, key)
        .context("failed to decode archive for source comparison")?;
    let live = store_backup::collect_backup_files(&paths.data_dir)
        .context("failed to read the current data directory for comparison")?;

    let archived_map: BTreeMap<&str, &[u8]> = archived
        .iter()
        .map(|f| (f.path.as_str(), f.data.as_slice()))
        .collect();
    let live_map: BTreeMap<&str, &[u8]> = live
        .iter()
        .map(|f| (f.path.as_str(), f.data.as_slice()))
        .collect();

    let mut diff = SourceDiff::default();
    for (path, data) in &archived_map {
        match live_map.get(path) {
            Some(live_data) if live_data == data => diff.matched += 1,
            Some(_) => diff.changed += 1,
            None => diff.missing += 1,
        }
    }
    for path in live_map.keys() {
        if !archived_map.contains_key(path) {
            diff.extra += 1;
        }
    }
    Ok(diff)
}

/// Drift between an archive and the current data directory.
#[derive(Default)]
struct SourceDiff {
    /// Files present in both with identical bytes.
    matched: usize,
    /// Files present in both but with differing bytes.
    changed: usize,
    /// Files in the archive but absent from the current data directory.
    missing: usize,
    /// Files in the current data directory but not in the archive.
    extra: usize,
}

impl SourceDiff {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "matched": self.matched,
            "changed": self.changed,
            "missing_in_source": self.missing,
            "new_in_source": self.extra,
        })
    }
}

/// Running tally of files by [`EntryKind`], for restore/dry-run reports.
#[derive(Default)]
struct KindCounts {
    recipes: usize,
    photos: usize,
    overlay: usize,
    other: usize,
}

impl KindCounts {
    fn add(&mut self, kind: EntryKind) {
        match kind {
            EntryKind::Recipe => self.recipes += 1,
            EntryKind::Photo => self.photos += 1,
            EntryKind::Overlay => self.overlay += 1,
            EntryKind::Other => self.other += 1,
        }
    }

    fn total(&self) -> usize {
        self.recipes + self.photos + self.overlay + self.other
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "recipes": self.recipes,
            "photos": self.photos,
            "overlay": self.overlay,
            "other": self.other,
        })
    }

    fn print_indented(&self) {
        println!("  recipes:    {}", self.recipes);
        println!("  photos:     {}", self.photos);
        println!("  overlay:    {}", self.overlay);
        if self.other > 0 {
            println!("  other:      {}", self.other);
        }
    }
}

/// Print per-kind counts derived from a manifest's entries (create/verify).
fn print_kind_counts(manifest: &BackupManifest) {
    let mut counts = KindCounts::default();
    for e in &manifest.entries {
        counts.add(e.kind);
    }
    println!("  files:      {}", manifest.entries.len());
    counts.print_indented();
}

/// First 12 hex chars of the archive root, for a compact human display.
fn short_root(root: &str) -> String {
    let head: String = root.chars().take(12).collect();
    format!("{head}… (blake3)")
}

/// Render a byte count as a compact human string.
fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    if bytes >= MB {
        format!("{:.1} MiB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KiB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
