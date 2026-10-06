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
   by a compiled allowlist **before** derivation (closes the `open_bundle` pre-auth DoS) and
   additionally authenticated by the wrap AEAD tag **after** derivation. Encodings and domain
   separation for the two secret inputs are fixed by the `FONDENC2` spec.
4. **Vault Key** — a random data key. In a household each member holds a **self-wrapped stable
   package** (`wrapped_stable_package[member]`, holding the shared object-id namespace key + identity
   seed) under their own KEK, and receives the per-epoch **Vault Key** through an **authenticated
   HPKE grant** sealed to their identity key (so an admin can rotate keys to remaining members using
   only public keys). Members can be enrolled and revoked without re-encrypting data. Purpose/epoch
   **subkeys** and **per-object DEKs** derive from the Vault Key, so a passphrase change re-wraps
   **one** package (no Vault Key inside) and key **rotation/revocation** is possible per epoch. The
   concrete construction — hierarchy, wire format, per-member wrapping, epoch grants, and rotation —
   is in the **Appendix: FONDENC2 protocol** below.

### Lazy creation (default = no account, encrypts nothing)

- `fond init` creates **no keyset and no account** and prints nothing about accounts. The default
  encrypted-overlay path (ADR-019) may continue to use the keychain key.
- The two-secret keyset + Emergency Kit are generated **just-in-time** the first time the user
  enables passphrase-based encrypted export or runs `fond sync setup` (ADR-021).

### Binding to a server (account is born) — see ADR-021

- Registration uses a modern **aPAKE — OPAQUE (RFC 9807)**. There is **no SRP-6a fallback**: a
  negotiated or availability-triggered fallback would be a downgrade path, and the candidate SRP
  crate is unaudited (A0.5 VR-020-D.1 / I.15). If a reviewed OPAQUE implementation is unavailable,
  **sync stays disabled** rather than downgrading authentication. The server receives an OPAQUE
  registration record and public salts — never the passphrase, Secret Key, or MUK. **Service
  authentication is separated from vault authorization:** destructive or key-changing operations
  require a **vault-key signature the server cannot forge**, so a reset of the login layer never
  authorizes vault destruction. See [ADR-021.2](021-optional-sync-server.md#appendix-account-authentication--pake-selection-adr-0212)
  for the PAKE selection and the two-layer auth-vs-vault-authorization boundary.
- The `FONDENC1 → FONDENC2` migration runs once (decrypt bundle under the old flat key, split into
  per-object blobs, encrypt under Vault-Key-derived DEKs). After that, the member's
  `wrapped_stable_package` and epoch `vk_grant` are uploaded and subsequent transfers upload
  already-encrypted blobs with **no re-encryption**.
- A second device logs in with email + passphrase + Secret Key (Emergency Kit / keychain export),
  proves knowledge via OPAQUE, downloads its `wrapped_stable_package`, re-derives the MUK, unwraps
  the stable package (object-id key + identity seed), HPKE-opens its epoch `vk_grant` (Vault Key),
  pulls and decrypts blobs, verifies the signed anti-rollback manifest (ADR-021), and
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
| SRP-6a as the primary PAKE | A stolen SRP verifier enables offline guessing; OPAQUE (RFC 9807) is the modern aPAKE. **No SRP fallback** — a negotiated fallback is a downgrade path and the crate is unaudited (A0.5). |
| Store MUK/Secret Key on the server "for convenience" | Destroys zero-knowledge; forbidden by ADR-019. |
| Ship the key hierarchy in 1.1 before the sync use case exists | Bakes in an abstraction over both FONDENC1 modes as a migration trap; deferred behind the A0 spec + review. |
| Mandatory keyset at `init` | Breaks the frictionless local default and the no-account promise. |

## Consequences

- New `FONDENC2` key-hierarchy layer replacing the flat-key `crypto.rs` path; new `fond identity`
  command surface; Emergency Kit generator; one-time FONDENC1 migration. Passphrase-change re-wraps
  one stable package; **member addition** creates the new member's stable wrap and re-grants the
  current VK to all target members under the new roster core (existing stable wraps untouched);
  **revocation** bumps the epoch and issues one **HPKE Vault-Key grant**
  per remaining member (never re-wrapping their secrets, never the data). The wire format and
  hierarchy are specified in the **Appendix: FONDENC2 protocol**.
- **Gated on Epic A0:** no crypto/sync code merges before the protocol spec is independently reviewed
  with published test vectors.
- **Honest cost:** losing *either* secret is unrecoverable; documented prominently, mitigated by the
  Kit, a verification drill, and the plaintext ownership backstop (recipes only).
- Enables ADR-021 (sync server) and ADR-023 (backup) to operate on encrypted blobs.
- ADR-013's "stable data model → 1.0" gate should be revisited for the identity/recipe-UUID columns
  this implies (tracked as issue F2/A4).
- No CI / `kafkade/github-infra` change from this ADR alone (new deps only, per ADR-019 precedent).

## Appendix: FONDENC2 protocol

This appendix specifies the concrete `FONDENC2` vault protocol that the Decision section
references but does not construct. It replaces the single flat-key `FONDENC1` envelope
(described in Context and implemented in `crates/fond-store/src/crypto.rs`) with a versioned
**key hierarchy**. Like [ADR-023's `FONDBKP1` appendix](023-backup-and-recovery.md#appendix-fondbkp1-wire-format-adr-0231),
the authoritative byte-level specification will live as module documentation in
`crates/fond-store/src/crypto.rs` **once implemented**; this appendix records the shape so the
ADR is self-contained. FONDENC2 **reuses the underlying XChaCha20-Poly1305 AEAD call** that
`seal_blob`/`open_blob` wrap, **not** the FONDENC1 envelope parser: the FONDENC1 blob sub-header
carries a key-mode byte and free-form Argon2 parameters that a FONDENC2 object expressly must not
(§D), so FONDENC2 defines its own envelope (§E) over the same reviewed AEAD primitive. Only the
cryptographic core is shared; the framing is new.

**Gate reminder:** this is a paper spec. **No crypto/sync code lands before the Epic A0
independent review (A0.5) clears**, with published test vectors. Every `[Validation Required]`
tag below marks a choice the A0.5 reviewer must sign off on; unresolved decisions are collected
in section K rather than decided silently.

### A0.5 remediation-mapping table (this revision)

The A0.5 adversarial review ([`docs/reviews/a05-fondenc2-adversarial-review.md`](../reviews/a05-fondenc2-adversarial-review.md))
returned a **NO-GO** and enumerated 6 structural blockers, findings N-01..N-14, and a 55-row
adjudication. A **round-2 re-review**
([`docs/reviews/a05-fondenc2-adversarial-review-round2.md`](../reviews/a05-fondenc2-adversarial-review-round2.md))
also returned **NO-GO** (24 RESOLVED / 15 PARTIALLY / 16 NOT-RESOLVED) and raised findings
N-15..N-32. The subsequent immutable
[round-3 review](../reviews/a05-fondenc2-adversarial-review-round3.md) returned **NO-GO**
(26 RESOLVED / 15 PARTIALLY / 14 NOT-RESOLVED), identifying **N-33**, the HPKE grant/full-directory
hash fixed point, and reopening the dependent rotation/recovery claims.

**Post-review human handoff (N-33 only).** This revision separates a byte-defined, grant-free
`roster_core_hash` from the full completed-directory `roster_hash` (§G). The structural N-33 cycle
has a **specified correction; pending human validation**. This post-review change has **NOT been
independently cryptographically reviewed**; it neither changes the historical review/hashes nor
promotes N-15/N-01/K.2/K.12/K.16 to universally resolved. The round-3
[primary residual-human checklist](../reviews/a05-fondenc2-adversarial-review-round3.md#primary-deliverable-residual-human-checklist)
remains the handoff authority, including invitation replay (N-32), the legacy pre-auth bounds
(N-06 / A1), canonical transcripts, roster state machine, and N-21/N-22. The user-directed
model-review loop **freezes after this correction**: no further model review is authorized.
[#120](https://github.com/kafkade/fond/issues/120) remains **OPEN / BLOCKED**, with **NO-GO** for
implementation until an independent human cryptographer explicitly signs off. Coverage for items
landing in **this appendix** (A0.3 items in the [A0.3 table](#a05-remediation-mapping-a03);
ADR-021 items in the ADR-021.1/ADR-021.2
tables):

| Finding | Handled in | Status (affected gates; other rows retain r2 history) |
|---|---|---|
| **N-33** grant/directory fixed point | [§G roster-core commitment](#grant-free-roster-core-commitment-n-33) | **Specified correction; pending human validation** (structural cycle only) |
| **N-15** epoch rotation mechanically impossible | [§G split package + HPKE grant](#g-per-member-key-wrapping--the-roster), [§H rotation](#h-enrollment-roles-invitation-revocation-epoch-rotation) | **Not closed**: N-33-only correction; composition, codec, and state machine `[Validation Required]` |
| **N-18** invitation freshness anchor | [§H invitation](#h-enrollment-roles-invitation-revocation-epoch-rotation) | **Resolved (A0.5-r2)** |
| **N-32** invitation delivers `NS_objectid` | [§H invitation](#h-enrollment-roles-invitation-revocation-epoch-rotation) | **Resolved (A0.5-r2)** |
| N-01 epoch-key archive distribution | [§L epoch-key archive](#l-epoch-key-archive--history-recovery-n-01) | **Not closed**: grant direction specified; archive addressing/commit and recovery `[Validation Required]` |
| N-02 roster key cycle | [§G roster split](#g-per-member-key-wrapping--the-roster) | Partially-resolved (round-2): cycle broken; canonical body / state machine open |
| N-10 identity-key & Kit recovery | [§M identity-key recovery](#m-identity-key-recovery--emergency-kit-n-10-k12) | Resolved (A0.5) |
| N-08 invitee-key substitution | [§H invitation](#h-enrollment-roles-invitation-revocation-epoch-rotation) | Resolved (substitution); freshness completed A0.5-r2 (N-18) |
| N-14 sealed-box primitive analogy | [§H invitation](#h-enrollment-roles-invitation-revocation-epoch-rotation), [§J](#j-primitives) | Resolved (A0.5): HPKE Base-mode |
| N-13 object-id collision strength | [§E envelope](#e-envelope--wire-format-per-object) | Resolved (width); canonical `len_prefix` `[Validation Required]` |
| N-28 Kit vs two-secret-loss contradiction | [§M](#m-identity-key-recovery--emergency-kit-n-10-k12) | **Resolved (A0.5-r2)** |
| K.1 MUK two-secret binding | [§C](#c-domain-separation--the-two-secret-muk) | Decided (direction); two-secret KAT `[Validation Required]` |
| K.2 wrap construction | [§G](#g-per-member-key-wrapping--the-roster) | **Partially-resolved**: self-wrap + HPKE grant selected; core/info/AAD bytes scoped here, signing bytes/composition and stable-wrap codec `[Validation Required]` |
| K.3 invitation transport | [§H](#h-enrollment-roles-invitation-revocation-epoch-rotation) | Decided: HPKE + signed transcript; +`NS_objectid`/anchor (A0.5-r2); canonical bytes `[Validation Required]` |
| K.4 subkey/DEK KDF | [§B](#b-key-hierarchy), [§F](#f-per-object-dek-derivation--object-granularity) | Decided: HKDF-SHA-256; KEK Extract salt / raw-vs-length-prefixed `[Validation Required]` |
| K.5 nonce strategy | [§E](#e-envelope--wire-format-per-object) | Resolved: keep random |
| K.6 object granularity | [§F](#f-per-object-dek-derivation--object-granularity) | Resolved |
| K.7 per-device keys | [§H](#h-enrollment-roles-invitation-revocation-epoch-rotation), [§M](#m-identity-key-recovery--emergency-kit-n-10-k12) | Partially-resolved (round-2): certificate transcript incomplete |
| K.8 roster signer model | [§G](#g-per-member-key-wrapping--the-roster) | Partially-resolved (round-2): canonical signed body / transition semantics open |
| K.11 object-id source & width | [§E](#e-envelope--wire-format-per-object), [§F](#f-per-object-dek-derivation--object-granularity) | Partially-resolved (round-2): `len_prefix` / class taxonomy open |
| K.12 Emergency Kit / recovery | [§M](#m-identity-key-recovery--emergency-kit-n-10-k12) | **Not closed**: recovery direction specified; authenticated grant path/full-loss freshness `[Validation Required]`; N-28 loss clarification retained |
| K.13 Argon2 figures/budget | [A0.3 registry](#argon2id-profile-registry) | Not-resolved / deferred (human + measurement) |

### New design decisions since A0.5 (not in the original review — scrutinize these)

The mapping table above answers *"where did each review finding go?"*. This block answers the
second, higher-risk axis: *"what did this revision introduce that the A0.5 review never saw?"* **The
A0.5 review did not see these; they are the highest-risk part of this revision for independent
human scrutiny.** The round-3 artifact records later model scrutiny; item 8 is a post-review
spec change, not human sign-off. Mechanics live in the linked sections.

1. **Forward-chained epoch-key archive** — `archive[e] = VK_e` sealed under a subkey of `VK_{e+1}`, so
   any current-VK holder walks back to `VK_0`; no separate archive-root key. → [§L](#l-epoch-key-archive--history-recovery-n-01)
2. **New members get full history by default; the history barrier is household-wide, not
   member-selective** — omitting `archive[b-1]` severs the chain for everyone on the current VK;
   truly per-member history restriction (segment-root wraps) is deferred. → [§L](#l-epoch-key-archive--history-recovery-n-01)
3. **Signed-but-cleartext roster/wrap directory** — authenticated by the admin signature, not sealed
   under any VK; concedes member **count/roles/pubkeys** as server-visible metadata to break the N-02
   key cycle. → [§G](#g-per-member-key-wrapping--the-roster), cross-ref [ADR-021.1 §G honest limits](021-optional-sync-server.md#g-honest-limits--what-this-cannot-do)
4. **Split member key material: stable self-wrap + per-epoch HPKE grant (A0.5-r2, replaces the
   round-1 unified package).** Round-1 wrapped `{current_Vault_Key ‖ NS_objectid ‖ identity_seed}`
   once under a symmetric KEK, which made admin-driven rotation impossible (N-15). It is now split by
   rotation behaviour: a **stable self-wrapped package** `{NS_objectid ‖ identity_seed}` (only the
   member re-wraps it, on passphrase change) plus a **per-epoch HPKE Vault-Key grant** that an admin
   seals to each member's X25519 identity public key using only public info. Rationale: an admin can
   (re-)grant `VK_{e+1}` to remaining members without any other member's KEK — the mechanism rotation
   requires. → [§G](#g-per-member-key-wrapping--the-roster), [§H](#h-enrollment-roles-invitation-revocation-epoch-rotation)
5. **Authenticated multi-parent DAG + signed merges + CAS + signed frontier (monotonic head-counter);
   per-device append-only op-log replacing scalar `own_counter`** — the history topology and
   anti-rollback state. → [ADR-021.1 §D](021-optional-sync-server.md#d-authenticated-history-topology-dag-cas--signed-head) / [§E](021-optional-sync-server.md#e-rollback--fork--equivocation-detection)
6. **Member identity keys from a random `identity_seed` in the stable self-wrapped package** — stable
   across passphrase change and recoverable by unwrapping it; chosen over deterministic-from-MUK,
   which would rotate identity (and invalidate device certs / the account sidecar) on every passphrase
   change. → [§M](#m-identity-key-recovery--emergency-kit-n-10-k12)
7. **The member X25519 identity key now also gates epoch access (A0.5-r2 blast-radius; scrutinize).**
   Because `vk_grant`s are HPKE-sealed to `member_x25519` (from `identity_seed`), compromise of
   `identity_seed` now exposes every granted `VK_e` and thus all content of those epochs — a genuine
   widening versus round-1, where epoch access was gated only by the KEK (passphrase + Secret Key).
   `identity_seed` still lives only inside the KEK-wrapped stable package, so compromise still requires
   breaking that wrap or capturing the seed in memory; and an admin gains no new plaintext power (it
   already holds `VK_e`). The trade-off is the price of making N-15 rotation mechanically possible.
   → [§G blast radius](#g-per-member-key-wrapping--the-roster)
8. **Grant-free target roster core, distinct from the full directory (post-round-3 N-33).** Grants
   bind a canonical projection of vault/epoch/predecessor/member identities/roles/public keys, never
   their own outputs. The full directory still commits to actual grants and signatures. Every
   chained successor changes the predecessor-bound core, including same-epoch updates, and requires
   re-grants; stable packages are unchanged except for the member's own explicit re-wrap.
   → [§G core and construction order](#grant-free-roster-core-commitment-n-33).

### A. Design goals

FONDENC2 closes the three gaps a single flat key leaves open, plus the pre-auth DoS:

- **Rotation & revocation** — an **epoch** counter lets the household rotate keys and revoke a
  member without re-encrypting all existing data.
- **Per-member access** — the Vault Key is wrapped **once per member** so each member unwraps
  with their own MUK; membership changes re-wrap one key, never the data (Principle #3).
- **Per-object granularity** — every object gets its own **DEK**, so objects can be added,
  rotated, and synced (ADR-021) independently.
- **No pre-auth key stretching** — object opens run **no** Argon2; the single Argon2 unlock uses
  a pinned, bounded profile, closing the `open_bundle` resource-exhaustion vector.

### B. Key hierarchy

```mermaid
graph TD
    PP["Passphrase (memory only)"] --> MUK["MUK = Argon2id(passphrase, secret = Secret Key, salt, pinned profile)"]
    SK["Secret Key (keychain / Emergency Kit)"] --> MUK
    MUK --> KEK["Member KEK = HKDF(MUK)"]
    KEK -->|unwraps| STABLE["Stable package = NS_objectid ‖ identity_seed (self-wrapped, epoch-invariant)"]
    STABLE --> IDK["Member identity keys (Ed25519 + X25519) from identity_seed (§M)"]
    IDK -->|X25519 opens| VK["Vault Key (random 32-byte household root, per epoch)"]
    GRANT["Per-epoch HPKE VK grant (admin-sealed to member_x25519, §G)"] -->|HPKE-opens| VK
    VK --> SUB["Purpose subkeys = HKDF(Vault Key, epoch, purpose)"]
    SUB --> DEK["Per-object DEKs = HKDF(subkey, object_class, object_id)"]
    DEK --> OBJ["FONDENC2 objects (XChaCha20-Poly1305 AEAD, header as AAD)"]
```

- **L0 — secrets:** the user-chosen **passphrase** (memory only) and the device **Secret Key**
  (keychain / Emergency Kit). Both are required; neither is ever uploaded.
- **L1 — MUK:** `Argon2id` stretches the low-entropy passphrase (section C).
- **L2 — member KEK & self-wrapped stable package:** a fast HKDF of the MUK yields the member's
  key-encryption key, which wraps/unwraps that member's **epoch-invariant stable package**
  (`NS_objectid ‖ identity_seed`) — **not** the Vault Key (section G). The identity keys derived from
  `identity_seed` (§M) are what open the per-epoch Vault-Key grant.
- **L2′ — per-epoch Vault-Key grant:** the household **Vault Key `VK_e` is distributed to each member
  by an authenticated per-member HPKE grant** — an admin seals `VK_e` to the member's X25519 identity
  public key (section G). This is what makes admin-driven **epoch rotation** mechanically possible: an
  admin re-grants `VK_{e+1}` using only public keys, never any other member's KEK/passphrase/Secret
  Key (N-15).
- **L3 — Vault Key:** a random 32-byte household root, one per **epoch**; obtained from the L2′ grant
  (or, on rotation, freshly generated by the admin and re-granted).
- **L4 — purpose subkeys** and **L5 — per-object DEKs:** derived from the Vault Key by HKDF with
  domain-separation labels (section F).

**KDF choice rationale.** `Argon2id` is used **only** at L1, where it stretches a low-entropy
human passphrase. Every derivation **below** the Vault Key takes a **uniformly random**
32-byte input, for which a memory-hard KDF buys nothing; a fast extract-then-expand KDF is the
correct tool. **Decided (A0.5, K.4):** **HKDF-SHA-256** for L2/L4/L5, chosen over keyed BLAKE3 for
standards interoperability and available KATs. Every HKDF-Extract salt and Expand `info`
transcript is explicitly defined (§F), with fixed-width or length-prefixed fields; no delimiter-free
variable concatenation.

### C. Domain separation & the two-secret MUK

- **Label namespace.** All derivation labels are ASCII byte strings prefixed
  `fond/fondenc2/v2/...` so a subkey can never collide across purpose, epoch, protocol, or
  version. The version token (`v2`) is part of every label, binding derived keys to this spec.
- **MUK derivation.** `MUK = Argon2id(password = passphrase, secret = Secret Key, salt = per-vault
  salt, params = PROFILE[kdf_profile_id])`. The two secrets are domain-separated **by
  construction**: the passphrase occupies Argon2's `password` slot and the Secret Key occupies
  Argon2's keyed `secret` (pepper) slot — they are never concatenated into one ambiguous buffer.
- **Passphrase encoding.** UTF-8 with **NFC** normalization, so derivation is deterministic
  across devices and input methods.
- **Decided (A0.5, K.1): use Argon2id's keyed `secret` (pepper) slot** for the fixed 32-byte Secret
  Key, **not** an ad hoc HKDF pre-mix. The decision pins **Argon2 version `0x13`** (the only accepted
  version; any other version is rejected before derivation), the exact low-level API semantics
  (`secret` = the raw 32-byte Secret Key; `password` = `len_prefix("fond/fondenc2/v2/muk") ‖
  NFC(passphrase)` — the passphrase buffer is **length-prefixed with the MUK use-label** so this use
  of the passphrase is domain-separated from the OPAQUE-login use of the same passphrase, which is
  labelled `fond/fondenc2/v2/opaque-ksf`, see [I.16 in ADR-021.2](021-optional-sync-server.md#f-chosen-ciphersuite-parameters--a05-sign-off);
  `salt`, `m/t/p` from `PROFILE[kdf_profile_id]`; 32-byte output), and a cross-implementation
  **Known-Answer Test (KAT)** that every client MUST pass. The raw Argon2id output is the MUK; the
  `fond/fondenc2/v2/kek` label (§G) additionally separates the downstream KEK derivation.

### D. Pinned KDF profiles — the authenticated-params fix

`FONDENC1`'s `open_bundle` reads free-form Argon2 `m/t/p_cost` `u32`s from an **untrusted**
header and runs Argon2id **before** authenticating — a hostile server can set enormous costs
for a pre-auth resource-exhaustion (DoS). FONDENC2 removes this structurally:

1. **Object opens run no Argon2 at all.** Per-object blobs are sealed under Vault-Key-derived
   DEKs (symmetric HKDF, microseconds). Argon2id runs **exactly once per unlock**, on the
   MUK-wrap record — never per object, never on server-supplied blobs.
2. **KDF params are a pinned, versioned profile — not free-form integers.** A single
   `kdf_profile_id` byte selects a bounded parameter set **compiled into the client**
   (`PROFILE[1] = {m_cost, t_cost, p_cost}`, …). An unknown, out-of-range, or (per-platform)
   non-accepted id is rejected **before** any derivation — this **allowlist/bounds check is what runs
   pre-Argon2** and closes the DoS. If raw params are also recorded for auditing, they are bound into
   the MUK-wrap record's AEAD associated data and MUST equal the pinned table entry; note that the
   **AEAD tag can only be verified *after* Argon2 derives the candidate KEK** (A0.3, N-11), so the tag
   authenticates the id/params *after* derivation — the pre-Argon2 guarantee is the compiled
   allowlist/bounds, not the tag.

Net effect: no attacker-controlled input reaches a memory-hard KDF, and no key stretching
happens on the untrusted per-object path.

### E. Envelope / wire format (per-object)

Each object is a self-describing envelope. The cleartext header is authenticated as AEAD
associated data; the DEK is **not** stored (it is re-derived from the Vault Key, epoch, and
object binding).

```text
┌─ FONDENC2 object envelope (cleartext header, authenticated as AEAD AAD) ─┐
│ magic         "FONDENC2"   8 bytes                                       │
│ version       u8           1  (2 = this FONDENC2 revision)               │
│ object_class  u8           0 = overlay, 1 = user-bucket, 2 = photo,      │
│                            3 = manifest, 4 = roster-meta (others reserved)│
│ epoch         u32 LE       Vault-Key epoch that derives the DEK          │
│ object_id     32 bytes     opaque per-object id (binds the DEK); §B/§F   │
│ nonce         24 bytes     XChaCha20 random nonce (CSPRNG, per seal)     │
├─ Ciphertext ──────────────────────────────────────────────────────────────┤
│ AEAD(XChaCha20-Poly1305) over the object plaintext                       │
│   key = DEK = HKDF(subkey_{object_class, epoch}, object_class ‖ object_id)│
│   AAD = the entire cleartext header above (70 bytes)                     │
└────────────────────────────────────────────────────────────────────────────┘
```

- **Header length.** `8 + 1 + 1 + 4 + 32 + 24 = 70 bytes`.
- **AAD binding (integrity of the framing).** Because the whole header — `magic`, `version`,
  `object_class`, `epoch`, `object_id`, `nonce` — is the AEAD associated data, an attacker cannot
  swap an object's `epoch` or `object_class`, or move a valid ciphertext onto a different
  `object_id`, without failing the Poly1305 tag. Envelopes fail **closed** exactly as
  `FONDENC1`/`FONDBKP1` do today.
- **Object-id width — decided (A0.5, N-13 / K.11 / I.1): 32 bytes.** The id is a keyed HMAC-SHA-256
  pseudonym ([ADR-021.1 §B](021-optional-sync-server.md#b-opaque-keyed-object-identifiers)). A
  16-byte truncation gives 128-bit preimage/forgery strength but only **64-bit generic collision**
  strength — insufficient to claim "128-bit collision resistance". The full 32-byte HMAC output is
  used, restoring 128-bit collision strength, so no collision-detection/recovery machinery is
  required.
- **Nonce safety.** XChaCha20-Poly1305's **192-bit** nonce is drawn from the system CSPRNG per
  seal. Because each object has its **own** DEK, the collision budget is **per-DEK**, not global:
  if one DEK re-seals its object `N` times, the birthday probability of a nonce collision is
  ≈ `N² / 2¹⁹³` — e.g. `N = 2³²` rewrites gives ≈ `2⁻¹²⁹`, negligible. This is the standard
  extended-nonce (XSalsa/XChaCha) argument that makes **random** nonces safe **under a sound
  RNG**, without a counter. **Decided (A0.5, K.5): keep pure-random 192-bit nonces, no per-DEK
  counter.** A durable counter adds rollback/crash state and can make reuse *more* likely after
  state loss; the random-nonce bound is already ample per DEK. RNG failure is treated as a
  **platform-fatal error** (seal aborts), not masked by a counter.
- **Version byte.** The `magic` distinguishes formats; the `version` byte tracks this format's
  own revision (`2`, aligned with the magic). `FONDENC1` remains readable via its own magic for
  the one-time migration (section I).

### F. Per-object DEK derivation & object granularity

- **Extract once:** `PRK_vault = HKDF-Extract(salt = "fond/fondenc2/v2/vault", ikm = Vault Key)`.
- **Purpose/epoch subkey:**
  `subkey_{purpose,epoch} = HKDF-Expand(PRK_vault, "fond/fondenc2/v2/subkey" ‖ purpose ‖ epoch_le, 32)`.
- **Per-object DEK:**
  `DEK = HKDF-Expand(subkey_{object_class,epoch}, "fond/fondenc2/v2/dek" ‖ object_class ‖ object_id, 32)`.

Every Extract salt and Expand `info` string above is a fixed ASCII label; multi-field `info`
values are length-prefixed (K.4), never delimiter-free.

Epoch-scoped purpose labels (rooted in the per-epoch `PRK_vault`; domain-separated, non-exhaustive):

| Purpose label | Derives | Consumed by |
|---|---|---|
| `content` | overlay/user-bucket object DEKs | authored overlay (ADR-015) |
| `photo` | photo object DEKs | content-addressed photos |
| `manifest` | manifest MAC/enc key | signed sync manifest (ADR-021) |
| `roster-meta` | optional confidential roster-metadata key | roster metadata (section G) |
| `archive` | epoch-key-archive wrap key (rooted in the **new** epoch's `PRK`, keyed by `epoch_le(e)` of the archived key) | old-epoch Vault-Key archive (section L) |

**Vault-lifetime (epoch-invariant) keys — NOT rooted in `PRK_vault`.** Some material must survive
epoch rotation, so it is **not** derived from the per-epoch Vault Key. It is generated once at vault
creation and distributed through authenticated membership state (§G, inside the member's **stable
self-wrapped package**), independent of `VK_e`:

| Vault-lifetime key | Role | Rotation |
|---|---|---|
| `NS_objectid` | HMAC-SHA-256 object-id namespace key ([ADR-021.1 §B](021-optional-sync-server.md#b-opaque-keyed-object-identifiers)) | epoch-invariant (I.2); rotates only via a coordinated re-id pass |
| `identity_seed` | seed for the member Ed25519/X25519 identity keys (§M) | epoch-invariant; stable across passphrase change |

Rooting `NS_objectid` in a random vault-lifetime secret (not the `object-id` subkey of any `VK_e`)
resolves **I.2**: object ids stay stable across rotation while DEKs stay epoch-scoped. The honest
cost — a revoked member who learned `NS_objectid` can still enumerate ids (metadata, never content)
— is explicitly accepted ([ADR-021.1 §G](021-optional-sync-server.md#g-honest-limits--what-this-cannot-do)).
The epoch-key archive (§L) needs **no** separate vault-lifetime anchor: each `archive[e]` is reachable
from the **current** Vault Key alone. That current Vault Key is **not** stored in the stable package —
it reaches each member through the **per-epoch HPKE grant** (§G, N-15); once a member opens the grant
for the current epoch they walk the archive chain backward (§L).

- **Object granularity — decided (A0.5, K.6): one object per independently-mergeable logical
  record** — one recipe body, one overlay row, one user-scoped record, or one photo. Broad
  per-user buckets are avoided: they amplify conflicts and rewriting under the ADR-021.1 merge
  (§F there). The metadata/blob-count tradeoff (more objects ⇒ more visible blob count, finer
  rotation) is documented and accepted. Photos are naturally per-file.

### G. Per-member key wrapping & the roster

- **Member keys.** At enrollment each member has: an **MUK** (from their own passphrase + Secret
  Key), a **KEK** = `HKDF-Expand(HKDF-Extract(_, MUK), "fond/fondenc2/v2/kek", 32)`, and a
  **member identity keypair** — **X25519** (invitation transport + epoch-grant recipient) plus
  **Ed25519** (roster authorization). The identity keypair derives from a **random `identity_seed`**
  carried in the member's stable package (below), so it is **stable across passphrase / profile
  changes** yet
  recoverable via passphrase + Secret Key (§M).
- **Per-device keys — decided (A0.5, K.7 / I.4).** Each *device* additionally holds its **own
  random Ed25519 signing key** (`device_sign`) and a random `device_id`. A device key is **not**
  recoverable and **not** shared between devices; it is **certified** by the member identity key via
  a **device certificate** `cert = Sign_{member_ed25519}(len_prefix(vault_id ‖ member_id ‖ device_id
  ‖ device_sign_pub ‖ not_before ‖ not_after))`. Per-device keys are **required, not optional**:
  `device_id` is a causal actor ([ADR-021.1 §C](021-optional-sync-server.md#c-per-device-version-vectors))
  and device revocation removes one certificate without touching the member identity. Manifest
  records are signed by `device_sign` and verified against the device certificate recorded in the
  authorizing roster (§H, and [ADR-021.1 §E](021-optional-sync-server.md#e-rollback--fork--equivocation-detection)).
- **Split member key material — decided (A0.5-r2, N-15).** Round-1 unified
  `{current_Vault_Key ‖ NS_objectid ‖ identity_seed}` under one symmetric KEK wrap. That made
  admin-driven **epoch rotation mechanically impossible**: rotation requires re-distributing the new
  Vault Key to every remaining member, but an admin cannot re-wrap another member's package — only
  that member knows the passphrase + Secret Key behind their KEK (N-15). The material is therefore
  split by **rotation behaviour** into two independently-distributed parts:

  1. **Stable self-wrapped package (epoch-invariant; only the member re-wraps it).** The member's KEK
     wraps a single canonical package that carries **no Vault Key** and never changes per epoch:

     ```text
     stable_member_package = NS_objectid(32) ‖ identity_seed(32)
     ```

     - `NS_objectid` is the epoch-invariant object-id namespace key ([ADR-021.1 §B](021-optional-sync-server.md#b-opaque-keyed-object-identifiers), I.2).
     - `identity_seed` is a random per-member seed that **deterministically yields the member identity
       keypair** (§M) — stable across passphrase changes because it lives here, not in the MUK.

     Only the owning member can produce this wrap (it needs their KEK). A **passphrase / Secret-Key /
     profile change re-wraps exactly this one package** (§H); **an epoch rotation does not touch it**
     (it holds no Vault Key).
  2. **Per-epoch Vault-Key grant (public-key distributed; an admin CAN produce it).** For each member,
     the current `VK_e` is delivered by an **authenticated per-member HPKE grant** sealed to the
     member's **X25519 identity public key** (published in the signed roster directory) and signed by
     an admin's Ed25519 key. Because the admin needs only the member's **public** X25519 key, an admin
     can (re-)grant `VK_{e+1}` to every remaining member on rotation **without** any other member's
     KEK/passphrase/Secret Key — the mechanism round-1 lacked (N-15). Grant-record bytes, HPKE `info`,
     and AAD are scoped below; the canonical signing transcript and composition remain
     `[Validation Required]`.

- **Stable-package wrapping — decided (A0.5, K.2; construction #1 of two): XChaCha20-Poly1305
  keywrap** with a random 24-byte nonce.
  `wrapped_stable_package[member] = XChaCha20-Poly1305(key = KEK_member, nonce, plaintext =
  stable_member_package, aad = wrap-AAD)`. AES-256-KW is rejected (no AAD, deterministic-equality
  leakage); AES-SIV is rejected (different key-size/library assumptions). The **wrap-AAD** binds only
  **per-member-stable** fields so it neither collides with the directory hash nor changes when *other*
  members change (and — now that the package holds no Vault Key — no longer needs to bind `epoch`):
  `len_prefix("fond/fondenc2/v2/wrap") ‖ vault_id ‖ member_id ‖ kdf_profile_id ‖ salt ‖
  member_ed25519`. This prevents a wrap from being replayed into a different member slot or vault.
  **Directory-level integrity** — which members/roles/device-certs/grants exist — is provided by the
  **admin signature over the unsigned directory body** (below), *not* by the wrap AAD, so adding a
  member or device, changing a role, or issuing a new epoch grant does **not** invalidate any
  existing member's stable wrap.

#### Grant-free roster-core commitment (N-33)

The target directory has **two distinct commitments**. `roster_core_hash` is SHA-256 of exactly
the following **grant-free core preimage**; it binds the target membership needed by HPKE grants.
It is **not** the directory's content address or a historical authorization substitute.

| Order | Field | Exact encoding |
|---|---|---|
| 1 | Domain | ASCII `fond/fondenc2/v2/roster-core` followed by one `00` octet (29 bytes total) |
| 2 | `vault_id` | 16 raw identifier octets |
| 3 | `current_epoch` | Target epoch, unsigned `u32` little-endian |
| 4 | `prev_roster_hash` | 32 raw bytes of the **immediate predecessor's full completed-directory hash**; all zero only at genesis |
| 5 | Member count | Unsigned `u32` little-endian, number of target member entries |
| 6 | Each target member | `member_id(16) ‖ role(1) ‖ member_ed25519(32) ‖ member_x25519(32)` (81 bytes) |

For this core only, `role` encodes `owner = 00`, `admin = 01`, `member = 02`; other octets are
rejected. Identifiers use their raw 16-byte order (UUID textual hex with hyphens removed, **no**
mixed-endian UUID field conversion). Public keys use their raw 32-byte Ed25519/X25519 encodings,
not text. Sort entries lexicographically by unsigned `member_id` octets before encoding. Reject
duplicate member ids (even identical entries), repeated Ed25519 or X25519 member public keys across
entries, unknown roles, incorrect widths, an empty member set, or a count outside `1..2^32-1`.
There are no optional fields, padding, implicit defaults, or trailing bytes. The preimage length
is exactly `85 + 81 * member_count` bytes. Decoding requires this order; sorting the input member
collection yields the same preimage regardless of its original enumeration order.

**Exact exclusions.** The core contains **only** the six rows above. Excluded are its own
`roster_core_hash` field; the current full `roster_hash`/`new_roster_hash`; every grant body,
`hpke_enc`, ciphertext/tag, and signature output; stable-wrap headers/KDF profile/salt/nonce and
ciphertext/tag; all `devices[]` certificate bodies, public keys, validity fields, and certificate
signatures; directory/admin signature lists; optional roster-metadata bodies/ciphertexts/addresses;
invitation/identity-sidecar outputs; archive bodies/addresses; and any dependent transition id,
manifest id/frontier/head/counter, or completion field. None may be added indirectly as a digest.
In particular, no target device-certificate or signing output can feed back into the core. The
**predecessor** full hash may contain predecessor grants/signatures because those are already
completed, authenticated inputs, not outputs of this target core.

The exclusions do **not** make devices, wraps, or grants unauthenticated: the completed directory
body and its admin signatures cover them as specified below. Core equality alone is insufficient
to authorize any directory, role change, device, or manifest. This is a **scoped codec for the new
core and grant HPKE context only**, not closure of N-17/general canonical signing bytes, public-key
validation, or the roster state machine.

#### Per-epoch grant bound to the core

- **Per-epoch Vault-Key grant — selected (A0.5-r2, K.2; construction #2 of two): HPKE-Base grant.**
  The grant reuses the invitation suite (**HPKE Base, DHKEM(X25519, HKDF-SHA-256), HKDF-SHA-256,
  ChaCha20-Poly1305**, §H) and is an admin-signed roster-directory field:

  ```text
  ┌─ epoch Vault-Key grant  vk_grant[member, e]  (roster-directory field) ──────┐
  │ vault_id       16 bytes                                                     │
  │ member_id      16 bytes    recipient member                                 │
  │ epoch          u32 LE      e (the epoch of the granted VK_e)                │
  │ roster_core_hash 32 bytes  grant-free target core commitment (above)        │
  │ hpke_enc       32 bytes    HPKE encapsulated key (X25519)                    │
  │ hpke_ct        48 bytes    HPKE seal of VK_e(32) + 16-byte Poly1305 tag      │
  ├─ signature ─────────────────────────────────────────────────────────────────┤
  │ admin_sig      64 bytes    Ed25519 over the grant body (all fields above)    │
  └─────────────────────────────────────────────────────────────────────────────┘
  ```

  - **Sealing.** `hpke_enc ‖ hpke_ct = HPKE-Seal(pkR = target_member_x25519, info =
    grant_context, aad = grant_context, pt = VK_e)`. **Exact 94-byte context:**
    ASCII `fond/fondenc2/v2/vk-grant` followed by one `00` octet (26 bytes total), then
    `vault_id(16) ‖ member_id(16) ‖ epoch(u32 LE) ‖ roster_core_hash(32)`, in that order, with no
    other prefixes or terminators. This replaces `len_prefix` **only for this grant context**.
    The recipient entry in the core binds its role and both member public keys; `pkR` must be
    that entry's X25519 key. The grant's six body fields above form a 148-byte payload, in that
    order; `admin_sig` signs all six fields, **not itself**. Its signing domain/framing remains
    `[Validation Required]`, so this is not a complete HPKE/signature interoperability vector.
    Existing members' public keys and the sealing admin's authority come from the **authenticated
    predecessor**, not a target directory signature that has not yet been produced. A new
    member's keys require §H's authenticated out-of-band fingerprint; a key change requires
    predecessor-authorized continuity, not a bare server replacement (state-machine details remain
    open). The admin constructs the target core from those authenticated inputs; it is not yet a
    signed directory.
  - **Opening.** First authenticate the completed directory's admin signature and predecessor
    authorization chain against prior trusted state, and check the signed transition's
    authorization/full-directory-hash/epoch linkage where required (genesis exception below).
    Recompute the core projection/hash and require equality with the directory's stored
    `roster_core_hash`. Require the grant's `vault_id/member_id/epoch/roster_core_hash` to equal the
    authenticated target vault, recipient entry,
    epoch, and recomputed hash; reject missing/duplicate/mis-slotted grants. Derive the recipient
    keys from the recovered stable package (§M), check their public keys against the target member
    entry (in particular the X25519 `pkR`), and verify `admin_sig` using an owner/admin key authorized
    by the predecessor **before HPKE-open**. Open with the exact context above, require a 32-byte
    candidate `VK_e`. Checks requiring the new key (e.g. decrypting/verifying the transition's
    manifest head) follow the open; keep the key provisional and do **not** accept/activate the
    target directory/epoch until all existing transition and anti-rollback rules succeed.
    Only then use the archive (§L). A matching core or successful decryption alone never accepts
    state. Whole-state replay still needs trusted watermarks; full-loss recovery freshness remains
    open (K.12/N-07).
  - **Offline remaining members.** A grant is data in the **signed directory**; an offline member's
    grant simply waits there until they next sync — no online interaction with the admin is needed.
  - **Revoked members.** On the rotation to `e+1` the admin issues **no** grant for the revoked
    member (forward-only): they never receive `VK_{e+1}` (§H).
  - **Blast radius (honest).** A member's **X25519 identity private key now also gates epoch access**:
    anyone who learns `identity_seed` (hence `member_x25519` private) can open every `VK_e` granted to
    that member and thus all content of those epochs. This is a genuine widening versus round-1, where
    epoch access was gated only by the KEK (passphrase + Secret Key). `identity_seed` still lives only
    inside the KEK-wrapped stable package, so compromise still requires breaking that wrap
    (passphrase and Secret Key) or capturing the seed in memory; and the admin gains no new power over
    plaintext (an admin already holds `VK_e`). The trade-off buys the mechanically-possible rotation
    N-15 requires.
- **Roster — split to break the key cycle (A0.5, N-02).** The roster is **not** a single object
  sealed under `VK_e`. The previous design was circular: the `e+1` roster carried the wraps needed
  to obtain `VK_{e+1}` yet was itself encrypted under a key derived from `VK_{e+1}`. It is split
  into two parts:

  1. **Membership & wrap directory — authenticated, NOT confidential under any `VK`.** A signed,
     cleartext record:

     ```text
     ┌─ roster directory (signed; cleartext — no VK confidentiality) ──────────────┐
     │ vault_id            16 bytes                                                 │
     │ current_epoch       u32 LE                                                   │
     │ prev_roster_hash    32 bytes    hash-chain to the previous directory         │
     │ roster_core_hash    32 bytes    recomputed grant-free target commitment      │
     │ members[]           list:                                                    │
     │   ├ member_id        16 bytes   pseudonymous id                              │
     │   ├ role             u8         owner / admin / member                       │
     │   ├ member_ed25519   32 bytes   identity signing pubkey                      │
     │   ├ member_x25519    32 bytes   identity transport pubkey (grant recipient)  │
     │   ├ devices[]        list of device certificates (K.7 above)                 │
     │   ├ wrapped_stable_package[member]  XChaCha20-Poly1305 self-wrap (K.2 #1)    │
     │   └ vk_grant[member]              HPKE VK_{current_epoch} grant (K.2 #2)     │
     ├─ signatures ────────────────────────────────────────────────────────────────┤
     │ admin_sigs[]        ≥1 owner/admin Ed25519 signatures over unsigned body     │
     └────────────────────────────────────────────────────────────────────────────┘
     ```

     Each `wrapped_stable_package` is individually AEAD-encrypted under that member's KEK and each
     `vk_grant` is an HPKE seal to that member's identity key, so the directory needs only
     **authentication** (the admin signatures), never `VK` confidentiality. That removes the
     **N-02 confidentiality cycle**; the grant-free core separately removes the **N-33 structural
     hash cycle**, pending human validation. The directory sits **outside** the new-key encryption
     boundary. **Decision (flagged):** this concedes that member **count, roles, and public keys**
     become server-visible — already within the honest "metadata leaks" limits
     ([ADR-021.1 §G](021-optional-sync-server.md#g-honest-limits--what-this-cannot-do)); content and
     the Vault Key stay confidential.
  2. **Optional confidential roster metadata.** Any non-essential roster metadata (e.g. member
     display labels) MAY be sealed as a separate FONDENC2 object of `object_class = roster-meta`
     under the `roster-meta` subkey (§F). It is not on the unlock path, so it introduces no cycle.

**Full-directory commitment and signature boundaries.** The unsigned directory body comprises
`vault_id`, `current_epoch`, `prev_roster_hash`, the recomputed `roster_core_hash`, and all complete
`members[]` entries shown above: ids/roles/member public keys, actual device certificates
(including their member signatures), stable-wrap headers/nonce/ciphertext/tag, and actual grant
bodies **including `hpke_enc`, `hpke_ct`, and `admin_sig`**. Any directory extension admitted by the
eventual codec is covered too, never silently dropped. Directory admin signatures sign this body
**excluding `admin_sigs[]` itself**. A device certificate signs its existing §G certificate body,
excluding its own signature; neither that body nor a grant signing body contains the current full
directory hash or dependent target transition/manifest outputs. Signature outputs never sign
themselves.

`roster_hash` (and transition `new_roster_hash`) remains SHA-256 of the **canonical completed
directory**, including that unsigned body **and the actual directory `admin_sigs[]` outputs**.
It excludes its own external hash/address and dependent target transition/manifest ids, which are
computed later, not directory fields. Thus changing an actual grant, device certificate, wrap, or
directory signature changes the full-directory preimage even when the core stays unchanged. The
full hash, not the core, remains the predecessor-chain and historical manifest authorization
commitment. The completed-directory codec/hash-domain framing, signature-list encoding/order,
and directory/grant/certificate signing domains remain **`[Validation Required]` (N-17)**:
these exact inclusion/exclusion boundaries do not define those existing transcripts or close K.8.

- **Roster signer model — decided (A0.5, K.8): per-admin keys**, each authorized by the historical
  roster, over a single shared vault signing key. A directory is accepted iff it carries ≥1 valid
  Ed25519 signature from a member whose `owner`/`admin` role is recorded in the **predecessor**
  directory (`prev_roster_hash`). **Ownership transfer** is a chained authorization: the old owner
  signs a transfer authorizing the new owner's identity key, and the new owner signs acceptance;
  both signatures appear in the transition (§H, A0.3 [transition object](#key-rotation--revocation-state-machine)).
  Threshold signatures are deferred unless a concrete recovery policy requires them.
- **Chaining.** `prev_roster_hash` makes roster history tamper- and rollback-evident and dovetails
  ADR-021's signed manifest; the two together make membership *and* content history
  rollback-evident. Genesis uses `prev_roster_hash = 0…0`.

#### Constructive directory order and same-epoch updates

The dependency order is **authenticated predecessor inputs → target core bytes → core hash →
HPKE grants → unsigned completed-directory body → directory admin signatures → full directory
hash → existing transition/manifest commit**. No edge returns from a target output to the core.
Device certificate signatures use their own certificate bodies, not target directory hashes;
they and stable wraps can be prepared independently and are authenticated in the completed body.

- **Genesis.** During local vault creation, generate the owner identity keys and `VK_0`; use
  `current_epoch = 0`, `prev_roster_hash = 0…0`, and the locally trusted owner's member entry.
  Hash that core first, produce the owner's stable wrap and seal/sign its self-addressed HPKE grant,
  assemble/sign the completed directory with the owner key, then derive its full hash for the
  existing genesis manifest/migration commit. There is no predecessor admin: creation trusts the
  locally generated owner key; a different receiver requires an explicitly authenticated/pinned
  genesis vault/owner anchor, never self-signature validity on a server-supplied key alone.
- **Epoch rotation.** Authenticate the predecessor and admin/remaining-member keys, select the
  target membership (no revoked entries), set `e+1` and the predecessor's full hash, hash the core,
  then seal/sign one fresh `VK_{e+1}` grant per target member. Assemble/sign/hash the directory and
  bind its **full** hash and the separately prepared archive reference into the existing
  transition/manifest commit. The core does not depend on any of those later outputs (A0.3 ordered procedure).
- **Same-epoch successor.** A membership/role/key/device or stable-wrap directory update also
  chains to the immediate predecessor's **full** hash. Recompute the target core before grants.
  If the core changes, **every target member needs a fresh grant**, including otherwise unchanged
  members; seal the **current `VK_e`**, not a new epoch key and never another member's KEK.
  Adding a member uses its out-of-band-authenticated keys and its own self-wrap; remaining members
  need no secrets or online interaction. Member revocation still requires an epoch rotation.
  A core-identical candidate may reuse an existing grant only with identical
  vault/member/epoch/recipient bindings and valid predecessor-admin authorization. Changing
  excluded outputs with the *same* predecessor/core (e.g. while preparing a candidate) does not
  require re-granting solely for the core. However, **every new chained directory successor
  changes `prev_roster_hash`**, so even a device-only or passphrase-wrap-only successor requires
  all target grants to be reissued. A previously accepted grant is not lifted unchanged onto
  that successor.

Only the member's explicit passphrase/profile/Secret-Key operation re-wraps its stable package;
the admin's same-epoch grant refresh never changes it or requires another member's KEK. Publishing
that wrap as a chained directory update requires an authorized admin with `VK_e` for the grant
refresh. This is not new member-self-service authorization; N-16/N-19 roster update/commit rules
and same-epoch acceptance durability remain `[Validation Required]`. Readers authenticate and
validate the completed successor under those rules, with any opened VK remaining provisional
until acceptance; an uncommitted core is never a freshness or authorization anchor.

#### Deterministic roster-core example

This **serialization/SHA-256 example only** uses synthetic identifier/public-key octets, not
approved keys, an enrollment transcript, or a full HPKE vector. Inputs: vault bytes `00..0f`,
epoch `7`, predecessor hash bytes `a0..bf`; owner id `10..1f`, Ed25519 bytes `30..4f`, X25519
bytes `50..6f`; member id `20..2f`, Ed25519 bytes `70..8f`, X25519 bytes `90..af`. Sorting by
member id yields owner then member even if supplied in reverse. Concatenate the following
hex lines (whitespace is not part of the bytes):

```text
666f6e642f666f6e64656e63322f76322f726f737465722d636f726500
000102030405060708090a0b0c0d0e0f
07000000
a0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebf
02000000
101112131415161718191a1b1c1d1e1f
00
303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f
505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f
202122232425262728292a2b2c2d2e2f
02
707172737475767778797a7b7c7d7e7f808182838485868788898a8b8c8d8e8f
909192939495969798999a9b9c9d9e9fa0a1a2a3a4a5a6a7a8a9aaabacadaeaf
```

The preimage is **247 bytes**; `roster_core_hash` (SHA-256) is
`092e098d3ab793e3a35ac62e20aceb0c4ac48a38e690a7c375188a68863e905a`.
Changing a role, recipient key, epoch, or predecessor changes these core bytes/hash; permuting
input members does not. With those core inputs fixed, changing grant/ciphertext/signature outputs
does not change this core,
but those actual outputs are still covered by the completed directory's authenticated full
commitment. No full-directory or signature digest is invented while their codecs remain open.
The example demonstrates **construction order**, not composition security or human sign-off.

### H. Enrollment, roles, invitation, revocation, epoch rotation

- **Device enrollment (same member, new device).** Transport the Secret Key via Emergency Kit /
  keychain export; the new device re-derives the MUK (passphrase + Secret Key), pulls the roster
  directory (§G), **unwraps its stable package** with its KEK (recovering `NS_objectid` and
  `identity_seed`), and thereby **recovers the member identity keypair** (§M). It then **HPKE-opens
  its `vk_grant`** for the current epoch (§G) to obtain `VK_{current}`, from which the archive chain
  (§L) yields all historical epoch keys. The new device then **generates a fresh random per-device
  signing key** (`device_sign`, K.7), self-presents it, and the member identity key **certifies** it
  into a new roster directory entry (a device certificate — this *is* a directory update, though not a
  new *member*). Publish it using §G's same-epoch order, refreshing grants of the current VK because
  the predecessor-bound core changes; authenticate the existing grant before the initial open.
  A **freshness anchor** — the current signed manifest head / checkpoint commitment
  ([ADR-021.1 §D](021-optional-sync-server.md#d-authenticated-history-topology-dag-cas--signed-head)) — is
  carried in the enrollment payload so the new device does not accept a stale head on first sync
  (N-07).
- **Roles.** `owner` (bootstraps the vault, transfers ownership, invites/revokes, rotates),
  `admin` (invites/revokes, rotates), `member` (reads/writes data, no membership changes). Under
  zero-knowledge the server cannot enforce content authorization, so roles are **cryptographic**:
  only an owner/admin Ed25519 signature produces a roster the other clients will accept.
- **Invitation (new member) — decided (A0.5, K.3 / N-08 / N-14; completed A0.5-r2, N-18 / N-32).**
  The primitive is **HPKE Base-mode** (RFC 9180), a fully-specified suite — **not** libsodium
  `crypto_box_seal`, whose "X25519 + XChaCha20-Poly1305" description was inaccurate (it is built on
  `crypto_box`, not that AEAD). Pinned suite: **HPKE Base, DHKEM(X25519, HKDF-SHA-256), HKDF-SHA-256,
  ChaCha20-Poly1305**. HPKE alone does **not** authenticate the recipient key source or bind fresh
  state, so the invitation adds recipient authentication, a **complete** sealed payload, and an
  **authenticated freshness anchor**:

  1. **Invitee key first, then authenticated fingerprint.** The invitee **generates `identity_seed`
     first**, derives `member_x25519` / `member_ed25519` (§M), and binds those public keys to an
     **out-of-band fingerprint** (QR code or short authentication string shown to the inviting admin).
     The invitee key is **never** trusted merely because the server relayed it, blocking server
     key-substitution (N-08). The admin seals to this authenticated `member_x25519`.
  2. **Complete sealed payload (N-32).** The invitee cannot construct the specified key material from
     the Vault Key alone, so the admin **HPKE-seals the full bootstrap payload** — not just the Vault
     Key:

     ```text
     invite_pt = VK_e(32) ‖ NS_objectid(32) ‖ freshness_anchor
     freshness_anchor = epoch(u32 LE) ‖ roster_hash(32) ‖ transition_hash(32) ‖
                        frontier_or_checkpoint_id(32) ‖ head_counter(u64 LE)
     ```

     The invitee HPKE-opens this, keeps its own freshly-generated `identity_seed`, and **self-wraps
     the stable package** `NS_objectid ‖ identity_seed` under its own KEK (K.2 #1). The plaintext Vault
     Key is never exposed to the server and never printed anywhere (§M).
  3. **Authenticated freshness anchor (N-18) + signed transcript (N-08).** The admin signs the whole
     invitation, binding the recipient, the HPKE ciphertext **digest**, an expiry, and the exact
     current state so a malicious server cannot replay an older self-consistent invitation:
     `invite_sig = Sign_{admin_ed25519}(len_prefix("fond/fondenc2/v2/invite") ‖ vault_id ‖ invite_id ‖
     recipient_fingerprint ‖ hpke_enc ‖ hpke_ct_digest ‖ role ‖ not_after ‖ epoch ‖ roster_hash ‖
     transition_hash ‖ frontier_or_checkpoint_id ‖ head_counter)`, where `hpke_enc` is the HPKE
     encapsulated key and `hpke_ct_digest = SHA-256(hpke_enc ‖ hpke_ct)`. The invitee verifies the
     signature (against an admin key it authenticated out-of-band with the fingerprint), checks the
     expiry, recomputes `hpke_ct_digest`, and cross-checks the sealed `freshness_anchor` against the
     signed transcript **before** accepting. Because the invitee has no prior watermark, this
     signed-and-sealed anchor **is** its bootstrap trust root — the ADR-021.1 §D N-07 anchor for the
     new-member case (same-member enrollment uses the enrolling device's signed head; a new member
     uses this invitation). Delivery and verification are over the authenticated channel, never the
     bare server response.
- **Revocation = epoch rotation (no bulk re-encryption).** Performed as **one signed transition
  object** (A0.3 [transition object](#key-rotation--revocation-state-machine), K.16), not as
  separate publishes:
  1. An owner/admin generates a **new** Vault Key `VK_{e+1}` and bumps the epoch `e → e+1`.
  2. **Archives the old key:** seals `VK_e` as `archive[e]` under `subkey_{archive,e}` derived from
     `VK_{e+1}` (§L, N-01), so remaining members retain recoverable history through the new key.
  3. **Builds/hashes the grant-free target core first (§G, N-33):** use the authenticated predecessor's
     full hash, target epoch `e+1`, and remaining members' identities/roles/public keys.
  4. **Re-grants the new Vault Key to each remaining member (N-15):** issues a fresh **HPKE
     `vk_grant`** of `VK_{e+1}` to each remaining member's **X25519 identity public key** (from the
     authenticated predecessor), binding the target `roster_core_hash`, using only public keys —
     no other member's KEK/passphrase/Secret Key is
     needed. **Stable packages are untouched** (they hold no Vault Key). The revoked member gets **no**
     `e+1` grant and no new archive grant; an **offline** remaining member's grant waits in the signed
     directory until they resync (§G).
  5. Assembles/authenticates the completed directory with those actual grants, derives its **full**
     `new_roster_hash`, and publishes it with the existing transition binding old/new full hashes,
     old/new epochs, and the manifest predecessor-frontier/head (§L, A0.3). N-21/N-22 still gate the
     archive/immutable commit representation; this does not close K.16.
  6. All **new** writes derive DEKs from `VK_{e+1}`; **existing** objects stay under `VK_e` and
     are **not** re-encrypted (readable via the archive).
- **Honest revocation limit.** Because existing data is not re-encrypted, a revoked member who
  retained `VK_e` (or the old-epoch ciphertext they already downloaded) can still decrypt
  **everything that existed at the moment of revocation**. Epoch rotation protects only data
  written **after** revocation; it does **not** retroactively protect past data. This is inherent
  to any revocation that avoids bulk re-encryption, and is stated plainly rather than implied
  away. **Decided (A0.5, K.9): optional lazy/background re-encryption** — opportunistically
  re-seal old-epoch objects under the current epoch on next write or a background pass — is kept as
  **best-effort forward hardening only**: it **shrinks but cannot eliminate** the exposure window
  (the member already saw the plaintext) and cannot force a malicious server to delete old
  ciphertext.
- **Passphrase change / Secret Key rotation.** Re-derives MUK → re-derives KEK → re-wraps the
  member's **stable package** (`NS_objectid ‖ identity_seed`, §G) under the new KEK. **One** durable
  wrap; **no** data re-encryption and **no** epoch bump. The stable package holds **no** Vault Key, so
  no VK is re-wrapped here — the member keeps reaching `VK_{current}` through its unchanged HPKE
  recipient key, and all historical epoch keys through the archive from it (§L). Publishing the
  new stable wrap as a chained directory successor **does** require same-epoch re-grants of the
  current VK to all target members (§G), solely because the predecessor-bound core changes.
  Previously accepted historical grants still open with the same identity but cannot be reused
  under a different core. The member identity keypair is likewise
  **unchanged**: it derives from the stable package's random `identity_seed` (§M), not the MUK, so a
  passphrase change neither rotates the identity nor invalidates existing device certificates,
  historically authenticated grants, or the account identity sidecar.

### I. `FONDENC1` → `FONDENC2` migration (one-time)

- **Trigger & detection.** Runs on the first `fond sync setup` / first hierarchy-backed encrypted
  export; a `FONDENC1` blob is detected by its magic.
- **Steps.** (1) **Reject legacy Argon2 params outside a compiled allowlist before any KDF** (N-06;
  A0.3 migration step 2). (2) Open the single `FONDENC1` bundle with the existing `KeyMaterial`
  (keychain raw key or passphrase) via `open_bundle`. (3) Generate the Vault Key, `NS_objectid`, and
  an epoch-0 roster directory with the owner's self-wrapped stable package plus a self-addressed epoch
  grant (core first, completed-directory full hash afterward, §G). (4) Split the `OverlayBundle`
  into per-object plaintext units by the decided granularity
  (§F). (5) Seal each unit as a `FONDENC2` object under its Vault-Key-derived DEK at epoch 0.
  (6) Retain (default) or best-effort delete the legacy `FONDENC1` blob per user choice.
- **Idempotent & lossless.** Re-running detects already-migrated state (a `FONDENC2` object
  present) and no-ops; existing user edits are never overwritten (import-idempotency house rule).
  The `.cook` files — the source of truth (ADR-002) — are untouched; migration only re-frames the
  derived overlay/photo blobs.

### J. Primitives

| Primitive | Role in FONDENC2 | Status |
|---|---|---|
| Argon2id | MUK stretch (L1), pinned bounded profiles | Named; version `0x13` pinned (K.1) |
| HKDF-SHA-256 | KEK / subkey / DEK / archive derivation (L2, L4, L5, §L) | **Decided (K.4)** over keyed BLAKE3 |
| XChaCha20-Poly1305 | per-object AEAD **and** stable-package self-wrap (K.2 #1) | Named; **decided** as the self-wrap (K.2 #1) |
| HPKE Base-mode (RFC 9180) | new-member invitation transport **and** per-epoch Vault-Key grant (DHKEM X25519 / HKDF-SHA-256 / ChaCha20-Poly1305) | **Decided (K.3 / N-14; grant A0.5-r2 / N-15 / K.2 #2)** |
| X25519 | HPKE KEM + member identity (invitation + epoch-grant recipient) | Named |
| Ed25519 | member identity + per-device signing, roster authorization, grant signature | Named; **per-admin + per-device keys decided (K.7 / K.8)** |
| HMAC-SHA-256 | opaque object-id namespace (32-byte output, ADR-021) | Named; **32-byte width decided (N-13 / K.11)** |
| OPAQUE (RFC 9807) | account/PAKE server binding | Referenced; OPAQUE-3DH ristretto255/SHA-512 pinned in [ADR-021.2 §F](021-optional-sync-server.md#f-chosen-ciphersuite-parameters--a05-sign-off) |

The individual primitives are standard and not hand-rolled, but their **composition is novel** — in
particular the A0.5-r2 **per-member per-epoch HPKE Vault-Key grant** (§G) is a new construction the
original review never saw. Whether that composition is sound is **`[Validation Required]`** (N-31):
the standard-primitive pedigree is *not* a security proof, which is precisely why the A0.5 independent
human review is mandatory before any implementation.

### K. Open questions — A0.5 remediation status

The A0.5 reviews adjudicated each item below. Affected K.2/K.12/K.16 claims are narrowed here in
light of round 3 and the **N-33-only specified correction, pending human validation**; unaffected
rows retain their earlier decision wording, not a new grading. Items are
**Resolved**, **Partially-resolved** (direction pinned, canonical bytes / transitions still owed),
**Decided (direction)** with a `[Validation Required]` tail, or **deferred**. Section/label pointers
are to the (revised) sections above.

| # | Item | Status (affected gates; other rows retain earlier wording) | Where |
|---|---|---|---|
| K.1 | MUK two-secret binding | Decided (direction: Argon2 `secret` slot, `0x13`); two-secret **KAT `[Validation Required]`** | §C |
| K.2 | Vault-Key/stable wrap construction | **Partially-resolved**: self-wrap + HPKE grant selected; scoped core/info/AAD fixed, signing bytes/stable-wrap codec/composition `[Validation Required]` | §G |
| K.3 | Invitation transport | Decided: HPKE + signed transcript; **+`NS_objectid`/freshness anchor (A0.5-r2)**; canonical bytes `[Validation Required]` | §H |
| K.4 | Subkey/DEK KDF | Decided (HKDF-SHA-256); KEK Extract salt / raw-vs-length-prefixed `[Validation Required]` | §B, §F |
| K.5 | Nonce strategy | Resolved: pure-random 192-bit, no counter | §E |
| K.6 | Object granularity | Resolved: one object per mergeable record | §F |
| K.7 | Per-device keys | Partially: key model fixed; certificate transcript `[Validation Required]` | §G, §H, §M |
| K.8 | Roster signer model | Partially: per-admin decided; canonical body / transition semantics `[Validation Required]` | §G |
| K.9 | Lazy re-encryption | Resolved: optional, best-effort forward hardening only | §H |
| K.10 | Identity ↔ OPAQUE binding | Partially: client-anchored; roster link / transcript `[Validation Required]` | [ADR-021.2 §E](021-optional-sync-server.md#e-binding-vault-identity-keys-to-the-account-client-anchored-resolves-k10) |
| K.11 | `object_id` source & width | Partially: 32-byte width decided; `len_prefix` / class taxonomy `[Validation Required]` | §E, §F, [ADR-021.1 §B](021-optional-sync-server.md#b-opaque-keyed-object-identifiers) |
| K.12 | Emergency Kit / recovery | **Not closed**: grant-based recovery direction; authenticated state/full-loss freshness `[Validation Required]`; N-28 clarification retained | §M |
| K.13 | Argon2 figures & budget | **Not-resolved / deferred (human + measured devices)** | [A0.3 registry](#argon2id-profile-registry) |
| K.14 | Profile deprecation & forced upgrade | Resolved: explicit, transactional, never silent | [A0.3 lifecycle](#argon2id-profile-registry) |
| K.15 | Legacy-blob disposition | Resolved: retain by default; delete is best-effort | [A0.3 migration](#fondenc1--fondenc2-migration-algorithm) |
| K.16 | Cross-object rotation atomicity | **Not closed**: N-33 structural correction only; `archive_ref` derivation / prepared→committed representation `[Validation Required]` (N-21/N-22) | [A0.3 transition object](#key-rotation--revocation-state-machine) |

**Still open (`[Validation Required]` / deferred).** The immutable round-3 ledger keeps **K.1,
K.4, K.12, K.13, K.16 NOT-RESOLVED** and **K.2, K.3, K.7, K.8, K.10, K.11 PARTIALLY-RESOLVED**.
This bounded change does not promote those grades or N-15/N-01. The core and grant HPKE context
now have scoped bytes, but the general codec, signing/certificate/roster/transition transcripts,
KDF Extract salts, archive/commit representations, and recovery freshness remain open. N-28's
two-secret-loss clarification remains; the separate invitation replay/freshness gates are not
changed here. **NO-GO**: only the structural N-33 cycle has a specified correction pending human
validation; #120 stays OPEN/BLOCKED pending independent human review and normative vectors.

### L. Epoch-key archive & history recovery (N-01)

Random per-epoch Vault Keys leave old objects encrypted under old keys. Without a recoverable
archive, a new device or a re-wrapped member would lose all pre-current-epoch data, and a passphrase
change could not "re-wrap one key". This specifies an archive direction, **not closure of N-01**:
grant composition/recovery and archive addressing/commit representations remain
`[Validation Required]` (N-21/N-22).

- **Forward-chained wrap.** Alongside each rotation `e → e+1`, the old key is sealed into an
  **authenticated archive record** under the standard §F subkey derivation, rooted in the **new**
  epoch's key:
  `archive[e] = XChaCha20-Poly1305(key = subkey_{archive, e}, nonce, plaintext = VK_e,
  aad = len_prefix("fond/fondenc2/v2/archive-aad") ‖ vault_id ‖ epoch_le(e) ‖ epoch_le(e+1))`, where
  `subkey_{archive, e} = HKDF-Expand(PRK_{VK_{e+1}}, "fond/fondenc2/v2/subkey" ‖ "archive" ‖
  epoch_le(e), 32)` (the §F formula, keyed by the **new** key `VK_{e+1}`, `info` naming the **old**
  epoch `e`). Because `archive[e]` is encrypted under a subkey of `VK_{e+1}`, **any holder of the
  current Vault Key can walk the chain backward** `VK_{cur} → VK_{cur-1} → … → VK_0`, recovering
  every historical epoch key. There is a **single** derivation (this one); no separate archive-root
  key is used.

  ```text
  VK_0  ◄─archive[0]─  VK_1  ◄─archive[1]─  VK_2  ◄─ … ◄─archive[e-1]─  VK_e (current)
         (archive[i] = VK_i sealed under subkey_{archive,i} derived from VK_{i+1})
  hold VK_e  ⇒  unwrap archive[e-1] ⇒ VK_{e-1} ⇒ unwrap archive[e-2] ⇒ … ⇒ VK_0
  ```

- **Reaching the chain root — via the per-member grant, not a package field.** A member obtains the
  **current** Vault Key `VK_{current}` from its **HPKE `vk_grant`** (§G, N-15), then walks the chain
  backward. The archive chain itself is unchanged; what round-1 got wrong was *distribution* — it
  assumed each member's package already held the current VK, which made rotation impossible. Now the
  current VK is delivered through the admin-issued public-key grant bound to `roster_core_hash`
  (§G, verified against an authenticated **full** directory), and the archive turns that single
  current key into the whole connected history. A **passphrase / Secret-Key change re-wraps only
  the stable package** (no VK); publishing a chained same-epoch directory refreshes grants using
  that same current VK (§G/§H), so it does not alter the archive chain.
- **No separate anchor needed.** Each `archive[e]` is reachable from `VK_{current}` alone, so the
  archive requires no vault-lifetime anchor key (§F) — only the current Vault Key that every remaining
  member receives through its epoch grant.
- **Authenticated & signed.** Each `archive[e]` is bound (AAD) to `vault_id` and the `(e, e+1)`
  epoch pair, and the transition object that publishes it (K.16) is signed by an owner/admin, so the
  server can neither forge nor reorder archive links.
- **New-member history policy — decision (flagged).** **Default: new members receive full history.**
  A new member receives the current `VK` (in the invitation, §H) and thereafter its epoch grants, from
  which the whole archive chain is reachable — consistent with the family-shared principle and with
  the fact that forward-only revocation already concedes that current members can read everything that
  existed. **History barrier is household-wide, not member-selective (corrected).** Omitting
  `archive[b-1]` at a barrier epoch `b` severs the chain for **every** holder of the current `VK` —
  pre-barrier *and* new members alike (those who already cached `VK_{<b}` keep it; nobody recovers it
  *through the chain*). Truly *member-selective* history restriction would require **per-member
  segment-root wraps** (grant each member only the archive segments they may read), which is
  **deferred** as a heavier optional capability. This corrects the earlier over-claim that a barrier
  could restrict new members only, and is surfaced for the re-reviewer as a genuinely new design
  decision.

### M. Identity-key recovery & Emergency Kit (N-10, K.12)

The prior text generated **random** member Ed25519/X25519 identity keys with **no** backup or
recovery path, so a restored device could re-derive the MUK yet could not prove the member identity;
"Emergency Kit = Secret Key only" was therefore insufficient (A0.5 N-10). Resolved by deriving the
member identity from a **random `identity_seed` carried in the KEK-wrapped stable package** (§G) —
recoverable, yet **stable across passphrase changes** — and keeping **per-device keys random and
non-recoverable**:

- **Member identity keys derive from `identity_seed`** (a field of the self-wrapped stable package,
  §G), **not** from the MUK — so a passphrase / Secret-Key / profile change (which re-derives the MUK
  and KEK and re-wraps the stable package) leaves the identity keypair, its device certificates, its
  historical grant decryption keys, and the account identity sidecar
  ([ADR-021.2 §E](021-optional-sync-server.md#e-binding-vault-identity-keys-to-the-account-client-anchored-resolves-k10))
  **unchanged**. Publishing the wrap in a chained directory refreshes current-VK grants under the
  new core (§G); this does not rotate the member identity:
  - `member_ed25519 = Ed25519_from_seed(HKDF-Expand(identity_seed, "fond/fondenc2/v2/member-ed25519", 32))`;
  - `member_x25519 = X25519_from_scalar(clamp(HKDF-Expand(identity_seed, "fond/fondenc2/v2/member-x25519", 32)))`.
- **Recovery direction — via the grant, not a cross-member re-wrap (K.12, still gated).** A
  restored device with passphrase + Secret Key re-derives the MUK → KEK and pulls the signed roster
  directory, provided a server/device/backup supplies it. It authenticates the directory chain
  against trusted state or an explicit genesis anchor (§G), **unwraps its stable package**
  (recovering `NS_objectid` and `identity_seed` → the identity keypair), then performs §G's core,
  recipient-key, grant-field, and admin-signature checks **before HPKE-opening its own `vk_grant`**.
  The candidate VK stays provisional until the remaining transition/anti-rollback checks succeed,
  then yields accepted `VK_{current}` and archive history (§L). This is a dependency-order
  direction, not a closed recovery protocol: full-loss recovery without a surviving trusted
  roster/head anchor remains `[Validation Required]`
  (K.12/N-07); a server's self-consistent old directory is not proof of current state. Recovery rides the
  **per-member public-key grant** — no admin ever needs another member's KEK, and the impossible
  round-1 "re-wrap each remaining member's package" flow is gone. No identity private key or Vault Key
  is ever printed or separately backed up.
- **Per-device keys stay random and are re-issued, not recovered** (K.7). A restored/new device
  generates a fresh random `device_sign`, and the recovered member identity key **certifies** it into
  the roster directory (§G/§H). Device compromise revokes one certificate; the member identity is
  untouched.
- **Emergency Kit — confirmed (K.12): Secret Key only.** The Kit carries the Secret Key + sign-in
  URL and **never** the MUK, the Vault Key, any epoch key, `identity_seed`, or any identity/device
  private key. With passphrase + Secret Key a member reconstructs MUK → KEK → the stable package (→
  `NS_objectid`, `identity_seed`) → its `vk_grant` (→ `VK_{current}` → archive history §L); nothing
  else needs printing. The **Vault Key is never printed** anywhere.
- **The Kit does *not* protect against passphrase loss (N-28, corrected).** Both secrets are required
  and the two protect against **different** losses: the printed Secret Key protects against **device
  loss / re-provisioning** (you can bring the Secret Key to a new device), while the passphrase lives
  only in the member's memory. Losing **either** secret is unrecoverable — the Kit cannot recreate a
  forgotten passphrase, and the passphrase cannot substitute for a lost Secret Key. This restates,
  not contradicts, the two-secret loss statement in "Emergency Kit & recovery"; the earlier phrasing
  that implied the Kit guards against *passphrase* loss was wrong.
- **Genesis / offline recovery note.** Recovery requires the server-held stable package **and** epoch
  grant. A fully offline single-device loss with no server copy and no other device is unrecoverable —
  the same honest limit stated in "Emergency Kit & recovery"; the Kit protects against device loss /
  re-provisioning, not simultaneous loss of every copy or a forgotten passphrase.

## Appendix: KDF profiles, rotation & migration (A0.3)

This appendix concretizes four sketches from the [FONDENC2 appendix](#appendix-fondenc2-protocol)
above into implementable detail: the **pinned KDF profiles**
([§D](#d-pinned-kdf-profiles--the-authenticated-params-fix)), the **two-secret MUK derivation**
([§C](#c-domain-separation--the-two-secret-muk)), the **epoch rotation / revocation** procedure
([§H](#h-enrollment-roles-invitation-revocation-epoch-rotation)), and the **one-time
`FONDENC1` → `FONDENC2` migration** ([§I](#i-fondenc1--fondenc2-migration-one-time)). It
**references** those sections rather than restating them, reusing their exact vocabulary (MUK,
Vault Key, epoch, `kdf_profile_id`, `PROFILE[...]`, member KEK, `fond/fondenc2/v2/...`, roster,
Ed25519). Its new decisions and the one remaining deferred figure are reflected in the FONDENC2
[§K status table](#k-open-questions--a05-remediation-status) (K.13–K.16).

**Gate reminder:** this is a paper spec. Every `[Validation Required]` tag marks a choice the A0.5
independent reviewer must sign off on; no crypto/sync code lands before the Epic A0 review clears.

### A0.5 remediation-mapping (A0.3)

Coverage for the review items landing in this A0.3 appendix:

| Finding | Handled in | Status (affected gates; other rows retain r2 history) |
|---|---|---|
| K.16 / N-02 transition object | [rotation state machine](#key-rotation--revocation-state-machine) | **Not closed**: core/grant dependency order specified; `archive_ref`/commit representation `[Validation Required]` (N-21/N-22) |
| N-33 grant/directory fixed point | [§G core](#grant-free-roster-core-commitment-n-33), [rotation state machine](#key-rotation--revocation-state-machine) steps 4–6 | **Specified correction; pending human validation** (structural cycle only) |
| N-15 epoch re-grant on rotation | [rotation state machine](#key-rotation--revocation-state-machine) step 5 | **Not closed**: public-key grant direction; composition/signing/state machine `[Validation Required]` |
| N-06 migration pre-auth Argon2 | [migration algorithm](#fondenc1--fondenc2-migration-algorithm) step 2 | Spec correct; **code fix is A1 impl task (#121)**, not spec-resolvable |
| VR-020-K13.5 / N-11 pre-auth ceiling | [profile registry](#argon2id-profile-registry) | Partially: per-platform ceiling policy decided; actual sets/ceilings `[Validation Required]` |
| K.14 deprecation & forced upgrade | [profile registry](#argon2id-profile-registry) | Resolved: no silent re-wrap |
| K.15 legacy-blob disposition | [migration algorithm](#fondenc1--fondenc2-migration-algorithm) step 9 | Resolved: best-effort delete |
| VR-020-K13.4 salt width | [MUK derivation parameters](#muk-derivation-parameters) | Resolved: 16-byte salt |
| K.1 MUK two-secret binding | [MUK derivation parameters](#muk-derivation-parameters) | Decided (direction); two-secret KAT `[Validation Required]` |
| K.2 wrap AAD | [profile registry](#argon2id-profile-registry) wrap entry | **Partially-resolved**: scoped core/grant HPKE context bytes fixed in §G; stable-wrap/signing bytes and composition `[Validation Required]` |
| K.13 / VR-020-K13.1-.3 figures | [profile registry](#argon2id-profile-registry) | Not-resolved / deferred (human + measurement) |

### Scope & pointer map

| This appendix subsection | Concretizes | Acceptance criterion |
|---|---|---|
| Argon2id profile registry | [§D](#d-pinned-kdf-profiles--the-authenticated-params-fix) | #1 |
| MUK derivation parameters | [§C](#c-domain-separation--the-two-secret-muk) | #2 |
| Key rotation & revocation state machine | [§H](#h-enrollment-roles-invitation-revocation-epoch-rotation) | #3 |
| `FONDENC1` → `FONDENC2` migration algorithm | [§I](#i-fondenc1--fondenc2-migration-one-time) | #4 |

It does **not** re-explain the key hierarchy (§B), the per-object envelope (§E), DEK derivation
([§F](#f-per-object-dek-derivation--object-granularity)), or the roster (§G) — those remain
authoritative in the FONDENC2 appendix and are only cited.

### Argon2id profile registry

*Concretizes [§D](#d-pinned-kdf-profiles--the-authenticated-params-fix) — the authenticated-params
fix.*

- **Registry structure.** `PROFILE: kdf_profile_id (u8) → { m_cost_kib: u32, t_cost: u32,
  p_cost: u32 }`, a fixed table **compiled into every client build**. The `kdf_profile_id` byte is
  the *only* selector; free-form Argon2 integers from a header never drive derivation (the §D fix).
- **Append-only, never mutate.** Once a `kdf_profile_id` ships, its parameter triple is **frozen**
  for the life of the format — editing it would silently change every MUK derived under that id and
  break unlock. Re-tuning allocates a **new** id; existing ids are never edited, removed, or reused.
  Ids may be **deprecated** (below) but the table only ever grows.
- **Reject-before-derive.** On unlock the client looks up `kdf_profile_id` in its compiled table. An
  id that is unknown, out of range, or withdrawn fails the unlock **before any Argon2 invocation and
  before any KDF memory is allocated** — the concrete structural closure of the pre-auth
  resource-exhaustion vector (§D).
- **Per-platform accepted-profile sets + local pre-auth ceiling — decided (A0.5, VR-020-K13.5 /
  N-11).** A single registry-wide `m_cost` ceiling (previously illustrated at ~1 GiB) is **too high
  as pre-auth work** on a constrained client: because a profile id is only *authenticated* once the
  client derives the candidate KEK and checks the wrap tag, any *valid, expensive* id an attacker
  substitutes still forces its bounded cost before the tag fails (N-11). So each platform ships a
  **small accepted-profile set** and a **much lower local pre-auth `m_cost` ceiling** than the
  registry maximum; a wrap naming a profile outside the local accepted set is refused before Argon2.
  In addition the client **pins the member's own expected `kdf_profile_id`** where it has seen this
  vault before, and applies **bounded retry / throttling** on repeated unlock failures. The heaviest
  work an attacker can trigger is thus bounded by the *local platform ceiling*, not the registry
  maximum or attacker-supplied header bytes.
- **Authenticating the selected id.** `kdf_profile_id` travels in the per-member wrap entry's
  cleartext header and is bound into that entry's AEAD **associated data**. **Note (A0.5, N-11):** the
  AAD tag can only be checked **after** the candidate KEK is derived — i.e. after Argon2 runs — so the
  tag catches an id/param *mismatch* but does **not** prevent a valid-but-expensive id from costing
  work first. What happens strictly **before** Argon2 is the compiled-table lookup and the
  per-platform accepted-set/ceiling check (above); those, not the AEAD tag, are what bound pre-auth
  work. If raw params are echoed for auditing, they MUST equal `PROFILE[kdf_profile_id]` or the tag
  fails after derivation.
- **Deprecation lifecycle — decided (A0.5, K.14): no silent forced re-wrap.** A profile moves
  `active → deprecated → withdrawn`. A `deprecated` id is still accepted to **open** existing wrap
  records (backward compatibility) but is refused for **new** wraps. Unlock under a deprecated id
  **MUST NOT silently** re-wrap: any upgrade to the current active profile is **explicit or
  prominently announced**, performed as a **transactional, crash-safe, rollback-safe** re-wrap
  (stage → fsync → atomic swap), and a **read-only recovery path under the old profile is retained**
  until the new wrap is confirmed durable. A profile is **never** made unreadable merely by marking
  it withdrawn while it is the *only* copy of a wrap; `withdrawn` is used only to refuse *new* wraps
  for a profile later found too weak, after members have migrated.
- **Concrete starting profiles — figures still deferred (K.13, `[Validation Required]`):**

| id | Name | `m_cost` | `t_cost` | `p_cost` | output |
|---|---|---|---|---|---|
| `PROFILE[1]` | desktop-interactive | ~256 MiB (`262144` KiB) | 3 | 1 | 32 B |
| `PROFILE[2]` | mobile/watch-constrained | ~64 MiB (`65536` KiB) | 3 | 1 | 32 B |

These figures are **illustrative anchors, not decisions** and remain **deferred to a human
cryptographer with measured device evidence (K.13, `[Validation Required]`)**: the reviewer records
p50/p95 time, peak RSS, thermal/battery behaviour, and concurrent-unlock limits on each minimum
device class, then pins each triple against a target budget (nominally desktop unlock ≈ 1 s,
mobile/watch ≈ 1.5 s at acceptable peak RAM — treated as goals to measure, not accepted values).
Phone and watch are **separated** if their measurements diverge (K13.2). Instead of one high
registry-wide ceiling, each **platform** pins its own **accepted-profile set and local pre-auth
`m_cost` ceiling** (VR-020-K13.5, above) — much lower than any registry maximum. The chosen profile
is recorded once, per member, at wrap creation; different members/devices of the same vault MAY use
different profiles, since the profile governs only that member's MUK stretch, not the shared Vault
Key.

**Per-member stable-package wrap entry + epoch grant (shape-only; the authoritative byte layout lives
in `crates/fond-store/src/crypto.rs` once implemented, per the FONDENC2 §E convention):**

```text
┌─ Per-member stable-package wrap entry (roster directory field; FONDENC2 §G K.2 #1) ┐
│ member_id       16 bytes    pseudonymous member id                         │
│ kdf_profile_id  u8          selects PROFILE[id] for THIS member's MUK       │
│ salt            16 bytes    per-member Argon2 salt (CSPRNG)                 │
│ nonce           24 bytes    XChaCha20 nonce for the wrap AEAD              │
├─ wrap ciphertext ───────────────────────────────────────────────────────────┤
│ AEAD (XChaCha20-Poly1305, K.2 #1) over the stable package (no Vault Key)    │
│   plaintext = NS_objectid(32) ‖ identity_seed(32)                          │
│   key = member KEK = HKDF(MUK)                          (FONDENC2 §G)       │
│   MUK = Argon2id(password = len_prefix("…/muk") ‖ NFC(passphrase),         │
│                  secret = Secret Key, salt, PROFILE[id])   (FONDENC2 §C)    │
│   AAD = len_prefix("fond/fondenc2/v2/wrap") ‖ vault_id ‖ member_id ‖       │
│         kdf_profile_id ‖ salt ‖ member_ed25519                 (§G)         │
└────────────────────────────────────────────────────────────────────────────┘

┌─ Per-member epoch Vault-Key grant  vk_grant[member]  (roster field; §G K.2 #2) ────┐
│ vault_id   16 bytes target vault                                           │
│ member_id  16 bytes target recipient member                                │
│ epoch      u32 LE   epoch of the granted VK_e                              │
│ roster_core_hash 32 bytes grant-free target core (§G, NOT roster_hash)     │
│ hpke_enc   32 bytes HPKE encapsulated key (X25519)                         │
│ hpke_ct    48 bytes HPKE seal of VK_e(32) + tag; sealed to member_x25519    │
│ admin_sig  64 bytes Ed25519 over the grant body (§G)                       │
│   HPKE info/AAD = grant_context (exact 94 bytes defined in §G)            │
└────────────────────────────────────────────────────────────────────────────┘
```

The two wrap **constructions** are now **decided** (FONDENC2 §G / K.2): a **symmetric
XChaCha20-Poly1305 self-wrap** of the epoch-invariant stable package (only the member can produce it)
and an **HPKE grant** of the per-epoch Vault Key (an admin can produce it from authenticated public
keys once the target core is hashed — the N-33-only structural correction). K.2/N-15 composition
and signing transcripts remain `[Validation Required]`. The stable-wrap `nonce` applies to that
AEAD; because the stable package holds no Vault Key, its AAD no longer binds `epoch`. What A0.3
pins here is that `kdf_profile_id` and `salt` are
**authenticated header fields** inside the exact wrap-AAD above (§G), which binds only
**per-member-stable** fields — so re-wrapping one member's stable package, adding another
member/device, or issuing a new epoch grant never invalidates any other member's wrap.
**Directory-level integrity is the admin signature's job** (§G), not the wrap AAD.

### MUK derivation parameters

*Concretizes [§C](#c-domain-separation--the-two-secret-muk) — the two-secret MUK.*

`MUK = Argon2id(password = len_prefix("fond/fondenc2/v2/muk") ‖ NFC(passphrase), secret = Secret Key,
salt, params = PROFILE[kdf_profile_id])`. A0.3 pins the surrounding parameters:

- **Salt.** 16 bytes (128-bit), drawn from the system CSPRNG per member at wrap creation, stored in
  the wrap entry above, never reused across vaults or members. **Decided (A0.5, VR-020-K13.4): a
  uniformly random 16-byte salt is sufficient**; widening to 32 bytes is not required.
- **Output length.** 32 bytes — feeds the member KEK HKDF (§G).
- **Passphrase encoding.** UTF-8 with **NFC** normalization, length-prefixed with the
  `fond/fondenc2/v2/muk` use-label as pinned in §C (input-side domain separation from the OPAQUE-login
  use of the same passphrase).
- **Domain-separation labels.** The MUK Argon2 `password` buffer carries the `fond/fondenc2/v2/muk`
  label (§C, K.1); the downstream KEK HKDF additionally uses the `fond/fondenc2/v2/kek` label (§G).
  Both label bindings are pinned, and are consistent with §C.
- **Two-secret binding — decided (A0.5, K.1).** The Secret Key enters via Argon2's keyed `secret`
  (pepper) slot (not an HKDF pre-mix), with Argon2 version `0x13` pinned and a cross-implementation
  KAT (§C). This appendix's salt, length, encoding, and profile feed that construction.

### Key rotation & revocation state machine

*Concretizes [§H](#h-enrollment-roles-invitation-revocation-epoch-rotation) — epoch rotation.*

- **Triggers.** (1) member **revocation**; (2) **passphrase / Secret-Key change**; (3)
  **periodic / policy** rotation; (4) **suspected compromise**. Triggers 1, 3, and 4 bump the epoch;
  a passphrase / Secret-Key change is a **re-wrap only**, with no epoch bump (below).

```mermaid
stateDiagram-v2
    [*] --> SteadyE
    SteadyE --> SteadyE: passphrase / Secret-Key change (re-wrap stable package only, no epoch bump)
    SteadyE --> Rotating: revocation / periodic / suspected compromise
    Rotating --> SteadyNext: signed transition object commits VK e+1, archive[e], roster e+1 (with new HPKE grants), manifest head
    SteadyNext --> [*]
    note right of Rotating
      NS_objectid + identity_seed stay epoch-invariant (ADR-021.1 I.2 / §M):
      NOT re-derived on rotation; VK e+1 re-granted per member via HPKE (N-15)
    end note
```

- **Ordered rotation procedure (revocation = epoch rotation, no bulk re-encryption).**
  1. **Precondition:** authenticate the predecessor directory/full hash and an owner/admin
     Ed25519 signing key authorized there; take remaining-member public keys from that trusted
     predecessor, not the not-yet-signed target (§G). Roles are cryptographic, not server-enforced (§H).
  2. Generate a fresh random 32-byte `VK_{e+1}`; set `epoch = e + 1`.
  3. **Archive the old key (N-01, §L):** seal `VK_e` as `archive[e]` under an `archive` subkey of
     `VK_{e+1}`, so remaining members keep recoverable history through the new key.
  4. **Build/hash the grant-free target core (N-33):** set target epoch `e+1`,
     `prev_roster_hash` to the predecessor's **full** hash, and target membership with revoked
     members removed. Canonicalize/hash §G's exact core bytes before any target grant.
  5. **Re-grant `VK_{e+1}` to each remaining member (N-15):** issue/sign a fresh **HPKE `vk_grant`**
     sealed to that member's authenticated X25519 key, using §G's `roster_core_hash` context —
     no other member's KEK or online interaction is needed. Stable packages are **not** touched
     (they hold no Vault Key). The revoked member gets no `e+1` grant and no new archive grant.
  6. Assemble the `e+1` roster directory with the computed core hash and actual grants, **sign its
     unsigned body** with the predecessor-authorized owner/admin key, then derive its **full**
     `new_roster_hash` including actual grants and directory signatures (§G).
  7. **Commit as one signed transition object** (below, existing acceptance requirement still gated
     on N-21/N-22). From here, **new** writes derive DEKs from
     `VK_{e+1}` (§F); existing objects keep their sealing epoch and are **not** re-encrypted (read via
     the archive).
- **Signed transition object — decided (A0.5, K.16 / N-02).** Separate "publish roster, then write a
  manifest" is not atomic and previously left the new roster key-circular. One **signed transition
  object** makes roster epoch and the [ADR-021.1](021-optional-sync-server.md#d-authenticated-history-topology-dag-cas--signed-head)
  manifest `vault_epoch` a single state-machine commit:

  ```text
  ┌─ transition object (signed by owner/admin Ed25519) ─────────────────────────┐
  │ vault_id            16 bytes                                                 │
  │ old_roster_hash     32 bytes    FULL completed-directory hash at epoch e     │
  │ new_roster_hash     32 bytes    FULL completed-directory hash at epoch e+1   │
  │ old_epoch           u32 LE      e                                            │
  │ new_epoch           u32 LE      e+1                                          │
  │ archive_ref         32 bytes    record_id of archive[e] (§L); 0…0 if barrier │
  │ pred_frontier[]     list<32 B>  the COMPLETE manifest frontier at epoch e    │
  │                                 (all heads, ADR-021.1 §D) this cut dominates  │
  │ manifest_head       32 bytes    epoch-e+1 record that dominates pred_frontier │
  │ completion          u8          0 = prepared, 1 = committed                  │
  ├─ signature ─────────────────────────────────────────────────────────────────┤
  │ ed25519_sig  64 bytes  over ALL fields above (owner/admin key, §G)          │
  └────────────────────────────────────────────────────────────────────────────┘
  ```

  - **Atomicity requirement (K.16, not established).** A reader advances to `e+1` **only** on a
    fully-signed transition object with
    `completion = 1` whose `new_roster_hash` and `manifest_head` both resolve and whose
    `manifest_head` **dominates every id in `pred_frontier[]`** (so no honest concurrent head is
    dropped, the intended DAG single-head-gap correction). Before that acceptance the reader stays
    at `e` (ignoring prepared state). This describes the required behavior, **not proof of crash
    atomicity or closure of K.16**: archive addressing and an immutable prepared→committed
    representation remain `[Validation Required]` (N-21/N-22).
  - **The new epoch grants commit with the roster.** The per-member `vk_grant[member]` records for
    `e+1` are **fields of the `e+1` roster directory**, so the **full** `new_roster_hash` covers their
    actual ciphertexts and signatures. Grants themselves bind only the earlier grant-free
    `roster_core_hash`, **never `new_roster_hash`**. A reader accepts new-epoch grants only when
    accepting the committed transition; no independent grant publish constitutes an epoch commit.
    This is an acceptance rule, not a claim that staged grant ciphertext is inaccessible before
    commit or that N-21/N-22 are resolved (K.16, N-15).
  - **Causal cut for the old roster.** The transition is a **causal cut**: records authorized by the
    epoch-`e` roster are valid only as **ancestors of `manifest_head`** (i.e. in `pred_frontier`'s
    history). A record citing the old roster/epoch that is **not** an ancestor of the transition is
    handled by author (ADR-021.1 §E check 6): a **revoked** device/member's such record is **rejected**
    from history (its un-merged offline writes are lost), while a **still-current** member's is
    **quarantined and re-authored under `e+1`** rather than discarded. This stops a just-revoked device
    from continuing to author "valid" old-roster history after rotation without losing honest offline
    work by remaining members.
  - **Which epoch authorizes writes during the transition.** Until the transition commits, writes are
    authorized under the `e` roster; after commit, under `e+1`. There is no window in which an
    undefined roster authorizes writes.

- **Changes vs. stays on an epoch rotation.**

| Changes | Stays unchanged |
|---|---|
| Vault Key (`VK_e → VK_{e+1}`), re-granted per member via HPKE (§G) | Existing ciphertext (old-epoch objects, never re-sealed; read via archive) |
| Current epoch counter | `.cook` source-of-truth files (ADR-002) |
| Roster directory (new signed entry + new `vk_grant`s, revoked member dropped) | MUK / KEK of every remaining member |
| DEKs for **new** writes (epoch-scoped, §F) | **`NS_objectid`** vault-lifetime namespace key — epoch-invariant (ADR-021.1 §I.2) |
| Epoch-key archive (gains `archive[e]`) | Member identity keys + stable self-wrapped package (from `identity_seed`, §M) |

- **Passphrase / Secret-Key change (no rotation).** Re-derive MUK (new passphrase and/or Secret
  Key) → re-derive KEK → re-wrap that member's **stable package** (`NS_objectid ‖ identity_seed`, §G).
  One durable symmetric wrap; the Vault Key, epoch, roster membership, archive, and all data are
  unchanged. Publishing the wrap in a chained directory successor changes the core's predecessor
  commitment and requires fresh grants of the **same current VK** to every target member (§G),
  produced by an authorized admin without touching other stable wraps. The stable package holds no
  Vault Key; no data re-encryption or other member's KEK is needed. Previously authenticated history
  still uses its original full directory hash and grants.
- **Honest forward-only limit.** As §H states plainly, a revoked member who kept `VK_e` (or
  old-epoch ciphertext already downloaded) can still decrypt everything that existed **at revocation
  time**; rotation protects only post-revocation writes. Optional lazy/background re-encryption
  (K.9, kept optional/best-effort) shrinks but cannot eliminate that window — the member already saw
  the plaintext. Separately, because the object-id namespace key is epoch-invariant (§I.2), a revoked
  member who learned it can keep enumerating `object_id`s — **metadata**, never content (cross-ref
  ADR-021.1 §G).

### `FONDENC1` → `FONDENC2` migration algorithm

*Concretizes [§I](#i-fondenc1--fondenc2-migration-one-time) — the one-time migration.*

Runs **once**, on first `fond sync setup` / first hierarchy-backed encrypted export.

```mermaid
flowchart TD
    A[Vault crypto state] --> B{FONDENC1 blob present and no migration marker?}
    B -->|no, marker present| Z[No-op - already migrated]
    B -->|yes| P{Legacy Argon2 params within compiled allowlist?}
    P -->|no| X[Refuse before KDF - flag as untrusted legacy header]
    P -->|yes| C[open_bundle with existing KeyMaterial]
    C --> INV[Write durable migration inventory]
    INV --> D[Generate Vault Key and epoch-0 roster; owner stable-wrap + self HPKE grant]
    D --> E[Split OverlayBundle into per-object plaintext units]
    E --> F[Seal each unit as a FONDENC2 object at epoch 0]
    F --> G[Write to temp, fsync, atomic rename, fsync parent dir]
    G --> H[Write migration marker, fsync parent dir]
    H --> I[Retain or securely delete FONDENC1 blob]
    I --> J[Done - FONDENC2 authoritative]
```

1. **Detect (idempotency guard).** Inspect the vault crypto state against the **durable migration
   inventory** (below): a `FONDENC1` blob (magic `b"FONDENC1"`) with **no** inventory/marker ⇒
   migrate; a completed inventory with its expected `FONDENC2` object ids/hashes present ⇒ **no-op**,
   return success; an incomplete inventory (crash mid-migration) ⇒ **verify/resume from the
   inventory**, never restart destructively and never infer completion merely from "some `FONDENC2`
   object exists".
2. **Reject legacy params before the KDF — decided (A0.5, N-06).** FONDENC1 was designed to travel
   over **untrusted** file sync (ADR-019), so a local blob is **not** automatically trusted. Before
   calling `open_bundle`, the migrator reads the FONDENC1 header's `m_cost`/`t_cost`/`p_cost` and
   **rejects any triple outside a small compiled allowlist/cap** — the same reject-before-derive
   discipline A0.3 pins for FONDENC2 (§D) — so an attacker-planted legacy header cannot drive Argon2
   into a pre-auth resource-exhaustion before authentication. Only after the params pass the
   allowlist does step 3 run.

   > **Implementation-task note (A0.5-r2, N-06 — not a spec change).** The round-2 review confirmed
   > the *spec* allowlist above is correct but that the current **code** still derives from
   > unauthenticated `m_cost`/`t_cost`/`p_cost` in **both** `open_bundle` and `open_blob` (the latter
   > reachable from `backup.rs`) **before** the allowlist/cap check. Enforcing the allowlist **before**
   > `Params::new` / any Argon2 allocation in *both* openers is an **A1 implementation task**
   > ([#121](https://github.com/kafkade/fond/issues/121)), not resolvable in this paper spec. It stays
   > tracked there; the spec is unchanged.

3. **Open legacy.** Decrypt the single `FONDENC1` bundle with the existing `KeyMaterial` (keychain
   raw key `MODE_KEYCHAIN`, or passphrase `MODE_PASSPHRASE`) via `open_bundle` — the only place
   legacy Argon2 params are read, and only after step 2's allowlist check.
4. **Write the durable migration inventory.** Persist a record binding `{ source_blob_hash,
   expected_object_ids[], expected_object_hashes[], epoch-0 roster_hash, migration_version }` to
   durable storage (outside `fond.db`) **before** committing outputs, so detection/resume in step 1
   is executable and completion is never guessed.
5. **Bootstrap hierarchy.** Generate the random 32-byte Vault Key and the vault-lifetime
   `NS_objectid` (§F) and a random owner `identity_seed`; derive the owner's MUK/KEK (§C, under a
   chosen `kdf_profile_id`) and the owner identity keypair from `identity_seed` (§M); create the
   **epoch-0** roster directory carrying the owner's **self-wrapped stable package**
   (`NS_objectid ‖ identity_seed`, §G) **and** a self-addressed HPKE `vk_grant` of `VK_0` to the
   owner's own X25519 identity key. Follow §G's genesis order: encode/hash the owner-only core with
   epoch 0 and a zero predecessor; seal/sign its core-bound grant; assemble/sign the directory's
   unsigned body with the locally trusted owner key; only then derive the full epoch-0
   `roster_hash` for the existing migration inventory/genesis manifest. Genesis does not trust
   arbitrary server-supplied owner keys.
6. **Split.** Partition the decrypted `OverlayBundle` into per-object plaintext units at the decided
   granularity (§F / K.6 — one object per mergeable record). Photos are already per-file.
7. **Seal.** For each unit, derive its DEK from `VK_0` (§F) and seal it as a `FONDENC2` object (§E)
   at **epoch 0**.
8. **Commit atomically (crash-safe).** Write all new objects + the epoch-0 roster into a
   **temporary** staging location; `fsync`; **atomically rename** into place; **`fsync` the parent
   directory**. Only after the rename is durable, write the migration marker and again **`fsync` the
   parent directory**. The live vault is never half-converted: either the legacy blob is
   authoritative (pre-rename) or the `FONDENC2` set is (post-rename), and the inventory records which.
9. **Dispose legacy.** Per user choice, **retain** the `FONDENC1` blob (default, safest — K.15) or
   **best-effort delete** it. Secure erase is **not guaranteed** on SSD / copy-on-write filesystems
   and cryptographic erasure is unavailable while the old passphrase/key may still exist; the delete
   is described as best-effort, not a guarantee (K.15, decided).

Invariants:

- **Idempotent.** Step 1 makes re-runs no-ops; a crash at any point leaves either a clean pre- or
  post-migration state (step 6), so a re-run completes or no-ops, never corrupts.
- **Lossless & edit-preserving.** Every overlay record maps to exactly one `FONDENC2` object;
  existing user edits are never overwritten (import-idempotency house rule).
- **`.cook` files untouched.** Migration re-frames only the derived overlay/photo blobs; the `.cook`
  source of truth (ADR-002) is never read or written.
- **Runs once.** Thereafter, transfers upload already-encrypted `FONDENC2` blobs with **no**
  re-encryption — the point of the hierarchy.

### A0.5 remediation status & validation

- **A0.3 items (K.13–K.16), N-33-only post-review correction.** **K.14** (deprecation
  lifecycle — no silent re-wrap) and **K.15** (legacy-blob disposition) are **resolved**; **K.16**
  (rotation atomicity) is **not closed** — the core-before-grant-before-full-hash order corrects
  the structural N-33 cycle, pending human validation, but `archive_ref` derivation and the
  prepared→committed representation remain `[Validation Required]` (N-21/N-22);
  **K.13** (Argon2 figures & budget,
  decomposed into `VR-020-K13.1`–`VR-020-K13.5`) **remains deferred** to a human cryptographer with
  measured device evidence, except `VR-020-K13.4` (16-byte salt, resolved); `VR-020-K13.5`
  (per-platform pre-auth ceiling) is **partially resolved** — the policy is decided but actual
  accepted-sets/ceilings stay `[Validation Required]`. Cross-appendix: **K.1** (Argon2 `secret` slot;
  KAT still owed), **K.2** (partially resolved: two constructions, scoped core/context bytes only;
  signing/stable-wrap codec and composition still gated), and **N-06** (spec allowlist correct; the
  code fix in `open_bundle`/`open_blob` is an A1 implementation task, [#121](https://github.com/kafkade/fond/issues/121)).
  Full status is in the FONDENC2 [§K table](#k-open-questions--a05-remediation-status).
- **Not a re-spec of the core.** This appendix pins operational parameters and procedures only; the
  cryptographic core (primitives, envelope, hierarchy) remains the FONDENC2 appendix's. The whole is a
  composition of standard primitives (Argon2id, HKDF, XChaCha20-Poly1305, Ed25519, HPKE), but the
  **composition — including the A0.5-r2 per-member epoch HPKE grant — is novel and its soundness is
  `[Validation Required]`** (N-31), which is exactly why the A0.5 independent review is mandatory
  before any implementation. **This revision does not claim GO.**
