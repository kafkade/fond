# ADR-023: Backup & recovery — device archive and per-user server snapshots

**Status**: Proposed
**Date**: 2026-07-24
**Decision**: Add two backup features: (1) a **device backup** — a portable archive of `.cook`
files + photos + the authored overlay in a **new, self-describing authenticated archive format** (its own
named MAC/signature trust anchor — **not** `FONDENC1`, which only seals an `OverlayBundle` and cannot
wrap a generic archive), available plaintext or encrypted; and (2) **per-user server-side backup** —
**immutable, coherent** encrypted snapshots on `fond-server` (ADR-021) with point-in-time restore and
a **separate deletion authority**. Both are optional and honor zero-knowledge. Device backup (1) is
**decoupled from the ADR-020 key hierarchy and ships first (1.1)**; only its `--encrypt` mode and the
server snapshots (2) depend on ADR-020/021. Extends ADR-019.

## Context

fond has **no first-class backup/restore** today; users rely on file-sync copies and OS backups.
With optional sync (ADR-021) and a zero-knowledge key hierarchy (ADR-020), backups become both more
necessary (a hosted vault the operator can't recover) and more tractable (the crypto and versioned
blobs already exist). The request explicitly asks for **device backup** and **per-user server
backup** with restore.

## Threat model

**In scope:** device loss/failure, accidental deletion, ransomware on the local machine, and server
loss. Encrypted backups protect confidentiality on untrusted media/hosts (AEAD, tamper-evident).

**Out of scope / honest limits:** an encrypted backup is **also** undecryptable if **either** the
passphrase **or** the Secret Key is lost (ADR-020) — the plaintext `.cook` backstop mitigates for
recipe content **only**, not for encrypted overlay or photos, and only for recipes still present on a
surviving device. A local attacker with keys present can read a decrypted restore (OS/device hygiene,
ADR-019). A backup is worthless unless a **restore has actually been verified** (drill required).

## Decision

### Device backup (local, ADR-023.1) — ships 1.1, no key hierarchy required

- `fond backup create [--dest PATH] [--encrypt] [--plaintext]` → a single portable archive in a
  **new self-describing authenticated archive format** containing `.cook` files, content-addressed
  photos, the authored overlay, and a manifest (per-file content hash + archive root) authenticated
  by a **named trust anchor** (a MAC key or signing key the format identifies explicitly — not an
  unspecified "signed digest"). `--plaintext` (default) produces a plain, integrity-checked archive of
  the user's own recipes (ownership guarantee). `--encrypt` protects confidentiality on untrusted
  media and, **once ADR-020 lands**, uses the Vault Key; until then it uses the existing keychain/
  passphrase key. The plaintext mode has **no dependency on ADR-020**, so device backup ships in 1.1.
- `fond backup restore <archive> [--dry-run]` → **verifies the archive authentication and per-file
  hashes first and fails closed** on any mismatch or missing key, restores files, then `fond reindex`
  rebuilds `fond.db`.
- **Cadence:** manual by default; documented **scheduled** snippets (cron/launchd) rather than a
  resident daemon, matching fond's local-first minimalism.

### Per-user server-side backup (ADR-023.2) — ships with `fond-server`

- `fond-server` (ADR-021) stores versioned encrypted blobs; server-side backup is **immutable,
  retention-bounded** snapshots of those versions with a **coherent snapshot manifest root** so a
  restore point cannot be a torn set of independently-versioned blobs. `fond backup restore
  --from-server --at <checkpoint>` pulls a prior snapshot; the client verifies the manifest and
  decrypts on restore.
- **Deletion authority is separate** from normal sync writes: a compromised device/login cannot purge
  snapshot history (defends against the anti-rollback threat in ADR-021).
- **Self-host:** the operator backs up the object store + Postgres (documented, Immich-style).
- **kafkade-hosted:** kafkade runs encrypted, geo-redundant, immutable backups as part of the
  subscription (ADR-022) — a concrete value-add over self-host.
- Server snapshots are **undecryptable to the operator** (zero-knowledge preserved).

## Rationale

- **Own archive format, not FONDENC1** — FONDENC1 seals a single `OverlayBundle` and cannot wrap a
  generic file archive; a self-describing authenticated format with a named trust anchor is required.
- **Ships early:** plaintext device backup depends on nothing in ADR-020, so it lands in 1.1 and
  gives users real protection immediately.
- **Ownership-first:** the `--plaintext` device archive guarantees the user can always extract their
  own recipes without any key or server.
- **Coherent, immutable server snapshots** — restore points are whole and tamper/rollback-resistant.
- **Honest about limits:** either-secret-lost unrecoverability and the verify-restore drill are
  stated plainly.

## Alternatives Considered

| Alternative | Rejected because |
|---|---|
| Rely on file-sync + OS backups only (status quo) | No point-in-time restore, no integrity verification, no server snapshots; the request explicitly wants backup features. |
| Reuse `FONDENC1` to encrypt the device archive | FONDENC1 only seals an `OverlayBundle`; it cannot authenticate a multi-file archive. A dedicated format is needed. |
| Block device backup on the ADR-020 key hierarchy | Unnecessary coupling; plaintext backup is valuable and independent, so it ships first (1.1). |
| Always-encrypted backups only | Breaks the ownership guarantee that the user can extract plaintext recipes without a key. |
| A resident backup daemon | Heavier than local-first warrants; scheduled snippets suffice. |
| Independently-versioned server blobs as "snapshots" | Can be torn mid-sync; a coherent immutable manifest root is required. |
| Server-side plaintext snapshots | Violates zero-knowledge (ADR-019/021). |
| Store the MUK inside the backup for "easy restore" | Defeats the purpose; a stolen backup would be readable. Only `wrapped_vault_key` may be included. |

## Consequences

- New `fond backup` command surface in `fond`/`fond-store` with a **new authenticated archive format**
  (named MAC/signature anchor); `--encrypt` later adopts the ADR-020 key hierarchy.
- Immutable, coherent server-side snapshot retention on `fond-server` with separate deletion
  authority; restore path in the client sync engine.
- Depends on ADR-019 (crypto) for `--encrypt`; on ADR-020 (keys) only once available; and — for
  server backup — ADR-021 (server).
- **Device backup (ADR-023.1) ships in 1.1** with plaintext + integrity, no server and no key
  hierarchy; `--encrypt` and server backup (ADR-023.2) follow with ADR-020/`fond-server`.
- No CI / `kafkade/github-infra` change from the client-side device backup alone (new deps only).

## Appendix: `FONDBKP1` wire format (ADR-023.1)

The device-backup archive is a self-describing binary format. The authoritative,
byte-level specification lives as the module documentation of
`crates/fond-store/src/backup.rs`; this appendix records the shape so the ADR is
self-contained. The format is **not** `FONDENC1` (which only seals a single
`OverlayBundle`).

```text
┌─ Header (cleartext; authenticated as AEAD AAD in encrypted mode) ─┐
│ magic     "FONDBKP1"   8 bytes                                    │
│ version   u8           1  (currently 1)                           │
│ mode      u8           0 = plaintext, 1 = encrypted               │
│ anchor    u8           0 = blake3-integrity,                      │
│                        1 = xchacha20poly1305-aead                 │
│                        (2 = ed25519 signature — reserved, ADR-020)│
├─ Payload ─────────────────────────────────────────────────────────┤
│ plaintext mode: payload verbatim                                  │
│ encrypted mode: crypto::seal_blob(payload, aad = header)          │
│                 (XChaCha20-Poly1305; key_mode + KDF params +      │
│                  nonce prefix, then ciphertext)                    │
│                                                                   │
│ payload = manifest_len (u32 LE)                                   │
│         ‖ manifest_json (UTF-8)                                   │
│         ‖ body (raw file bytes, concatenated in manifest order)   │
└───────────────────────────────────────────────────────────────────┘
```

- **Manifest** — JSON: `{ format, version, created_at, tool_version, entries,
  archive_root }`. Each entry is `{ path, kind, size, blake3 }`, where `path` is a
  `/`-separated data-dir-relative path, `kind` ∈ `recipe|photo|overlay|other`,
  `size` is the byte length, and `blake3` is the hex BLAKE3-256 of the file's
  bytes. Entries are sorted by path and de-duplicated.
- **Archive root** — hex BLAKE3-256 over the canonical serialization of the sorted
  entries (`path \0 kind \0 size_le \0 blake3 \n` per entry). One value anchoring
  the whole manifest.
- **Named trust anchor** — recorded explicitly in the `anchor` header byte:
  - `blake3-integrity` (plaintext): the unkeyed archive root. Verify/extract need
    **no key**. Honest limit: detects corruption and accidental tampering, not a
    forging adversary who controls the archive — use `--encrypt` on untrusted
    media.
  - `xchacha20poly1305-aead` (encrypted): the Poly1305 tag keyed by the
    keychain/passphrase key (the ADR-020 Vault Key later). The cleartext header is
    bound in as AEAD associated data.
  - `ed25519` (reserved): a future detached signature over the archive root once
    the ADR-020 device signing key exists — a non-breaking addition.
- **Verification is fail-closed** — the anchor is checked first (AEAD open in
  encrypted mode), then every per-file hash is recomputed from the body and the
  archive root is recomputed from the manifest. Any byte flip, reorder, missing
  file, wrong key, or truncation errors out. Restore verifies the whole archive
  before writing any file and rejects unsafe paths (absolute or `..`).
- **Restore** reconstructs the `.cook`/photo/overlay files losslessly, then the
  caller runs `fond reindex` to rebuild the disposable `fond.db`. The database is
  never stored in the archive. The authored overlay lives in `fond.db` and is
  captured only as `overlay/` sidecars, so a backup must export the overlay first
  (the `fond backup` CLI's responsibility, #113).
