# ADR-019: Encryption at rest — threat model and the encrypted overlay sidecar

**Status**: Accepted
**Date**: 2026-07-04
**Decision**: Rely on **OS-level full-disk encryption** as fond's baseline
at-rest protection for the derived database, `.cook` files, and photos, and add
**opt-in, authenticated, symmetric encryption for the authored-overlay sidecar**
(ADR-015) so personal data can cross untrusted file-sync safely. The key is
user-controlled — an OS-keychain key (default) or a passphrase (Argon2id) — and
is never hardcoded or fetched from the network. A missing or wrong key **fails
closed**: no plaintext personal data is ever written.

## Context

Before this ADR fond had **no encryption at rest anywhere**. Two surfaces matter:

- **`fond.db`** — the SQLite overlay/index. It is *derived and rebuildable*
  (`fond reindex` reconstructs it from `.cook` files + bundled reference data,
  per ADR-002). It is device-specific and must **never** be synced (ADR-012).
- **The authored-overlay sidecar** (ADR-015) — `notes`, `ratings`, `cook_logs`,
  dietary `profiles`, `pantry`, and `meal_plans` exported as **plaintext JSONL**.
  This is the Tier 2 sync payload, and it is *designed to travel* over a
  user-controlled file-sync channel (Syncthing/Dropbox/iCloud). Plaintext there
  means anyone with file access — a shared server, a cloud provider, another
  account on the machine — can read personal data.

`.cook` recipe files are intentionally plaintext for ownership and portability
(ADR-002); recipe *content* is not the concern. The concern is the **authored
personal overlay**.

Constraints from the product principles: any cryptography must be **local-first**
(#1) and preserve **data ownership** (#2) — user-controlled, offline, no
mandatory cloud, no keys-as-a-service, no vendor lock-in.

## Threat model

**In scope — what this ADR addresses:**

- An attacker who can **read files on the sync channel or a shared host** (a
  home server, a cloud folder, a sync relay, a backup) but does **not** have the
  key. Against them, the encrypted sidecar is confidential and tamper-evident.
- Accidental exposure of personal data through routine multi-device sync.

**Out of scope — honestly stated limitations:**

- **A compromised local device with the key available** (malware running as the
  user, an unlocked machine). App-level encryption cannot defend the running
  system; this is what OS disk encryption + device hygiene are for.
- **`fond.db` and `.cook` files at rest.** These are protected by **OS-level
  full-disk encryption** (FileVault / LUKS / BitLocker), not by fond. This is a
  deliberate delegation, not an oversight: encrypting a rebuildable local index
  in-app adds key-management complexity for little gain over the OS mechanism the
  user already controls.
- **Metadata / size.** The sealed bundle reveals its own size and the fact that
  an overlay exists; it does not hide how much authored data you have.
- **Deletion propagation.** Unchanged from ADR-015 — encryption is a transport
  concern layered under the identical merge engine.

## Decision

### Baseline: delegate DB / files / photos to the OS

Document, clearly and honestly, that at-rest protection for `fond.db`, `.cook`
files, and photos is provided by **OS full-disk encryption**, which fond
recommends enabling. fond does not re-encrypt these locally.

### Opt-in encrypted overlay sidecar

Add a second overlay codec alongside the plaintext JSONL layout: a single
**sealed bundle** file, `authored-overlay.fenc`, that carries the entire
authored overlay confidentially.

- **AEAD**: XChaCha20-Poly1305. The 24-byte random nonce makes per-file random
  nonces safe. The full envelope header (magic, version, key mode, KDF
  parameters, nonce) is authenticated as associated data, so tampering or
  truncation fails the open.
- **Key sources** (mutually exclusive, recorded in the header):
  - **Keychain (default)** — a random 32-byte key stored in the OS keychain
    (reusing the `keyring-core` dependency already in the tree). Generated and
    stored on first encrypted export.
  - **Passphrase** — a key derived with **Argon2id** from a user passphrase and
    a per-file random salt; salt and cost parameters live in the header so any
    device with the passphrase can re-derive. Supplied interactively (hidden
    prompt) or via `FOND_OVERLAY_PASSPHRASE` for headless/CI use.
- **Fail closed**: a missing key, wrong key/passphrase, key-mode mismatch, or
  any corruption yields an error and decodes nothing. `--encrypt` likewise
  refuses to fall back to plaintext when a key cannot be obtained.

### Envelope format (`FONDENC1`)

```text
magic  "FONDENC1"  (8 bytes)
version            (1 byte)
key_mode           (1 byte: 0 = keychain, 1 = passphrase)
── passphrase mode only ──
  salt             (16 bytes)
  argon2 m_cost    (u32 LE, KiB)
  argon2 t_cost    (u32 LE, iterations)
  argon2 p_cost    (u32 LE, lanes)
──────────────────────────
nonce              (24 bytes)
ciphertext         (AEAD over JSON(OverlayBundle), AAD = the header above)
```

Encryption inherently gives up the JSONL layout's line-oriented diffability, so
the encrypted export is **one sealed bundle** rather than per-line files — a
documented trade-off (confidentiality over diffability). `fond overlay status`
surfaces the mode so the choice is never hidden.

### Surface

- `fond overlay export --encrypt [--passphrase]` — write the sealed bundle;
  refuses to write plaintext if the key is unavailable; warns if stale plaintext
  sidecars still sit beside it.
- `fond overlay import` — auto-detects the sealed bundle, reads its mode, and
  decrypts transparently; missing/wrong key fails closed. Falls back to the
  plaintext JSONL layout only when no sealed bundle is present.
- `fond overlay status` — reports whether the overlay is encrypted and the key
  mode.
- `fond reindex` auto-import — silently decrypts a **keychain**-keyed bundle (the
  OS may prompt to unlock the keychain) but **skips a passphrase-keyed bundle
  with a hint**, so reindex never blocks on an interactive prompt. Fail-closed is
  preserved throughout.

### Architecture

A new, deliberately **platform-free** `fond-store::crypto` module owns the
envelope, AEAD, and KDF; it takes raw key material and never touches the keychain
or a terminal. The `fond` binary owns key acquisition (keychain get-or-create via
the existing `CredentialStore`, or a passphrase prompt/env). The overlay codec is
refactored around a shared in-memory `OverlayBundle` (`collect_bundle` /
`apply_bundle`) so the plaintext and encrypted paths funnel through **one merge
engine** — merge semantics (ADR-015) are identical regardless of transport.

## Alternatives considered

| Alternative | Rejected / Deferred because |
|-------------|-----------------------------|
| **Encrypt `fond.db` in-app (e.g. SQLCipher)** | It is a rebuildable, never-synced local index; OS disk encryption already covers it. Adds key management + a heavier SQLite build for little gain. **Delegated to the OS.** |
| **Encrypt `.cook` files** | Breaks the plaintext ownership/portability guarantee (ADR-002). Recipe content is not the sensitive surface. **Rejected.** |
| **Per-file encryption of each JSONL sidecar** | Encryption already destroys diffability; N sealed files add complexity with no benefit over one sealed bundle. **Rejected.** |
| **AES-256-GCM** | Fine, but its 96-bit nonce makes random-nonce reuse a real risk without a counter. XChaCha20-Poly1305's 192-bit nonce is safe with random nonces and simpler to use correctly. **Chose XChaCha20-Poly1305.** |
| **Mandatory encryption** | Violates local-first simplicity and the plaintext-diffable default many households want. **Opt-in.** |
| **A hosted key service** | Violates principles #1/#2 (no mandatory cloud, no keys-as-a-service). **Rejected.** |

## Consequences

- New `fond-store::crypto` module and sealed-bundle codec; new `--encrypt` /
  `--passphrase` flags on `fond overlay export`; transparent decrypt on import,
  status, and reindex. New crate dependencies: `chacha20poly1305`, `argon2`,
  `zeroize` (store) and `rpassword` (CLI).
- **No CI / `kafkade/github-infra` change**: no new required checks and no job
  renames — the single `CI` gate is unaffected; only new dependencies are added.
- Losing the key means losing the ability to decrypt that sidecar — by design.
  The keychain-created key prints a one-time notice; passphrase mode's confirm
  prompt guards against typos. The plaintext `.cook` files remain the durable
  source of truth regardless.
- The encrypted bundle is not human-diffable; users who value diffability keep
  the plaintext default. `status` makes the active mode explicit.
