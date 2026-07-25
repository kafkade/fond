# ADR-020: On-device-first identity & the zero-knowledge account model

**Status**: Proposed
**Date**: 2026-07-24
**Decision**: Establish a **local, two-secret keyset** (passphrase + device-generated Secret Key →
Master Unlock Key → wrapped **household** Vault Key) created **lazily** on first encrypted-export or
sync opt-in — **not** an account on first run. An "account" is created only later, when the user
binds the keyset to a server (ADR-021) by attaching a PAKE verifier. Moving already-encrypted local
data to a server requires a one-time migration from today's single-key **`FONDENC1`** envelope to a
versioned **`FONDENC2`** key hierarchy (there is no key hierarchy today — see Context), after which
further transfers need no re-encryption. This keeps the README's *"no accounts, no cloud"* promise
for the default experience while making fond zero-knowledge-ready for optional sync.
**This ADR is contingent on the Epic A0 protocol spec + independent crypto review (tracked in the
ZK-Sync epic; see GitHub milestone *Zero-Knowledge Sync*);
no crypto code lands before that sign-off.** Extends ADR-005 (identity); **supersedes the single-key
model of ADR-019** by introducing a key hierarchy and `FONDENC2`.

## Context

fond is local-first with `.cook` files as the source of truth (ADR-002) and today has **no accounts
and no auth** (ADR-005 defers identity; README: *"No accounts. No cloud dependency."*). ADR-019
added opt-in symmetric encryption of the authored-overlay sidecar. **Ground truth from
`crates/fond-store/src/crypto.rs`:** `seal_bundle`/`open_bundle` seal one whole `OverlayBundle` under
a **single flat key** (raw keychain key *or* Argon2id-from-passphrase) in a `FONDENC1` envelope.
There is **no** Vault Key, no wrapped-key hierarchy, and no per-object encryption today. Two
consequences follow: (a) "no re-encryption when moving to a server" is **false as-is** — a hierarchy
must be introduced first (`FONDENC2`); (b) `open_bundle` reads Argon2 cost parameters from the
envelope header and derives **before** authenticating, a pre-auth resource-exhaustion vector when the
envelope comes from an untrusted server.

The product now wants an **optional** path to sync data across devices through a server (ADR-021),
with every synced byte end-to-end encrypted 1Password-style so no server operator can read it. That
raises a paradox: the user should be able to "create an account," but there may be no server — ever.
Where does the account live, and how do we avoid breaking the local-first, no-account promise?

The resolution must also honor **Principle #3 (family-shared)**: a household has multiple members,
so the vault must be a **multi-member** vault (per-member wrapped Vault Key, device enrollment,
revocation), not a single-person keyset. `user_id` (ADR-005) is a DB row, not a cryptographic
identity, and does not satisfy this on its own.

## Threat model

**In scope:** an attacker who later gains access to a sync server or its backups (ADR-021) must
learn nothing about recipe content, notes, or photos. Offline brute-force of a stolen server
verifier must be infeasible.

**Out of scope:** a compromised local device with keys available (OS/device hygiene, ADR-019); the
plaintext `.cook` files at rest (OS full-disk encryption, ADR-019). Losing **either** the passphrase
**or** the Secret Key is unrecoverable **by design** (both are required to derive the MUK) — mitigated
by the Emergency Kit and, *for recipes that still exist on a surviving device*, by the plaintext
`.cook` ownership backstop. The backstop does **not** recover overlay data, server-only photos, or
deleted history.

## Decision

### Identity ≠ account

- **Identity** is local and always available: a keyset that can encrypt data on-device.
- **Account** is remote and lazy: a keyset bound to a server with an auth verifier (ADR-021).

### The local keyset (two-secret model)

1. **Secret Key** — a random, high-entropy, device-generated secret. Stored in the OS keychain and
   surfaced once in an **Emergency Kit**. **Never leaves the device; never sent to any server.**
2. **Passphrase** — user-chosen; held in memory only; never stored; never sent.
3. **Master Unlock Key (MUK)** = `Argon2id(passphrase, secret=Secret Key, salt, pinned params)`.
   Never stored; re-derived on unlock. KDF profiles are **pinned/versioned**; parameters are bounded
   and authenticated **before** derivation (closes the `open_bundle` pre-auth DoS). Encodings and
   domain separation for the two secret inputs are fixed by the `FONDENC2` spec.
4. **Vault Key** — a random data key. In a household it is wrapped **once per member** (`wrapped_vault_key[member]`)
   so each member unwraps it with their own MUK; members can be enrolled and revoked without
   re-encrypting data. Purpose/epoch **subkeys** and **per-object DEKs** derive from the Vault Key, so
   a passphrase change re-wraps **one** key and key **rotation/revocation** is possible per epoch.

### Lazy creation (default = no account, encrypts nothing)

- `fond init` creates **no keyset and no account** and prints nothing about accounts. The default
  encrypted-overlay path (ADR-019) may continue to use the keychain key.
- The two-secret keyset + Emergency Kit are generated **just-in-time** the first time the user
  enables passphrase-based encrypted export or runs `fond sync setup` (ADR-021).

### Binding to a server (account is born) — see ADR-021

- Registration uses a modern **aPAKE — OPAQUE (RFC 9807) preferred**, SRP-6a only as a reviewed
  fallback (`[Validation Required]`; never hand-rolled). The server receives an OPAQUE registration
  record and public salts — never the passphrase, Secret Key, or MUK. **Service authentication is
  separated from vault authorization:** destructive or key-changing operations require a **vault-key
  signature the server cannot forge**, so a reset of the login layer never authorizes vault
  destruction.
- The `FONDENC1 → FONDENC2` migration runs once (decrypt bundle under the old flat key, split into
  per-object blobs, encrypt under Vault-Key-derived DEKs). After that, `wrapped_vault_key[member]` is
  uploaded and subsequent transfers upload already-encrypted blobs with **no re-encryption**.
- A second device logs in with email + passphrase + Secret Key (Emergency Kit / keychain export),
  proves knowledge via OPAQUE, downloads its `wrapped_vault_key`, re-derives the MUK, unwraps the
  Vault Key, pulls and decrypts blobs, verifies the signed anti-rollback manifest (ADR-021), and
  rebuilds `fond.db` via `reindex`.

### Emergency Kit & recovery

`fond identity emergency-kit` produces a printable artifact holding the Secret Key + sign-in URL.
Passphrase known **and** Secret Key available → full recovery. **Either** lost → the encrypted blob
store is unrecoverable; local plaintext `.cook` files and file-sync copies survive for recipes still
present on a device (ownership backstop), but overlay data, server-only photos, and deleted history
do not. A recovery **verification drill** (prove a restore actually works) is required before relying
on the Kit.

## Rationale

- **Keeps the promise both ways:** no account by default (README/Principle #1) *and* a real path to
  encrypted sync when wanted.
- **Lossless transfer after one migration:** the `FONDENC1 → FONDENC2` step is one-time; thereafter,
  because the Vault Key exists locally, uploading to a server never re-encrypts.
- **Two-secret strength (server-breach case):** a stolen server verifier is useless without the
  never-uploaded Secret Key, defeating **server-side** offline attacks that single-passphrase schemes
  suffer. It does **not** defend against local device compromise.
- **Family-shared:** the multi-member wrapped Vault Key satisfies Principle #3 without re-encrypting
  data when members change.
- **Reuses existing primitives** (XChaCha20-Poly1305, Argon2id, `zeroize`, keychain) but **composes a
  new protocol** — hence the mandatory Epic A0 independent review.

## Alternatives Considered

| Alternative | Rejected because |
|---|---|
| Create a real account (email+password) on first run | No server exists; breaks *"No accounts"*; forces a cloud-shaped concept on a local-first tool. |
| No identity until sync; derive keys fresh at sync time | Can't retroactively encrypt an existing local overlay under the same key; forces re-encryption and two key regimes. |
| Single passphrase, no Secret Key | Weaker: server breach + weak-passphrase offline attack compromises vaults. |
| Single-member vault (one keyset) | Fails Principle #3 (family-shared); needs a per-member wrapped Vault Key. |
| SRP-6a as the primary PAKE | A stolen SRP verifier enables offline guessing; OPAQUE (RFC 9807) is the modern aPAKE. SRP kept only as a reviewed fallback. |
| Store MUK/Secret Key on the server "for convenience" | Destroys zero-knowledge; forbidden by ADR-019. |
| Ship the key hierarchy in 1.1 before the sync use case exists | Bakes in an abstraction over both FONDENC1 modes as a migration trap; deferred behind the A0 spec + review. |
| Mandatory keyset at `init` | Breaks the frictionless local default and the no-account promise. |

## Consequences

- New `FONDENC2` key-hierarchy layer replacing the flat-key `crypto.rs` path; new `fond identity`
  command surface; Emergency Kit generator; one-time FONDENC1 migration. Passphrase-change re-wraps
  one key; member add/revoke re-wraps one key.
- **Gated on Epic A0:** no crypto/sync code merges before the protocol spec is independently reviewed
  with published test vectors.
- **Honest cost:** losing *either* secret is unrecoverable; documented prominently, mitigated by the
  Kit, a verification drill, and the plaintext ownership backstop (recipes only).
- Enables ADR-021 (sync server) and ADR-023 (backup) to operate on encrypted blobs.
- ADR-013's "stable data model → 1.0" gate should be revisited for the identity/recipe-UUID columns
  this implies (tracked as issue F2/A4).
- No CI / `kafkade/github-infra` change from this ADR alone (new deps only, per ADR-019 precedent).
