# What fond protects

fond is local-first: your data lives on your own disk. **Your first run creates no
account and encrypts nothing** — fond encrypts only when you opt in, and optional
**end-to-end encrypted sync** (planned) protects data *in transit and at rest on the
remote*, never your local plaintext working copy. This page explains, plainly,
**what is and isn't encrypted, and why** — so you can make an informed choice about
syncing personal data across machines.

For the full threat model and design rationale, see
[ADR-019](https://github.com/kafkade/fond/blob/main/docs/adr/019-encryption-at-rest.md)
(overlay encryption at rest) and
[ADR-020](https://github.com/kafkade/fond/blob/main/docs/adr/020-zero-knowledge-identity.md)
(the account model and two-secret sync design).

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

## What is and isn't encrypted

Be clear-eyed about what lives in plaintext. Today, **everything fond works with on
your device is plaintext** — protected only by your OS full-disk encryption (below):

- `.cook` recipe files — plaintext by design; you own them and can read them forever.
- `fond.db` — the SQLite index and overlay (ratings, notes, pantry, meal plans, cook logs).
- The **FTS5 full-text search index** inside `fond.db`.
- SQLite's **write-ahead log and shared-memory** sidecars (`fond.db-wal`, `fond.db-shm`).
- **Temporary files** written during import, export, and reindex.
- **In-memory** working data while fond is running.
- Photo **EXIF metadata** (camera, timestamps, and any embedded GPS location).
- Generated **thumbnails**.

None of these is encrypted by fond itself. When app-level encryption ships, it protects
the data that **leaves** your machine — **end-to-end during sync and at rest on the
remote** — **not** your local working copy. Locally, your defense is OS full-disk
encryption plus basic device hygiene (see below). The one exception you can turn on
today is the authored-overlay *sidecar* you export for syncing, covered further down.

## If you lose a secret

When optional encrypted sync ships, it will use a **two-secret model**
([ADR-020](https://github.com/kafkade/fond/blob/main/docs/adr/020-zero-knowledge-identity.md)),
like 1Password: a **passphrase** you choose plus a device-generated **Secret Key**.
Both are required to unlock your encrypted vault, and **losing *either* one loses the
encrypted vault** — there is no backdoor and no operator who can reset it for you.

The only backstop is ownership: the plaintext `.cook` files still present on a
surviving device. That recovers **recipes that still exist on that device — and
nothing else.** It does **not** recover your photos, your authored overlay (notes,
ratings, cook logs, pantry, meal plans), or anything that lived **only** in a lost
encrypted remote. Keep the Secret Key somewhere safe (a printed Emergency Kit) and
prove a restore actually works before you rely on it.

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
