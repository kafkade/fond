# ADR-021: Optional self-hostable zero-knowledge sync server

**Status**: Proposed
**Date**: 2026-07-24
**Decision**: Add an **optional**, self-hostable **`fond-server`** (Axum) that is a
**zero-knowledge, versioned, encrypted-blob store** with per-user accounts (**OPAQUE / RFC 9807**,
ADR-020) — packaged Immich-style (`docker compose`). Devices sync encrypted blobs to it; **all merge
logic runs client-side** (the server never reads content). File-sync-first (ADR-012) remains the free
default; this server is an *addition* for users who want an always-on, zero-knowledge hub, and its
build is **gated on demonstrated demand** (ADR-022; the ZK-Sync "paid pilot" work item).
This **supersedes ADR-012**: ADR-012 rejected a custom server for **two** reasons — that it would be
*mandatory* **and** that its *scope* was unjustified. This ADR answers the first (the server is
strictly optional) and answers the second only conditionally (build it only if demand is proven).
Extends ADR-015 (merge semantics, revised to sibling-retaining) and ADR-019 (crypto). **Contingent on
the Epic A0 protocol spec + independent crypto review; no sync/server code lands before that.**

## Context

[ADR-012](012-sync-multi-device.md) chose **file-sync-first** and explicitly **rejected** a custom
fond sync server for **two** reasons: it would be *mandatory infrastructure* contradicting
local-first, **and** its *scope* (a full multi-user backend) was unjustified for the value. This ADR
addresses the first squarely — the server never gates core features (`CONTRIBUTING`: no server for
core), stores only ciphertext, and coexists with file-sync — but the second reason still stands
unless demand is demonstrated. Hence the server is **not** greenlit by this ADR alone; it is gated on
the paid-pilot / unit-economics decision in ADR-022 (Epic E1). If demand is not shown, the correct
outcome is **not to build it**.

Real gaps file-sync (Syncthing/iCloud/Dropbox) leaves open:

- No always-on peer for devices that are rarely online together (phone ↔ laptop).
- NAT/relay friction for peer-to-peer.
- No encrypted-at-rest custody on an untrusted always-on host.
- No clean per-user account model for a future kafkade-hosted offering (ADR-022).

ADR-020 provides the identity/keys (see its
[FONDENC2 protocol](020-zero-knowledge-identity.md#appendix-fondenc2-protocol) appendix for the
vault key hierarchy these sync blobs consume); ADR-015 (revised) provides client-side merge
semantics; ADR-019 provides the crypto primitives. This ADR provides the optional hub.

## Threat model

**In scope:** a curious or breached server operator (self-host neighbor, cloud host, or kafkade).
They must learn no recipe/notes/photo **content** from storage, backups, or transit. AEAD makes each
blob tamper-evident **individually** — but AEAD alone does **not** prevent replay, withholding,
deletion, equivocation, or rollback of *which* blobs are served. Those require the anti-rollback
machinery below and are **mandatory**, not future work.

**Multi-tenant server threats (in scope):** IDOR / cross-account object access, cross-account
overwrite, presigned-URL leakage, quota/storage theft, abuse of an opaque-blob store to host
arbitrary content, and a racy/hijackable first-admin bootstrap. The server enforces per-tenant
authorization on every object, ownership checks, quotas, rate limits, and audit logging, and uses a
**bootstrap token** (not "first request wins") for initial admin creation.

**Supply-chain (in scope, partially):** self-hosting removes kafkade from the **storage** trust path
but **not** the **software** trust path — clients still run kafkade-built binaries, an update channel,
and dependencies, and perform TOFU on the `server` URL. Mitigations: reproducible builds, signed
releases, and **server-certificate/URL pinning** after first use.

**Out of scope / documented residual risks:**

- **Metadata**: the server learns blob counts, sizes, change timestamps, device IDs. Mitigated (not
  eliminated) by blob padding (deferred hardening). Object identifiers are **opaque HMACs**, so they
  do **not** leak titles or enable known-file matching.
- **Withholding**: a malicious server can refuse to serve the latest blobs. This is **unpreventable**;
  clients detect *rollback/equivocation* via a signed hash-chained manifest + trusted device state,
  but cannot force a server to return data. Stated honestly to users (see
  [Appendix §E and §G](#appendix-authenticated-causal-history-adr-0211)).
- **Billing identity** (kafkade-hosted only): email/payment is necessarily plaintext (ADR-022),
  outside the zero-knowledge boundary. Self-host avoids it entirely.

## Decision

### `fond-server` crate (distinct from `fond serve`)

A **new crate/binary** `fond-server`, separate from Phase 4's local `fond serve` web UI. Rationale:
the trust model (public, multi-user, internet-facing, ciphertext-only) differs fundamentally from
the local single-user renderer. It reuses `fond-core`/`fond-domain` types but assumes **no plaintext
access** to content.

- **Stack:** Axum + Postgres (accounts, OPAQUE records, encrypted-blob metadata, signed manifest
  checkpoints) + object storage (filesystem volume by default; S3/R2 optional). Per-tenant
  authorization, quotas, rate limits, and audit logs are first-class.
- **Per-user / household libraries:** each household has an isolated encrypted-blob namespace with
  per-member OPAQUE accounts (ADR-020's multi-member vault). Every object access is authorization-
  checked against the requesting member (no IDOR). Admin onboarding uses a **bootstrap token**. See
  [Appendix ADR-021.2](#appendix-account-authentication--pake-selection-adr-0212) for the PAKE
  selection and the auth-vs-vault-authorization boundary.

### Sync protocol (client-driven, zero-knowledge)

- **Unit = an encrypted blob** addressed by an **opaque, keyed identifier** (HMAC under a namespace
  key), listed only in an **encrypted manifest**. Plaintext `recipe_slug`, content hashes, and
  UUIDv7 timestamps are **never** sent (slugs leak titles, content hashes enable known-file matching,
  UUIDv7 leaks time). Note: recipes are **slug-only today** (V010 gave UUIDs only to overlay rows) —
  ADR-020/A4 introduces a durable recipe UUID in `.cook` frontmatter for stable identity. See
  [Appendix §B and §H](#appendix-authenticated-causal-history-adr-0211).
- Causal history is tracked by **per-device version vectors** (not wall-clock `updated_at`, which is
  skew-unsafe) and protected by a **signed, hash-chained manifest** with checkpoints. Clients verify
  the chain against **trusted local device state** to detect rollback, fork, and equivocation. See
  [Appendix §C–§E](#appendix-authenticated-causal-history-adr-0211).
- **Merge is client-side and sibling-retaining** (revises ADR-015): a whole recipe body is **never**
  last-writer-wins. Concurrent edits produce retained siblings resolved by three-way / `.cook`-aware
  merge or an explicit user prompt; append-only logs union; deletes propagate via **tombstones**. See
  [Appendix §F](#appendix-authenticated-causal-history-adr-0211).
- **Exactly one authoritative transport per library:** running file-sync and server-sync on the same
  library simultaneously creates two uncontrolled writers and is refused. The client also warns if
  plaintext `.cook` file-sync targets the **same host** as the ZK server (which would defeat
  zero-knowledge).
- After a pull, the client decrypts, verifies the manifest, merges, writes `.cook`/overlay, and runs
  `reindex` — `fond.db` is never synced (ADR-002/012).

### Packaging & config

- `docker compose up` with server + Postgres + object storage; `.env`; a web onboarding page that
  consumes a one-time **bootstrap token** to create the first admin (Immich-style, but not
  "first request wins"). Clients pin the server URL/cert on first use (TOFU).
- Devices point at the server via `config.toml`:

  ```toml
  [sync]
  server  = "https://fond.example.home"
  account = "alice@example.com"
  # Secret Key lives in the OS keychain / Emergency Kit, never in config.toml
  ```

- CLI: `fond sync setup <url>`, `fond sync login <url>`, `fond sync status`, `fond sync now`.

## Rationale

- **Optional, not mandatory, and demand-gated:** upholds Principle #1 and `CONTRIBUTING`; file-sync
  stays the free default; the server is built only if Epic E1 proves demand.
- **Zero-knowledge by construction:** a full server-DB + object-store dump yields no content, and
  opaque identifiers leak no titles or known-file matches.
- **Tamper-evident *and* rollback-evident:** AEAD plus a signed hash-chained manifest and trusted
  device state; withholding is stated as an unpreventable residual.
- **Sibling-retaining merge:** no silent whole-recipe data loss (revised ADR-015).
- **Self-host parity:** kafkade-hosted (ADR-022) runs the *same* code, preventing ecosystem
  fragmentation.

## Alternatives Considered

| Alternative | Rejected because |
|---|---|
| Keep ADR-012's "no server" absolutely | Leaves a real always-on/NAT gap; an *optional*, demand-gated ZK server doesn't violate local-first. But "don't build it" remains the correct outcome if E1 shows no demand. |
| Reuse `fond serve` (Phase 4) as the sync server | Different trust model (public multi-user vs local single-user); risks exposing a local UI to the internet. |
| SRP-6a for accounts | A stolen verifier enables offline guessing; OPAQUE (RFC 9807) preferred, SRP a reviewed fallback only. See [Appendix ADR-021.2 §B](#b-why-opaque-rfc-9807-not-srp-6a). |
| Whole-recipe last-writer-wins merge | Silent data loss on concurrent edits; sibling-retaining three-way merge required instead. |
| Server-side plaintext with "optional" encryption | Violates the zero-knowledge requirement and ADR-019; makes any hosted offering a liability. |
| Server performs the merge | Impossible under zero-knowledge (can't read bodies); merge must be client-side. |
| Federation/P2P instead of a hub | ADR-017 covers decentralized *sharing*; the gap here is an always-on *sync* hub. |
| `cr-sqlite` CRDT over encrypted blobs | Per-row causal metadata reconciles poorly with opaque blobs; version-vector + sibling merge suffices. Kept as unused fallback. |

## Consequences

- **New crate `fond-server`** → workspace + CI matrix change. **If a new required status check is
  added, update OpenTofu `kafkade/github-infra` `repos/fond/main.tf` or PRs will be blocked.**
- **Gated twice:** on Epic A0 (protocol spec + crypto review) *and* Epic E1 (demonstrated demand /
  unit economics). Neither the client sync engine nor the server is built before both clear.
- New client sync engine in `fond-store`/`fond`; new `config.toml [sync]` block; new `fond sync`
  commands; single-authoritative-transport enforcement.
- Depends on ADR-020 (identity/keys, multi-member vault, recipe UUID) and revised ADR-015 (merge).
  Enables ADR-022 (kafkade-hosted) and ADR-023 (server-side backup).
- Mandatory anti-rollback (signed manifest), multi-tenant authorization, and supply-chain hardening
  (reproducible builds, signed releases, TOFU/pinning) require a threat-model doc; blob padding is
  post-2.0 hardening.
- Recommended as a **2.0 major** *if built at all* (introduces the optional server + account concept).

## Appendix: authenticated causal history (ADR-021.1)

This appendix specifies the **authenticated causal history** that the Threat model and the
`### Sync protocol (client-driven, zero-knowledge)` Decision reference but do not construct. It is
what makes sync safe against a **malicious server**: AEAD (the FONDENC2 envelope, ADR-020) makes
each blob tamper-evident *individually*, but does **not** by itself prevent a server from replaying,
withholding, deleting, forking, or rolling back *which* blobs it serves. The machinery here —
per-device version vectors, a signed hash-chained manifest with checkpoints, opaque keyed object
ids, and tombstoned sibling-retaining merge — closes those gaps to the extent they are closable, and
states plainly the one that is not (withholding).

Like [ADR-020's FONDENC2 appendix](020-zero-knowledge-identity.md#appendix-fondenc2-protocol) and
[ADR-023's FONDBKP1 appendix](023-backup-and-recovery.md#appendix-fondbkp1-wire-format-adr-0231),
the authoritative byte-level specification will live as module documentation in the sync engine
(`fond-store` / `fond-server`) **once implemented**; this appendix records the shape so the ADR is
self-contained. It **composes reviewed primitives** — Ed25519 signatures, HMAC-SHA-256, hash chains,
version vectors, tombstones, three-way merge — and hand-rolls nothing.

**Gate reminder:** this is a paper spec. **No crypto/sync code lands before the Epic A0 independent
review (A0.5) clears.** Every `[Validation Required]` tag marks a choice the A0.5 reviewer must sign
off on; unresolved decisions are collected in section I rather than decided silently. This appendix
**expands** — and does not restate or contradict — the Decision and Threat-model text above, into
which the Sync-protocol bullets carry one-line "see Appendix" pointers.

**Keys are reused from FONDENC2, never re-invented.** The manifest signer, the manifest object's
encryption key, and the object-id namespace key all derive from the ADR-020 hierarchy:

| This appendix uses | Derived from (ADR-020) |
|---|---|
| object-id namespace key (§B) | the `object-id` purpose subkey, [§F label table](020-zero-knowledge-identity.md#f-per-object-dek-derivation--object-granularity) |
| manifest confidentiality (§D) | the `manifest` purpose subkey (§F), sealed as a FONDENC2 object `object_class = manifest` ([§E envelope](020-zero-knowledge-identity.md#e-envelope--wire-format-per-object)) |
| manifest authorship (§D–§E) | the member's Ed25519 roster/manifest signing key ([§G roster](020-zero-knowledge-identity.md#g-per-member-vault-key-wrapping--the-roster)) |

### A. What AEAD alone cannot do

The FONDENC2 envelope authenticates each blob **in isolation**: a client that opens an object knows
its bytes came from a Vault-Key holder and were not altered. A malicious or breached server never
sees plaintext, but it **does** control delivery, and AEAD says nothing about *the set and order of
objects served*. Five distinct attacks survive AEAD:

- **Replay** — re-serving a superseded but validly-signed blob as if current.
- **Withholding** — refusing to serve the latest blob(s).
- **Deletion** — dropping a blob (or a delete) so a change never propagates.
- **Rollback** — presenting an older, internally-consistent state as the head.
- **Equivocation / fork** — showing device A one history and device B a divergent one.

This appendix adds the three things AEAD lacks: a **causal order** independent of server-controlled
clocks (§C), an **authenticated, append-only history** the server cannot silently rewrite (§D–§E),
and **merge/delete semantics** that never lose a concurrent edit (§F). It cannot defeat withholding;
§G states that honestly.

### B. Opaque, keyed object identifiers

The sync unit is one encrypted blob, addressed by an identifier that must be **stable across
devices** yet **leak nothing** to the server. The three obvious identifiers all leak:

- **Slugs** (`chicken-adobo`) leak the recipe **title** directly.
- **Content hashes** (`blake3(bytes)`) let a server that guesses a plaintext — a public recipe —
  **confirm** its presence (known-file matching) and reveal when two members hold the same file.
- **UUIDv7** embeds a **millisecond timestamp**, leaking creation/edit **time**.

Instead the id is a keyed pseudonym under a household-secret namespace key:

```text
object_id      = HMAC-SHA-256(namespace_key, canonical_input) truncated to 16 bytes
namespace_key  = subkey_{object-id}             # ADR-020 §F `object-id` purpose subkey
canonical_input = object_class ‖ durable_uuid   # durable_uuid: see §H
```

- **Why keyed, not a bare hash.** Only Vault-Key holders can compute the id, so the server cannot
  correlate an id to any guessed plaintext (defeats known-file matching) and cannot mint ids.
- **Deterministic across devices.** Every member derives the *same* `object_id` for the same object
  because `namespace_key` and `durable_uuid` are shared — this is what lets independent devices
  converge without a server-assigned key.
- **Width.** 16 bytes matches the FONDENC2 envelope `object_id` field
  ([§E](020-zero-knowledge-identity.md#e-envelope--wire-format-per-object)); truncated HMAC-SHA-256
  retains ≥128-bit collision resistance across a household's object count.

`[Validation Required]` (I.1) the exact `canonical_input` fields (whether `object_class`, a
sub-object index, or a per-recipe part id are included) and the truncation width (16 vs. 32 bytes) —
aligns with ADR-020 open question K.11 (`object_id` source & width).

`[Validation Required]` (I.2) the namespace key must be **epoch-invariant**. Unlike per-object DEKs —
which are epoch-scoped so rotation re-keys them — `object_id` must **not** change when the vault
rotates epochs, or a rotation would orphan every object's identity. But ADR-020 §F roots *all*
subkeys (via `PRK_vault`) in the **per-epoch** Vault Key, so a subkey named `object-id` would still
change each rotation. A0.5 must decide how to root the `object-id` namespace key in a
**rotation-invariant** vault-lifetime secret while keeping DEKs epoch-scoped.

### C. Per-device version vectors

Causality is tracked with a **version vector** (vector clock), **not** wall-clock `updated_at` —
device clocks skew, and a server that controls timestamps could reorder history for free.

**Structure.** A version vector is a map `VV : device_id → u64 counter`. Each device owns exactly one
component and increments **only its own** counter, by 1, on each local mutation of the object it
stamps. Absent components are implicitly 0.

**Causality.** For two vectors `A` and `B`:

- `A` **happens-before** `B` (`A → B`) iff `A[d] ≤ B[d]` for every device `d`, and `A ≠ B`.
- `A` and `B` are **concurrent** (`A ‖ B`) iff neither happens-before the other — each leads on some
  device. Concurrency is exactly the condition that produces a **sibling** (§F).
- Equal vectors denote the same version.

This partial order is fixed by the writers themselves; the server cannot forge it without a device's
signing key, because the vector travels **inside** the signed manifest (§D).

**Join / leave.**

- A **new device** joins with an all-zero implicit vector and appears in the vault **roster**
  (ADR-020 §G) before its writes are accepted; its first write introduces its `device_id` component.
- A **departed / revoked** device stops writing and is removed from the roster, but its **past
  components remain** in existing vectors as immutable history — they must, or prior causality
  breaks. Epoch rotation (ADR-020 §H) revokes its *key*, not its *history*.

**Baseline (adopted).** A **single library-wide signed manifest chain** (§D) whose **entries each
carry a per-object version vector** — one chain to verify, with per-object concurrency detection.
`[Validation Required]` (I.3) whether to split into **finer-grained per-object sub-chains** (more
parallelism and smaller diffs, but N chains to verify and correlate) is deferred to A0.5; the
library-wide chain is the default, not an open choice. `[Validation Required]` (I.4) the `device_id`
derivation and whether each device carries its own subkey/signing key for per-device revocation —
ties to ADR-020 open question K.7.

### D. Signed, hash-chained manifest & checkpoints

The **manifest** is the authenticated index of the library: it names every live object, its version
vector, and a binding to its ciphertext, and it is **append-only and hash-chained** so the server
cannot rewrite history undetected.

```text
┌─ manifest record (plaintext shape; sealed as a FONDENC2 object, class = manifest) ─┐
│ manifest_seq        u64      monotonic per-library sequence number                  │
│ prev_manifest_hash  32 bytes hash of the previous record's canonical bytes (0…0 at  │
│                              genesis)                                                │
│ vault_epoch         u32      FONDENC2 epoch in force (ADR-020 §F)                    │
│ author_device       bytes    device_id of the signer (a current-roster member)      │
│ is_checkpoint       u8       1 = self-contained snapshot (see below)                 │
│ entries[]           list     one per live object:                                   │
│    ├ object_id      16 bytes opaque id (§B)                                          │
│    ├ version_vector map      { device_id → counter } (§C)                            │
│    ├ blob_hash      32 bytes hash of the object's FONDENC2 ciphertext                │
│    └ tombstone?     opt      present iff the object is deleted (§F)                  │
├─ signature ──────────────────────────────────────────────────────────────────────────┤
│ ed25519_sig  64 bytes  over ALL fields above (seq ‖ prev_hash ‖ epoch ‖ author ‖     │
│                        is_checkpoint ‖ canonical(entries)); key = the author's       │
│                        Ed25519 roster/manifest key (ADR-020 §G)                      │
└───────────────────────────────────────────────────────────────────────────────────────┘
```

- **What each link commits to.** `prev_manifest_hash` is the hash of the immediately-preceding
  record's canonical serialization, forming a tamper-evident chain back to the genesis record
  (`prev = 0…0`). Altering any past record changes its hash and breaks every later link.
- **What the signature covers.** The Ed25519 signature covers the **entire** record including
  `manifest_seq`, `prev_manifest_hash`, and every entry's **`version_vector`** and `blob_hash`, so a
  server can neither reposition a record in the chain nor **forge, rewind, or reorder causal state**
  without breaking the tag. Only a **current-roster** member key is accepted (§E check 1), making
  authorship a cryptographic role (ADR-020 §H), not a server assertion.
- **Confidentiality.** The record is sealed as a FONDENC2 object (`object_class = manifest`,
  [§E envelope](020-zero-knowledge-identity.md#e-envelope--wire-format-per-object)) under the
  `manifest` purpose subkey, so the server stores only ciphertext plus the coarse metadata it must
  index. The **hashes and vectors live inside** the encrypted body; the server cannot read them.
- **Checkpoints.** Periodically a device writes an `is_checkpoint` record whose `entries` are the
  **complete** live-object set (not a delta). A checkpoint lets a new device bootstrap without
  replaying the whole chain, bounds how far back verification must walk, and enables **tombstone
  reaping** (§F). The chain continues from the checkpoint's hash.
- **Dovetails the roster chain.** This is the manifest analogue of ADR-020 §G's `prev_roster_hash`
  roster chain; together they make membership *and* content history rollback-evident.

`[Validation Required]` (I.5) the **hash function** — SHA-256 (the HMAC/HKDF family already in
FONDENC2) vs. BLAKE3 (the FONDBKP1 integrity family). `[Validation Required]` (I.6) **checkpoint
cadence**, **who may author** a checkpoint (any member vs. owner/admin), and how concurrent
checkpoints reconcile.

### E. Rollback / fork / equivocation detection

Detection rests on **trusted local device state** — a small, durable, per-device watermark the server
never sees and cannot influence:

```text
trusted_state = { last_seq, last_hash, own_counter }
  last_seq     highest manifest_seq this device has accepted
  last_hash    that record's hash (the head it trusts)
  own_counter  highest counter this device has itself written (§C)
```

**Requirement (not optional).** The watermark **MUST** persist in **durable, tamper-resistant local
storage outside the disposable `fond.db`**. `fond.db` is never synced and is rebuilt from files by
`reindex` (ADR-002/012); if the watermark lived there, a `reindex` or reset would **silently reset
the rollback baseline**, letting a server replay an old head undetected. Only the exact **home** is
open — `[Validation Required]` (I.7) a dedicated local state file vs. the OS keychain alongside the
Secret Key — **not** whether it lives outside `fond.db`.

On every pull the client runs these checks **before** applying anything, and **fails closed**:

1. **Signature & roster.** Every record must carry a valid Ed25519 signature from a **current-roster**
   member (ADR-020 §G). Reject unknown or removed signers — this blocks a server forging history.
2. **Chain continuity.** Each record's `prev_manifest_hash` must equal the prior record's actual
   hash, back to the trusted `last_hash` (or a signed checkpoint). A break is **tampering / fork**.
3. **No rollback.** The served head must have `manifest_seq ≥ last_seq` **and** descend from
   `last_hash`. A head with `seq < last_seq`, or a same-or-higher seq on a chain that does **not**
   include `last_hash`, is a **rollback** → reject.
4. **Own-write monotonicity.** For this device's own component, entries for objects it last wrote must
   satisfy `VV[own_device] ≥ own_counter`. A manifest that **omits or lowers** this device's known
   writes is the server **withholding / rewinding** the device's own history → reject.
5. **No equivocation.** Two validly-signed records at the **same `manifest_seq`** with **different
   hashes**, or two heads that fork below `last_hash`, are **equivocation** — the server showed
   divergent histories. Each device always detects a fork of **its own** timeline; cross-device forks
   are caught only when devices corroborate out-of-band (a shared gossip value or the roster chain).

**Rollback vs. fork — detection is not symmetric.** Checks 3–4 catch **rollback** *immediately and
locally*: a rewind **below what this device has already witnessed** contradicts the trusted watermark,
with no peer required. A **fork/equivocation the device never witnessed** — the server shows *this*
device a self-consistent history while showing *another* device a divergent one — **cannot** be caught
by the local watermark alone. It surfaces only **eventually**, when the two devices later compare heads
(gossip, shared trusted state, or a subsequent manifest that would have to descend from both forks and
cannot). Equivocation detection is therefore **eventual, not instant**, and hinges on devices actually
reconciling.

`[Validation Required]` (I.8) the **response** to a detected fork/equivocation: hard-fail (halt sync,
alert the user) vs. warn-and-quarantine the divergent branch.

### F. Tombstones & sibling-retaining merge

**No whole recipe body is ever last-writer-wins.** Merge is client-side (the server cannot read
bodies) and preserves concurrent work.

**Tombstones (deletes).** A delete is not a missing entry — the server could forge absence — but an
explicit record carried in the manifest entry for that `object_id`:

```text
tombstone = { object_id, deleted_vv, deleter_device }
```

`deleted_vv` is the version vector at deletion, giving a delete a causal position like any other
write.

- **Lifecycle / reaping.** A tombstone is retained until it is **causally dominated on every live
  device** (all devices' vectors happen-after `deleted_vv`) **and** a **checkpoint** (§D) has captured
  the deletion — only then can it be dropped without a lagging device resurrecting the object.
  `[Validation Required]` (I.9) the exact reap predicate, and whether it counts **all roster** devices
  or only **recently-active** ones (a permanently-offline device otherwise pins tombstones forever).
- **Delete races an edit.** If a delete and an edit are **concurrent** (`deleted_vv ‖ edit_vv`, §C),
  the edit is **retained as a live sibling** and the tombstone is surfaced as a **conflict** for the
  user — the edit is **never** silently discarded. A delete removes the object only if it **causally
  dominates** the latest edit (the deleter had already seen it).

**Sibling-retaining body merge.** When two version vectors for a recipe body are concurrent:

1. Attempt an automatic **three-way / `.cook`-aware merge** via the lossless `CookDocument` edit layer
   (ADR-011 / ADR-015) against the common causal ancestor.
2. If it does not cleanly merge, **retain both siblings** and prompt the user to choose or merge —
   nothing is dropped in the meantime.

**Append-only logs** (per-user notes, cook logs) **union** by id, as in ADR-015 — concurrency is not a
conflict there. **Point overlay data** (ratings, pantry presence) may remain last-writer-wins, but
ordered by **version vector**, not wall-clock: the skew-unsafe `updated_at` tiebreak of ADR-015 is
replaced by causal order. This **revises ADR-015's** "additive merges never delete" specifically for
the server-sync transport — deletes now propagate, via tombstones.

### G. Honest limits — what this cannot do

- **Withholding is unpreventable.** A server can serve a **stale-but-internally-valid** prefix of the
  chain, or simply refuse the newest blobs. The client will **detect** that it is stuck (its own
  writes stop advancing, or a peer reports a higher head) but **cannot force** the server to return
  data. Anti-rollback makes withholding **loud, not impossible** — the honest guarantee is
  *detection*, never *availability*. Users are told plainly (Threat model, "Withholding").
- **Cross-device equivocation** with two internally-consistent histories is caught only when devices
  can **compare state out-of-band**, and only **eventually** (§E, "Rollback vs. fork"): a server that
  partitions two devices that never reconcile can keep them forked, and the local watermark alone
  never sees it. Corroboration (shared gossip, the roster chain, an occasional direct
  device-to-device check) shrinks but does not close this.
- **A revoked member can still enumerate identifiers.** Because the `object-id` namespace key is
  **epoch-invariant** (§B), a revoked member who learned it can keep **recomputing and enumerating
  `object_id`s** after revocation — correlating which objects exist and when they change (**metadata**,
  never content; epoch rotation re-keys DEKs but not identifiers). This is the honest cost of stable
  cross-device identity; whether the object-id key should ever rotate — at the price of a coordinated
  re-id pass — is `[Validation Required]` (I.2).
- **Metadata still leaks.** Blob counts, sizes, change **timing**, and device ids remain visible
  (Threat model, "Metadata"); opaque ids (§B) remove *titles* and known-file matching, not traffic
  shape. Blob padding is deferred hardening.

### H. Identity dependency — durable recipe UUID (A4 / #124)

`object_id` (§B) requires a **durable, per-recipe identifier** that is stable across devices and
edits. Today recipes are **slug-only**: migration V010 gave durable UUIDs only to *overlay* rows, not
to recipes themselves, and a slug changes when a recipe is renamed — unusable as a sync identity.

This spec **assumes** a durable recipe **UUID stored in `.cook` frontmatter** (the source of truth,
ADR-002; written via the lossless `CookDocument` layer, ADR-011) supplies `durable_uuid` in the HMAC
input. Introducing that frontmatter key / column is tracked as **A4 (#124)** and is a **hard
dependency** of sync — there is no slug fallback (a slug identity would leak titles and break on
rename). Until A4 lands this appendix is unimplementable, consistent with the A0 gate.

### I. Open questions for A0.5 (independent review)

1. **`object_id` input & width** — exact `canonical_input` fields; 16- vs. 32-byte truncation (aligns
   ADR-020 K.11). (§B)
2. **Object-id key rotation-invariance** — root the `object-id` namespace key in a
   **rotation-invariant** vault-lifetime secret rather than the per-epoch Vault Key, so ids survive
   epoch rotation while DEKs stay epoch-scoped (ADR-020 §F roots all subkeys in the per-epoch Vault
   Key today). Corollary: because it does **not** rotate, a revoked member retains the ability to
   enumerate/correlate `object_id`s (metadata linkage, §G) — should the object-id key ever rotate, at
   the cost of a coordinated re-id pass? (§B, §G)
3. **Manifest / VV granularity** — baseline is one **library-wide** signed chain with **per-object**
   version-vector entries; whether to split into finer-grained **per-object sub-chains** is the open
   choice. (§C, §D)
4. **`device_id` & per-device keys** — derivation and per-device signing/revocation (ties ADR-020
   K.7). (§C)
5. **Manifest hash function** — SHA-256 vs. BLAKE3. (§D)
6. **Checkpoint cadence & authority** — frequency; who may author; concurrent-checkpoint
   reconciliation. (§D)
7. **Trusted-state home** — the anti-rollback watermark **MUST** live outside `fond.db` in durable,
   tamper-resistant storage (firm requirement, §E); open only: **which** store — a dedicated local
   state file vs. the OS keychain. (§E)
8. **Equivocation response** — hard-fail vs. warn-and-quarantine. (§E)
9. **Tombstone reap predicate** — dominance across which device set; interaction with
   permanently-offline devices. (§F)
10. **OPAQUE crate & version pin** — `opaque-ke` (facebook/novi, RFC 9807) at a pinned version vs.
    any alternative; confirm the cited third-party review actually covers that release. `[Validation
    Required]` (ADR-021.2 §C).
11. **OPAQUE ciphersuite & KSF binding** — OPRF group, hash/KDF/MAC, and AKE choice, and binding the
    OPAQUE key-stretching function to the **A0.3 Argon2id profile registry** so the vault has one
    Argon2 regime, not two. `[Validation Required]` (ADR-021.2 §F).
12. **Identity ↔ OPAQUE binding construction** — envelope-embedded (OPAQUE `client_identity` /
    `export_key`-encrypted client state) vs. a sidecar Ed25519 self-signed attribute, for committing
    the vault identity public keys to the account. Resolves **ADR-020 K.10** in principle; the
    construction is the residual. `[Validation Required]` (ADR-021.2 §E).
13. **Non-resettable-attribute invariant** — how the server guarantees a login/password **reset**
    replaces the OPAQUE record but **never** rewrites the bound vault identity public key, plus the
    reset authority and flow. `[Validation Required]` (ADR-021.2 §D, §E).
14. **Vault-authorization signature format** — the exact Ed25519 signature (message canonicalization,
    a `fond/fondenc2/v2/...` domain-separation label, and which roster key signs) required on every
    destructive / key-material op, and the server's verification obligation. `[Validation Required]`
    (ADR-021.2 §D).
15. **SRP-6a fallback conditions** — the precise circumstances (if any) under which the reviewed
    SRP-6a fallback engages, confirming it is never the default and never hand-rolled. `[Validation
    Required]` (ADR-021.2 §B, §C).
16. **Passphrase two-use domain separation** — the same passphrase feeds the OPAQUE login KSF and the
    MUK Argon2id (A0.3 / ADR-020 §C); pin how they are domain-separated (distinct
    `fond/fondenc2/v2/...` labels vs. the structural OPRF-secret / Secret-Key-pepper / distinct-salt
    argument) so neither derivation's output aids attacking the other, and confirm that reusing the
    A0.3 profile registry shares parameters only, never a derived value. `[Validation Required]`
    (ADR-021.2 §F).

## Appendix: account authentication — PAKE selection (ADR-021.2)

This appendix concretizes the **account-authentication** primitive that ADR-021's Decision (the
[`fond-server` crate](#fond-server-crate-distinct-from-fond-serve) and its per-user **OPAQUE /
RFC 9807** accounts) names but does not construct, and draws the line between *authenticating to the
service* and *authorizing changes to the vault*. It **references** rather than restates: the vault
key hierarchy lives in
[ADR-020's FONDENC2 appendix](020-zero-knowledge-identity.md#appendix-fondenc2-protocol), the
Argon2id key-stretching profiles in its
[A0.3 appendix](020-zero-knowledge-identity.md#appendix-kdf-profiles-rotation--migration-a03), the
member roster and Ed25519 signing keys in
[§G](020-zero-knowledge-identity.md#g-per-member-vault-key-wrapping--the-roster) /
[§H](020-zero-knowledge-identity.md#h-enrollment-roles-invitation-revocation-epoch-rotation), and the
signed causal history in the
[ADR-021.1 appendix](#appendix-authenticated-causal-history-adr-0211) above. It reuses their exact
vocabulary (MUK, Vault Key, member KEK, epoch, roster, `wrapped_vault_key`, Ed25519 roster/manifest
signatures, X25519, `fond/fondenc2/v2/...` labels) and **never hand-rolls a PAKE**.

**Gate reminder:** this is a paper spec. **No auth/sync code lands before the Epic A0 independent
review (A0.5) clears.** Every `[Validation Required]` tag marks a choice the A0.5 reviewer must sign
off on; unresolved choices are appended to the ADR-021.1
[§I](#i-open-questions-for-a05-independent-review) list (as I.10–I.16) rather than decided here. Per
the honesty rule of this spike, **no crate is "audited/validated" *for fond* until A0.5 signs off** —
a *prior* third-party review of a dependency is cited as evidence, never as validation of the
selection.

### Scope & pointer map

| This appendix subsection | Concretizes | Acceptance criterion (#119) |
|---|---|---|
| A. Threat model for account authentication | ADR-021 [Threat model](#threat-model); ADR-020 [Threat model](020-zero-knowledge-identity.md#threat-model) | #4 |
| B. Why OPAQUE (RFC 9807), not SRP-6a | ADR-021 Alternatives `SRP-6a for accounts`; ADR-020 Alternatives `SRP-6a as the primary PAKE` | #2 |
| C. Rust crate evaluation & selection | ADR-021 Decision (per-user OPAQUE accounts); ADR-020 [§J primitives](020-zero-knowledge-identity.md#j-primitives) | #1 |
| D. Service authentication vs. vault authorization | ADR-020 [§G](020-zero-knowledge-identity.md#g-per-member-vault-key-wrapping--the-roster) / [§H](020-zero-knowledge-identity.md#h-enrollment-roles-invitation-revocation-epoch-rotation) roster & Ed25519 | #3 |
| E. Binding vault identity keys to the OPAQUE record | ADR-020 [K.10](020-zero-knowledge-identity.md#k-open-questions-for-a05-independent-crypto-review) | #3 |
| F. Chosen ciphersuite, parameters & A0.5 sign-off | ADR-020 [§J primitives](020-zero-knowledge-identity.md#j-primitives); A0.3 Argon2id registry | #4 |

It does **not** redefine the vault key hierarchy, the per-object envelope, DEK derivation, or the
roster — those remain authoritative in ADR-020 and are only cited.

### A. Threat model for account authentication

The adversary is ADR-021's **curious or breached server operator** (self-host neighbor, cloud host,
or kafkade), now aimed specifically at the login layer. Account authentication must survive:

- **Server-storage compromise.** A full dump of the server DB (Postgres OPAQUE records + object
  store) must yield **no offline-guessable password verifier** and **no ability to forge vault
  authorization** (§D). This is the property SRP-6a fails and OPAQUE is chosen for (§B).
- **Pre-computation.** An attacker must not be able to build a dictionary / rainbow table *before*
  compromise that is then instantly usable *after* it. OPAQUE's oblivious PRF ties password hardening
  to a **server-side OPRF secret**, so no pre-computed table applies until that secret is also stolen
  — and even then each guess still pays the key-stretching cost (§F).
- **Online guessing.** The server (the service-auth layer) rate-limits and audit-logs OPAQUE login
  attempts (ADR-021 multi-tenant threats); this is the one place an interactive guess is possible,
  and it is bounded by policy, not cryptography.
- **Passive transit / MITM.** OPAQUE never transmits the password or a password-equivalent; the
  blinded OPRF evaluation and the AKE transcript reveal nothing offline-usable to a network or server
  observer.

**The OPRF's role.** On every login the client *blinds* its password, the server evaluates the OPRF
under its secret, and the client unblinds — the server learns nothing about the password and the
client obtains a value it could not have pre-computed. This is what removes the pre-computation
advantage that plain salted-hash and SRP verifiers retain.

**Where the two-secret model backstops this (cross-ref ADR-020).** Even a *worst-case* full server
compromise that also recovers the OPRF secret and mounts a key-stretch-bounded offline dictionary
attack against a weak passphrase only yields the **account credential** — i.e. *service access*. It
does **not** yield the Vault Key: unwrapping requires the **MUK = Argon2id(passphrase, secret =
Secret Key, …)** and the **Secret Key never leaves the device** (ADR-020 two-secret model). Nor does
it yield **vault authorization**, which requires an Ed25519 vault identity key the server never holds
(§D). The auth/authorization separation is therefore not merely policy — it is what caps the blast
radius of a server breach.

### B. Why OPAQUE (RFC 9807), not SRP-6a

Both are password-authenticated key exchanges that avoid sending the password to the server. The
decisive difference is **what a stolen server record lets an attacker do offline**.

- **SRP-6a (RustCrypto `srp`).** The server stores a **verifier** `v = g^x mod N` with
  `x = H(salt, identity, password)`. A single server-storage compromise leaks `v`, after which the
  attacker mounts an **offline dictionary attack**: for each candidate password compute `x`, then
  `g^x mod N`, and compare to `v` — a cheap modular exponentiation per guess, fully offline, with no
  further interaction with anyone. The verifier is, by construction, an **offline-guessable
  artifact**. SRP-6a also carries older-design baggage (fixed groups, no formal aPAKE
  pre-computation resistance, well-known implementation footguns).
- **OPAQUE (RFC 9807, an aPAKE with an oblivious PRF).** The server stores a per-user **envelope**
  plus OPRF / AKE key material, **not** a password verifier. The password is hardened through the
  OPRF (keyed by a server secret) *and* a key-stretching function (KSF, §F). A server-storage
  compromise **alone** yields **no artifact against which an offline dictionary attack can be run** —
  the OPRF secret is required even to *begin* guessing, and OPAQUE is designed to be
  **pre-computation resistant** so tables built before compromise do not apply. A full compromise
  (records **and** the OPRF secret) degrades only to a **KSF-bounded** offline attack — strictly
  costlier per guess than SRP's cheap verifier attack, and, for fond, still capped by the two-secret
  / vault-authorization separation (§A, §D).

**Decision.** OPAQUE (RFC 9807) is the **primary** aPAKE; **SRP-6a is retained only as a reviewed
fallback** — never the default, never hand-rolled — for an environment where a maintained OPAQUE
implementation is genuinely unavailable. The conditions under which the fallback would ever engage
are an open question (§G, I.15). Any claim that either option is "audited" or "validated" *for fond*
is downgraded to `[Validation Required]` pending A0.5 (§C).

### C. Rust crate evaluation & selection

**Recommended: `opaque-ke` (facebook/novi) as the OPAQUE implementation, with RustCrypto `srp` as the
reviewed SRP-6a fallback.** Every maintenance/audit statement below is *evidence for A0.5*, not a
validation — the *selection* is `[Validation Required]` until A0.5 signs off, and **fond never
hand-rolls a PAKE**.

| Crate | Role | Maintenance / provenance | Audit provenance | RFC conformance | Dependency footprint |
|---|---|---|---|---|---|
| `opaque-ke` | OPAQUE aPAKE (primary) | Facebook/novi; the reference-grade Rust OPAQUE, tracking the CFRG work that became RFC 9807 | Reports a **prior third-party cryptographic review** — A0.5 MUST locate it, cite it, and confirm it covers the pinned release `[Validation Required]` | Implements OPAQUE-3DH per RFC 9807; upstream test vectors published `[Validation Required]` | Elliptic-curve group (ristretto255 / P-256), its `voprf` OPRF dependency, plus KSF / `hkdf` / `hmac` / `sha2` / `rand` / `zeroize` — larger, justified by aPAKE properties |
| `voprf` | Oblivious PRF (transitive dep of `opaque-ke`) | Facebook/novi; the OPRF `opaque-ke` builds on, tracking RFC 9497 | Reviewed alongside the OPAQUE work `[Validation Required]` | RFC 9497 VOPRF `[Validation Required]` | RustCrypto curve + hash primitives |
| `srp` (RustCrypto) | SRP-6a (fallback only) | RustCrypto `PAKEs`; smaller, widely used | No dedicated aPAKE audit assumed; treat as unaudited for this purpose `[Validation Required]` | SRP-6a (RFC 2945 family), **not** an aPAKE | Minimal (`num-bigint-dig`, `digest`, `sha2`) |

Rationale for `opaque-ke`:

- **It is the audited-lineage RFC 9807 implementation in Rust**, built by the group that authored much
  of the OPAQUE / VOPRF standards work, so an RFC-conformance and review trail *exists to cite* —
  subject to A0.5 confirming both against the exact pinned version (`[Validation Required]`, §G I.10).
- **It composes cleanly with the rest of FONDENC2:** its KSF slot accepts **Argon2id**, letting us
  bind account key-stretching to the **A0.3 Argon2id profile registry** rather than introducing a
  second Argon2 regime (§F).
- **The larger dependency footprint is the price of an aPAKE** and is acceptable given the
  server-compromise properties (§B); the `srp` fallback stays available for constrained builds.

**Explicit non-goal.** Neither OPAQUE nor SRP is implemented by hand. If no maintained, reviewable
OPAQUE crate is acceptable at implementation time, the fallback is the maintained RustCrypto `srp`
crate under the §B caveats — **never** a bespoke construction.

### D. Service authentication vs. vault authorization

The core of this spike: **two distinct layers**, so that resetting the one the server can touch never
grants the powers only a vault key confers.

**Layer 1 — service authentication (resettable; the server participates).** OPAQUE login proves you
may *talk to* the account: open a session, list / upload / download **ciphertext** blobs, read the
signed manifest (ADR-021.1). This layer is **resettable** — a forgotten passphrase can be recovered
by re-registering an OPAQUE record (a password reset), and an admin / bootstrap action can restore
account *access*. The server is a full participant: it holds the OPAQUE record and enforces
per-tenant authorization, quotas, and rate limits (ADR-021 threat model). Crucially, everything this
layer authorizes operates on **opaque ciphertext** — it never decrypts content and never mutates the
vault's trust state.

**Layer 2 — vault authorization (non-resettable; the server cannot forge it).** Any **destructive or
key-material** operation MUST carry an **Ed25519 signature from a vault identity key the server never
holds and cannot forge** (ADR-020 §G / §H roster signing keys). These operations include:

- deleting or overwriting encrypted blobs (beyond append / tombstone, ADR-021.1 §F);
- publishing a new **roster** or bumping the **epoch** (revocation / rotation, ADR-020 §H);
- **enrolling or revoking** a member (roster membership change).

Because roles are **cryptographic, not server-enforced** (ADR-020 §H), the server can *store and
relay* these signed objects but can neither author nor alter them: a client rejects any roster or
destructive action not carrying a valid owner/admin Ed25519 signature chaining from known roster
state.

**The invariant that falls out of the separation.** A **login-layer reset restores account access but
can NEVER authorize vault destruction or membership change.** Concretely: a password reset replaces
the OPAQUE record (Layer 1); it does **not** — and cannot — reconstruct the member's **Secret Key**
(never uploaded, ADR-020) or their **Ed25519 vault identity key** (Layer 2). So a reset actor can log
back in and read ciphertext, but cannot unwrap the Vault Key, cannot sign a new roster, and cannot
delete or rewrite vault state. An attacker who fully compromises the *service* layer is likewise
capped: no forged vault authorization, by construction. The server's obligation to ensure a reset
**never** rewrites the bound vault identity key is the non-resettable-attribute invariant of §E
(open: §G I.13).

**Account recovery ≠ data recovery (the honest flip-side).** A server-side login reset — or an admin
/ bootstrap action — restores **account access only**. It does **not** recover vault *data*:
decryption still requires the **passphrase + Secret Key** (Emergency Kit;
[ADR-020 two-secret model](020-zero-knowledge-identity.md#the-local-keyset-two-secret-model)), and
the reset layer possesses neither. This is *precisely why* a resettable login cannot authorize vault
destruction — the layer that can be reset never holds the material (Secret Key, Ed25519 vault key)
that decryption or signing would require. A member who loses the Secret Key does **not** regain their
encrypted data by resetting the password; account recovery and data recovery are distinct events with
distinct prerequisites
([ADR-020 "Emergency Kit & recovery"](020-zero-knowledge-identity.md#emergency-kit--recovery)).

### E. Binding vault identity keys to the OPAQUE record (resolves K.10)

ADR-020 **K.10** asks how the X25519 / Ed25519 vault identity keys bind to the OPAQUE login record so
a login-layer reset cannot forge vault authorization. This spike **resolves it in principle**; the
exact construction is the only residual (§G I.12).

**Resolution.** At account registration the client commits the member's **vault identity public keys**
(Ed25519 signing, X25519 transport — ADR-020 §G) to the account, and the server stores that
commitment as an **authenticated, non-resettable attribute** kept **distinct from the resettable
OPAQUE password record**:

- **Resettable record** = the OPAQUE registration record (envelope + OPRF / AKE material). A password
  reset **replaces** this.
- **Non-resettable attribute** = the committed vault identity public key(s), bound at first
  registration and immutable except via an **Ed25519 vault-key-signed rotation** (never via a
  password reset, never by the server).

The server MUST enforce that replacing the resettable record **never** rewrites the non-resettable
attribute (§G I.13). This is what makes §D's invariant hold end-to-end: a reset restores Layer-1
access but leaves the Layer-2 identity commitment — and therefore vault authorization — untouched.

**Construction options (`[Validation Required]`, §G I.12).** Two viable bindings, to be chosen at
A0.5:

1. **Envelope-embedded.** Carry the vault identity public key inside OPAQUE's authenticated
   `client_identity` / `cleartext_credentials`, and/or encrypt a client-authored binding record under
   OPAQUE's per-user **`export_key`** so it is recoverable only by the legitimate client. The binding
   then rides the OPAQUE record itself.
2. **Sidecar self-signed attribute.** Store the vault identity public key as a separate server
   attribute accompanied by an **Ed25519 self-signature** over
   `account_id ‖ vault_identity_pubkey ‖ registration_context` under a `fond/fondenc2/v2/...`
   domain-separation label. The signature (which the server cannot produce) authenticates the
   attribute; the server treats it as append-only.

Option 2 keeps the non-resettable attribute cleanly separable from the resettable OPAQUE record (a
natural fit for §D's reset invariant); option 1 couples them more tightly but leans on OPAQUE's own
authenticated fields. The message canonicalization and label for both the binding self-signature and
the per-operation authorization signature (§D) are `[Validation Required]` (§G I.14).

### F. Chosen ciphersuite, parameters & A0.5 sign-off

All figures below are **placeholders**, every one `[Validation Required]` at A0.5 (§G I.11); they
exist to anchor the review, not to decide it.

**OPAQUE ciphersuite (RFC 9807 / `opaque-ke` configuration):**

| Component | Placeholder | Note |
|---|---|---|
| AKE | OPAQUE-3DH | the RFC 9807 authenticated key exchange `[Validation Required]` |
| OPRF group | ristretto255 (alt. P-256) | RFC 9497 VOPRF; group choice `[Validation Required]` |
| Hash | SHA-512 | paired with ristretto255 per `opaque-ke` defaults `[Validation Required]` |
| KDF / MAC | HKDF-SHA-512 / HMAC-SHA-512 | `[Validation Required]` |
| **KSF (key-stretching)** | **Argon2id via the A0.3 profile registry** | bind to `PROFILE[kdf_profile_id]` so the vault keeps **one** Argon2 regime, not two `[Validation Required]` (§G I.11) |
| Nonce / seed lengths | per RFC 9807 | `[Validation Required]` |

**KSF binding (important).** The OPAQUE key-stretching function reuses the **A0.3 Argon2id profile
registry**
([ADR-020 A0.3 appendix](020-zero-knowledge-identity.md#appendix-kdf-profiles-rotation--migration-a03))
rather than defining independent Argon2 parameters, so account login and MUK derivation share one
pinned, budgeted, append-only parameter table. Whether the account KSF profile and the member's MUK
profile are the *same* `kdf_profile_id` or two registry entries is `[Validation Required]`.

**Passphrase used twice — domain-separated (`[Validation Required]`, §G I.16).** The *same* user
passphrase feeds **two** derivations: the **OPAQUE login** (service auth) and the **MUK** Argon2id
(vault unwrap; [ADR-020 §C](020-zero-knowledge-identity.md#c-domain-separation--the-two-secret-muk) /
the A0.3 appendix). These MUST be **domain-separated** so the server-side OPAQUE record can never
assist an attack on the MUK, and MUK material can never assist forging an OPAQUE login. Reusing the
A0.3 Argon2id **profile registry** as OPAQUE's KSF shares only the *tuned parameter table*, **not**
the derived value: the two stretches are **separate invocations over distinct inputs and salts** —
OPAQUE's KSF runs over the **OPRF-transformed** password (keyed by the server's OPRF secret, and never
mixing in the Secret Key), while the MUK runs over the **raw passphrase + Secret-Key pepper**
(ADR-020 §C) under its own per-member salt and `fond/fondenc2/v2/...` label — so their outputs are
independent by construction. Whether that independence is pinned via **distinct domain-separation
labels** or rests on the **structural** OPRF-secret / Secret-Key-pepper / distinct-salt argument is
`[Validation Required]` (§G I.16).

**Threat-notes recap — what a FULL server compromise does and does not yield (for A0.5, from §A;
every claim `[Validation Required]`).**

- **(a) OPAQUE login records → no offline password guessing.** A dump of the OPAQUE records yields no
  offline-guessable verifier (§B); pre-computation is resisted by the OPRF secret, and online guessing
  is bounded by server rate-limits, not cryptography.
- **(b) Exfiltrated `wrapped_vault_key` blobs + OPAQUE records → still no vault decryption.** The
  **Secret Key is never on the server** (ADR-020 two-secret model), so even holding every wrap blob
  and OPAQUE record the attacker cannot derive the **MUK** or unwrap the **Vault Key**; and without an
  Ed25519 vault key it cannot forge vault authorization (§D). The blast radius of a breach is capped
  by material the server never possesses.
- **(c) But metadata still leaks — do not overclaim.** Blob counts, sizes, change timestamps, and
  device ids remain visible to the server, and a revoked member can still enumerate opaque
  `object_id`s (cross-ref [ADR-021.1 §G](#g-honest-limits--what-this-cannot-do) and the ADR-021
  Threat model's documented residuals). Zero-knowledge covers **content**, not traffic analysis.

**A0.5 sign-off checklist.** The independent reviewer must confirm: (1) the OPAQUE crate + pinned
version and its cited audit scope (I.10); (2) the full ciphersuite and the KSF↔A0.3-registry binding
(I.11); (3) the identity↔OPAQUE binding construction resolving K.10 (I.12); (4) the
non-resettable-attribute server invariant and reset flow (I.13); (5) the per-operation Ed25519
vault-authorization signature format (I.14); (6) the SRP-6a fallback conditions (I.15); (7) the
passphrase two-use domain separation between the OPAQUE KSF and the MUK (I.16). **No auth/sync code
lands until all seven clear.**

### G. New open questions & validation

- **New open questions.** A0.4 appends seven items to the ADR-021.1
  [§I](#i-open-questions-for-a05-independent-review) list (extending, never renumbering): **I.10**
  OPAQUE crate & version pin; **I.11** ciphersuite & KSF↔A0.3-registry binding; **I.12**
  identity↔OPAQUE binding construction (resolves ADR-020 **K.10**); **I.13** non-resettable-attribute
  server invariant; **I.14** vault-authorization signature format; **I.15** SRP-6a fallback
  conditions; **I.16** passphrase two-use domain separation (OPAQUE KSF vs. MUK). ADR-020 **K.10** is
  **advanced / resolved-in-principle** here (§E) and carries a one-line pointer back; its residual
  construction is I.12.
- **Not a re-spec of the vault.** This appendix decides the *account-authentication* primitive and the
  auth-vs-authorization boundary only; the key hierarchy, envelope, roster, and causal history remain
  authoritative in ADR-020 and the ADR-021.1 appendix. The whole stays a **composition of reviewed
  primitives** (OPAQUE / RFC 9807, VOPRF / RFC 9497, Argon2id, Ed25519, X25519) with **nothing
  hand-rolled** — which is exactly why the A0.5 independent review is mandatory before any
  implementation.
