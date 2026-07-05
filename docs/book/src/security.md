# What fond protects (encryption at rest)

fond is local-first: your data lives on your own disk. This page explains, plainly,
**what is encrypted, what is not, and why** — so you can make an informed choice
about syncing personal data across machines.

For the full threat model and design rationale, see
[ADR-019](https://github.com/kafkade/fond/blob/main/docs/adr/019-encryption-at-rest.md).

## The short version

| Data | At-rest protection |
|------|--------------------|
| `fond.db` (the SQLite index) | **OS full-disk encryption** — derived & rebuildable |
| `.cook` recipe files | **OS full-disk encryption** — plaintext by design (you own them) |
| `photos/` | **OS full-disk encryption** |
| **Authored-overlay sidecar** (the sync payload) | **OS disk encryption + optional app-level encryption** |

fond delegates baseline at-rest protection to your operating system, and adds
**opt-in** encryption for the one surface that is *designed to leave your machine*:
the authored-overlay sidecar.

## Baseline: turn on OS full-disk encryption

Your `fond.db`, `.cook` files, and photos are protected at rest by your operating
system's full-disk encryption. fond does not re-encrypt them locally, so make sure
it is enabled:

- **macOS** — FileVault (System Settings → Privacy & Security → FileVault).
- **Linux** — LUKS (usually offered during installation).
- **Windows** — BitLocker.

This is the single most important step. Without it, anyone with physical access to
the disk can read everything, and no app-level feature changes that.

`fond.db` is a *rebuildable* index (`fond reindex` recreates it from your `.cook`
files) and must never leave the device (see [Syncing](./syncing.md)), so fond
does not encrypt it in-app — the OS mechanism you already control covers it. `.cook`
files are intentionally plaintext so you own and can read them forever with any
tool.

## Optional: encrypt the authored-overlay sidecar

The **authored overlay** — your notes, ratings, cook logs, dietary profiles,
pantry, and meal plans — is the personal data you may want on more than one
machine. To move it between devices you export it to a sidecar
(`fond overlay export`) and sync that alongside your recipes. By default the
sidecar is **plaintext JSONL** (line-diffable, easy to inspect). Over an untrusted
sync channel — a shared server, a cloud folder — that plaintext is readable by
anyone with file access.

When you want that data to travel confidentially, encrypt the export:

```bash
# Keychain-backed key (default): the key is generated once and stored in your
# OS keychain. Nothing to remember; other devices need the same key.
fond overlay export --encrypt

# Passphrase-backed key: derive the key from a passphrase (Argon2id). Any device
# with the passphrase can decrypt — good for cross-machine sync.
fond overlay export --encrypt --passphrase
```

This writes a single sealed bundle, `overlay/authored-overlay.fenc`, using
**XChaCha20-Poly1305** authenticated encryption. Import is transparent — fond
detects the sealed bundle and decrypts it:

```bash
fond overlay import          # auto-detects and decrypts the .fenc bundle
fond overlay status          # shows whether the overlay is encrypted, and the key mode
```

### How it behaves

- **Fail closed.** A missing or wrong key (or a tampered/corrupted bundle) makes
  import **error out and write nothing** — there is never a silent fall back to
  plaintext. `--encrypt` also refuses to write a plaintext export if it cannot get
  a key.
- **Passphrase for non-interactive use.** Set `FOND_OVERLAY_PASSPHRASE` to supply
  the passphrase without a prompt (CI, scripts, headless machines).
- **`fond reindex` stays non-interactive.** It will silently decrypt a
  keychain-keyed bundle, but it **skips a passphrase-keyed bundle** (printing a
  hint) rather than blocking on a prompt — run `fond overlay import` yourself for
  those.
- **Encrypted means not diffable.** Encryption necessarily replaces the
  line-by-line JSONL layout with one sealed blob. If you value plaintext
  diffability more than confidentiality, keep the default. `fond overlay status`
  always shows which mode is active.

## What this does *not* protect

Being honest about the limits (full detail in
[ADR-019](https://github.com/kafkade/fond/blob/main/docs/adr/019-encryption-at-rest.md)):

- **A compromised device with the key present.** App-level encryption cannot
  defend a running machine that already holds the key. That is what OS disk
  encryption and basic device hygiene are for.
- **`fond.db` / `.cook` / photos beyond the OS layer.** fond relies on full-disk
  encryption for these; it does not add a second in-app layer.
- **Size and existence metadata.** The sealed bundle hides its contents, not the
  fact that an overlay exists or roughly how large it is.
- **Losing the key = losing that sidecar.** If you forget the passphrase or lose
  the keychain entry, the encrypted bundle cannot be recovered — by design. Your
  plaintext `.cook` recipe files remain the durable source of truth regardless.
