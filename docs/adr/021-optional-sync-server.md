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
| SRP-6a for accounts | A stolen verifier enables offline guessing; OPAQUE (RFC 9807) is chosen. **No SRP fallback** (A0.5 I.15) — a negotiated fallback is a downgrade path. See [Appendix ADR-021.2 §B](#b-why-opaque-rfc-9807-not-srp-6a). |
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

### A0.5 remediation-mapping table (this revision)

The A0.5 adversarial review ([`docs/reviews/a05-fondenc2-adversarial-review.md`](../reviews/a05-fondenc2-adversarial-review.md))
returned a **NO-GO** and identified structural blockers in the history topology. This revision
applies its recommendations. Coverage for items landing in **this appendix**:

| Finding | Handled in | New status |
|---|---|---|
| N-03 / N-12 / I.3 honest-concurrency topology | [§D DAG, CAS, head pointer](#d-authenticated-history-topology-dag-cas--signed-head) | Resolved (A0.5) |
| N-04 historical roster binding | [§D](#d-authenticated-history-topology-dag-cas--signed-head), [§E](#e-rollback--fork--equivocation-detection) | Resolved (A0.5) |
| N-05 own-write anti-rollback | [§E op-log commitment](#e-rollback--fork--equivocation-detection) | Resolved (A0.5) |
| N-07 bootstrap freshness anchor | [§D checkpoints](#d-authenticated-history-topology-dag-cas--signed-head) | Resolved (A0.5) |
| N-13 / I.1 object-id collision strength & width | [§B](#b-opaque-keyed-object-identifiers) | Resolved (A0.5): 32-byte typed id |
| I.2 object-id key rotation-invariance | [§B](#b-opaque-keyed-object-identifiers), [§G](#g-honest-limits--what-this-cannot-do) | Resolved (A0.5): vault-lifetime key |
| I.4 device_id & per-device keys | [§C](#c-per-device-version-vectors) | Decided per review: per-device signing keys |
| I.5 manifest hash function | [§D](#d-authenticated-history-topology-dag-cas--signed-head) | Decided per review: SHA-256 canonical |
| I.6 checkpoint cadence/authority | [§D](#d-authenticated-history-topology-dag-cas--signed-head) | Decided per review |
| I.7 trusted-state home | [§E](#e-rollback--fork--equivocation-detection) | Decided per review: MAC'd file + re-trust |
| I.8 equivocation response | [§E](#e-rollback--fork--equivocation-detection) | Decided per review: hard-stop + quarantine |
| I.9 tombstone reaping | [§F](#f-tombstones--sibling-retaining-merge) | Decided per review: signed dominance |

> **New-since-A0.5 design choices** (the DAG topology, signed frontier/head-counter, and the
> per-device op-log below are among them) are consolidated for the re-reviewer in ADR-020's
> [New design decisions since A0.5](020-zero-knowledge-identity.md#new-design-decisions-since-a05-not-in-the-original-review--scrutinize-these).

**Keys are reused from FONDENC2, never re-invented.** The manifest signer, the manifest object's
encryption key, and the object-id namespace key all derive from the ADR-020 hierarchy:

| This appendix uses | Derived from (ADR-020) |
|---|---|
| object-id namespace key `NS_objectid` (§B) | a random **vault-lifetime** namespace key, distinct from every `VK_e` ([§F vault-lifetime keys](020-zero-knowledge-identity.md#f-per-object-dek-derivation--object-granularity), I.2) |
| manifest confidentiality (§D) | the `manifest` purpose subkey (§F), sealed as a FONDENC2 object `object_class = manifest` ([§E envelope](020-zero-knowledge-identity.md#e-envelope--wire-format-per-object)) |
| manifest authorship (§D–§E) | the member's per-**device** Ed25519 signing key, certified by the member identity key ([§G roster / K.7](020-zero-knowledge-identity.md#g-per-member-key-wrapping--the-roster)) |

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

Instead the id is a keyed pseudonym under a household-secret, **vault-lifetime** namespace key:

```text
object_id      = HMAC-SHA-256(NS_objectid, canonical_input)      # full 32-byte output
NS_objectid    = random vault-lifetime namespace key             # ADR-020 §F; NOT any VK_e (I.2)
canonical_input = len_prefix("fond/fondenc2/v2/objid")           # format label
               ‖ u8(object_class) ‖ durable_uuid(16) ‖ u16_le(sub_part)   # durable_uuid: see §H
```

- **Why keyed, not a bare hash.** Only Vault-Key holders can compute the id, so the server cannot
  correlate an id to any guessed plaintext (defeats known-file matching) and cannot mint ids.
- **Deterministic across devices.** Every member derives the *same* `object_id` for the same object
  because `NS_objectid` and `durable_uuid` are shared — this is what lets independent devices
  converge without a server-assigned key.
- **Width — decided (A0.5, N-13 / I.1 / K.11): the full 32-byte HMAC-SHA-256 output.** A 16-byte
  truncation gives 128-bit preimage/forgery strength but only **64-bit generic collision** strength —
  the earlier "≥128-bit collision resistance" claim was wrong. The full 32 bytes restore 128-bit
  collision strength, so no collision-detection/recovery machinery is needed. The FONDENC2 envelope
  `object_id` field is 32 bytes to match ([ADR-020 §E](020-zero-knowledge-identity.md#e-envelope--wire-format-per-object)).
- **Canonical input — decided (A0.5, I.1): a fixed typed, length-prefixed transcript** — a format
  label, then `object_class` (`u8`), the durable UUID (16 bytes), and a fixed-width `sub_part`
  (`u16_le`, `0` for whole-object). No delimiter-free variable fields.
- **Namespace key — decided (A0.5, I.2): a random vault-lifetime key `NS_objectid`,** generated once
  at vault creation and **distinct from every per-epoch `VK_e`**. It is distributed through the
  authenticated roster directory ([ADR-020 §G](020-zero-knowledge-identity.md#g-per-member-key-wrapping--the-roster))
  and is **epoch-invariant** — so ids survive rotation while DEKs stay epoch-scoped. Rooting it in
  the per-epoch Vault Key (the earlier sketch) would orphan every object's identity on rotation; a
  vault-lifetime key resolves that. The honest cost — a revoked member who learned `NS_objectid` can
  keep enumerating ids (metadata, never content) — is explicitly accepted (§G).

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

- A **new device** joins with an all-zero implicit vector and appears in the vault **roster
  directory** (ADR-020 §G) — with its own **certified per-device signing key** — before its writes
  are accepted; its first write introduces its `device_id` component.
- A **departed / revoked** device stops writing and has its device certificate removed from the
  roster, but its **past components remain** in existing vectors as immutable history — they must, or
  prior causality breaks. Epoch rotation (ADR-020 §H) revokes its *key*, not its *history*.

**`device_id` & per-device keys — decided (A0.5, I.4 / K.7).** `device_id` is a **random** 16-byte
id, and each device carries its **own random Ed25519 signing key**, **certified** by the member
identity key ([ADR-020 §G](020-zero-knowledge-identity.md#g-per-member-key-wrapping--the-roster)).
A per-member-only signing key cannot support `device_id` ownership or per-device revocation; the
per-device key does. Version-vector components, manifest signatures, counters, and revocation all
bind to the device certificate, and every historical manifest record identifies the roster state
that authorized that device (§D, §E).

**History topology — decided (A0.5, I.3 / N-03 / N-12): an authenticated multi-parent DAG**, not a
single linear chain. A linear `prev` chain assumes one writer and would mislabel two honest devices
extending the same head as equivocation. Manifest records therefore carry **`parents[]`** and honest
concurrent heads are **siblings**, converged by **signed merge records** (§D). Per-object version
vectors continue to detect per-object concurrency *within* that DAG. (Finer-grained per-object
sub-chains remain a possible future optimization but are not required for correctness now.)

### D. Authenticated history topology: DAG, CAS & signed head

The **manifest** is the authenticated index of the library: it names every live object, its version
vector, and a binding to its ciphertext. It is an **append-only, content-addressed, multi-parent
DAG** (not a linear chain, A0.5 N-03/I.3) so **honest concurrent writers converge** instead of being
mislabelled as equivocation, and so the server can neither **rewrite a client's witnessed history**
nor **forge records** undetected. (It **cannot** force the server to *serve* the latest state —
withholding/partition remains possible and is stated honestly in §E/§G.)

```text
┌─ manifest record ─────────────────────────────────────────────────────────────────────┐
│ CLEARTEXT CAS header (server-visible; needed only for addressing & append):            │
│   record_id     32 bytes  = SHA-256(sealed envelope bytes below); external content addr │
│   parents_ct[]  list<32B>  copy of parents[] so the server can CAS; re-checked on decrypt│
│                                                                                         │
│ SEALED BODY (FONDENC2 object, object_class = manifest; envelope object_id = HMAC over a  │
│ fresh random 16-byte record-UUID, §B) — the server sees only ciphertext:                │
│   parents[]        list      0 at genesis; 1 for a normal extension; ≥2 for a MERGE      │
│   vault_epoch      u32       FONDENC2 epoch in force (ADR-020 §F)                        │
│   roster_hash      32 bytes  roster directory that authorized the signer (HISTORICAL, N-04)│
│   author_device    16 bytes  device_id of the signer                                    │
│   device_cert      bytes     member-signed certificate for author_device (§C)           │
│   record_kind      u8        0 = normal, 1 = checkpoint, 2 = merge                       │
│   entries[]        list      one per affected/live object:                              │
│      ├ object_id     32 bytes  opaque id (§B)                                            │
│      ├ version_vector map      { device_id → counter } (§C)                              │
│      ├ blob_hash     32 bytes  SHA-256 of the object's ENTIRE FONDENC2 envelope (I.5)    │
│      └ tombstone?    opt       present iff the object is deleted (§F)                    │
│   ed25519_sig  64 bytes  over the domain-separated canonical body (parents ‖ epoch ‖     │
│                          roster_hash ‖ author ‖ cert ‖ kind ‖ canonical(entries));       │
│                          key = the author's per-DEVICE Ed25519 key (ADR-020 §G / K.7)   │
└─────────────────────────────────────────────────────────────────────────────────────────┘
```

- **`record_id` is external, never inside the sealed body.** It is the SHA-256 of the sealed
  envelope bytes (a content address), so it cannot appear inside the bytes it hashes. `parents[]` live
  **inside** the signed body; a **cleartext copy** rides in the CAS header **only** so the server can
  address and append records it cannot decrypt. On decrypt a client **re-checks** `parents_ct[]`
  against the signed `parents[]` and rejects any mismatch — a lying server only corrupts its own CAS
  graph, never a client's verified view.
- **Multi-parent DAG (N-03/N-12).** A record's `parents[]` are the `record_id`s it causally extends.
  Two honest devices that both read head `H` and each append a new record with `parents = [H]`
  produce **two sibling heads**, not equivocation. A later **merge record** (`record_kind = 2`) with
  `parents = [H_a, H_b]` — signed by a device — reunites them; its `entries[]` carry the merged
  per-object version vectors (§C, §F). The DAG therefore accepts concurrency and converges it, which
  a single linear chain could not.
- **Content-addressed, immutable records + CAS (N-12).** The server stores records **keyed by
  `record_id`** and offers only **compare-and-append**: a record is accepted iff every id in
  `parents_ct[]` already exists and `record_id` is new. Because ids are content addresses, the server
  can neither mutate a stored record (its id would change) nor invent one (it lacks a device signing
  key). This is the external immutable record naming the linear chain lacked.
- **Signed head / frontier pointer (corroboration, with a monotonic counter).** The current tip is a
  **frontier** — the set of head `record_id`s with no children. A device publishes a **signed head
  advertisement** `head_adv = Sign_device(len_prefix("fond/fondenc2/v2/head") ‖ vault_id ‖
  roster_hash ‖ sorted(frontier_ids) ‖ head_counter)`, where **`head_counter` is that signer's own
  strictly-monotonic counter**. Each client persists the **highest `head_counter` seen per signer**
  in `trusted_state` (§E) and **rejects a lower one** as a replay. A head advertisement is
  **corroboration / bootstrap freshness**, not the sole anti-rollback mechanism — the primary
  guarantees are the own-op-log and frontier checks (§E); an advertisement whose counter regresses,
  or whose frontier fails to descend from trusted state, is rejected.
- **What the signature covers.** The Ed25519 body signature covers `parents[]`, `roster_hash`, and
  every entry's `version_vector` and `blob_hash` — so a server can neither reposition a record,
  re-parent it, nor **forge, rewind, or reorder causal state** without breaking the tag.
- **Historical roster binding (N-04).** Each record carries the `roster_hash` **that authorized its
  signer at signing time** plus the `device_cert`. Verification replays *that* historical roster
  (§E check 1) — so history authored by a **later-revoked** member/device stays valid, and a signer
  who was **not** authorized in that roster is rejected. Checking against the *current* roster alone
  was both too strict (rejects valid old history) and too weak (doesn't prove authorization then).
  Post-revocation abuse of an old roster is bounded by the **causal cut** (§E check 6, ADR-020
  transition object): an old-roster record must be an **ancestor of** the epoch transition, else it is
  handled by author (revoked → rejected; still-current → quarantined for re-authoring under `e+1`).
- **Hash function — decided (A0.5, I.5): SHA-256** over a **domain-separated canonical full-record**
  serialization (the HMAC/HKDF family already in FONDENC2), and `blob_hash` covers the **entire**
  FONDENC2 envelope (header + ciphertext + tag), not just the body.
- **Confidentiality.** The body is sealed as a FONDENC2 object (`object_class = manifest`,
  [§E envelope](020-zero-knowledge-identity.md#e-envelope--wire-format-per-object)) under the
  `manifest` purpose subkey, so the server stores only ciphertext plus the cleartext CAS header it
  needs to index (`record_id` and `parents_ct[]`). The hashes, vectors, `roster_hash`, and author all
  live **inside** the encrypted body.
- **Checkpoints — decided (A0.5, I.6 / N-07).** A checkpoint (`record_kind = 1`) is a record whose
  `entries` are the **complete** live-object set (not a delta) and whose `parents[]` name **all**
  frontier heads it summarizes (so it dominates them). It binds the historical `roster_hash`, and is
  accepted only from an **owner/admin** device (authority) **plus** corroboration that it descends
  from state the accepting device already trusts. A checkpoint lets a new device bootstrap without
  replaying the whole DAG, bounds verification depth, and enables **tombstone reaping** (§F).
- **Bootstrap freshness anchor (N-07).** A **new device must not accept any checkpoint as genesis on
  the server's word** — the server could replay an old self-consistent checkpoint. The device's
  trusted starting head/checkpoint commitment is carried by an **authenticated channel**: the
  enrolling device's signed head advertisement or a roster transition
  ([ADR-020 §H](020-zero-knowledge-identity.md#h-enrollment-roles-invitation-revocation-epoch-rotation)),
  never the bare server response. This closes the first-sync rollback window.
- **Dovetails the roster chain.** This is the manifest analogue of ADR-020 §G's `prev_roster_hash`
  roster directory chain and the §H transition object; together they make membership *and* content
  history rollback-evident.

### E. Rollback / fork / equivocation detection

Detection rests on **trusted local device state** — a small, durable, per-device watermark the server
never sees and cannot influence:

```text
trusted_state = { trusted_frontier, own_oplog, seen_head_counters, last_transition, mac }
  trusted_frontier   the set of manifest record_ids this device has accepted as heads
  own_oplog          this device's durable append-only operation log (entries, not just a head)
  seen_head_counters map: signer_device_id -> highest head_counter seen (§D replay guard)
  last_transition    record_id + new_epoch of the latest accepted epoch transition (ADR-020 §H)
  mac                HMAC over the whole state, keyed by a device-local key (I.7)
```

- **Own operation log (replaces the scalar `own_counter`, A0.5 N-05).** A single `own_counter`
  cannot validate per-object writes: a global counter wrongly rejects a valid older object once a
  newer write advances the device, and a per-object counter needs a map, not a scalar. Instead each
  device keeps a **durable append-only operation log** (`own_oplog`, whose **entries are stored**, not
  just a head hash): every local mutation appends `entry = { object_id, own_component_after,
  prev_oplog_hash }`, hash-chained. The log is a **commitment to all of this device's own per-object
  writes**, so a served history that **omits or lowers** any logged write is rejected (check 4).
- **Bounded, checkpoint-rooted compaction.** The log is **never** pruned in the interior (that would
  break the hash chain). Instead it is **re-based at a trusted checkpoint** (§D): when a checkpoint
  the device trusts captures every object at `version_vector[own_device] ≥ own_component_after` (or a
  tombstone for it), the device **truncates the whole prefix** and starts a **fresh chain rooted at
  the checkpoint's `record_id`**, retaining only entries **after** the checkpoint. A **mandatory
  checkpoint cadence** — at least every `N` records or `T` interval (exact `N`/`T` are
  deployment-tunable) — bounds the log so check 4 never replays unbounded history; if checkpoints are
  overdue the client warns rather than growing without limit. A server that **withholds** checkpoints
  (only an owner/admin device can author one, §D) can still prevent compaction — this is one more face
  of the unpreventable **withholding** limit (§G), surfaced as an overdue-checkpoint warning, not a
  silent unbounded growth.

**Requirement (not optional).** The watermark **MUST** persist in **durable, tamper-resistant local
storage outside the disposable `fond.db`**. `fond.db` is never synced and is rebuilt from files by
`reindex` (ADR-002/012); if the watermark lived there, a `reindex` or reset would **silently reset
the rollback baseline**. **Decided (A0.5, I.7): a dedicated local state file outside `fond.db`,
atomically updated and MAC'd with a device-local key** (the OS keychain does not generally provide
monotonic anti-rollback storage either). Backup/restore or device re-provisioning resets this
baseline, so an explicit **re-trust flow** (re-anchor to a corroborated head, §D bootstrap anchor)
is required and documented; the reset is a deliberate, user-visible event, never silent.

On every pull the client runs these checks **before** applying anything, and **fails closed**:

1. **Signature & historical roster (N-04).** Every record must carry a valid Ed25519 signature from
   the **device key certified in the roster identified by that record's `roster_hash`** — the
   historical authorization state, not the current roster. Reject a signer not authorized in *that*
   roster, or a `roster_hash` that does not resolve to a validly-signed roster directory chaining
   from trusted state. This blocks a server forging history **and** admits valid history by
   later-revoked members.
2. **DAG continuity.** Every `record_id` must be the SHA-256 of its sealed bytes, `parents_ct[]` must
   match the signed `parents[]` on decrypt, and every parent id must resolve to a **known record in
   the locally trusted DAG** — i.e. `trusted_frontier`, any of its **ancestors**, or a corroborated
   checkpoint (§D). (Resolving to *ancestors*, not only the current frontier heads, is what lets an
   honest sibling that branches from an earlier trusted point attach.) A dangling or non-resolving
   parent is **tampering**.
3. **No rollback (DAG-correct — must not reject honest concurrency).** The rule is **dominance, not
   linear descent**: **every id in `trusted_frontier` must still be present in the served frontier, or
   be an ancestor of (dominated by) some served head.** Additional served heads that validly extend
   shared ancestry are **honest concurrency and are allowed** (they become siblings, merged per §F). A
   served state is a **rollback** only if it **drops a trusted head that no served head dominates**, or
   presents a head that does not descend from the trusted ancestry at all. (This deliberately does
   **not** require every served head to descend from a trusted head — that would reject the very
   sibling the DAG exists to accept.)
4. **Own-write completeness (N-05).** Every un-pruned entry in `own_oplog` must appear, with
   `version_vector[own_device] ≥ own_component_after`, in the served history for its `object_id`. A
   history that omits or lowers any of this device's own logged writes is the server **withholding /
   rewinding** the device's own history → reject.
5. **No double-signing / stale advertisement.** Detectable equivocation is precisely: **two
   validly-signed records by the same `author_device` with the same `parents[]` set but different
   content** (a device provably signed two conflicting extensions of one point), or a **signed head
   advertisement whose `head_counter` regresses** below `seen_head_counters[signer]` (§D). Honest
   concurrent siblings — different authors, or the same author extending *different* heads — are
   **not** equivocation; they are merged (§D, §F).
6. **Epoch causal cut (N-04 abuse bound).** A record citing roster epoch `e` is normally valid only
   as an **ancestor of the committed `e → e+1` transition** (ADR-020 §H transition object;
   `last_transition` in trusted state). An epoch-`e` record that is **not** an ancestor of that
   transition is handled by author:
   - authored by a **revoked** device/member (absent from the `e+1` roster) → **rejected** as a
     post-cut write under a dead roster (this is the abuse the cut exists to stop; such a member's
     un-merged offline writes are lost, stated honestly);
   - authored by a **still-current** member (present in the `e+1` roster) → **not** discarded:
     **quarantined and surfaced for re-authoring under `e+1`**, so a legitimate write made offline
     *before* the rotation but uploaded *after* is preserved, not silently dropped.

**Rollback vs. withholding — detection is not symmetric.** Checks 3–4 catch **rollback** *immediately
and locally*: a rewind **below what this device has already witnessed** contradicts the trusted
frontier and the own-op-log, with no peer required. What the local watermark **cannot** catch is a
**partitioning / split-view server** that shows *this* device one honest-looking branch while
**withholding** another device's concurrent branch. This is **not** an internal inconsistency the DAG
can reject — in the DAG the two branches are legitimate concurrent siblings that would simply **merge**
if ever served together (a merge record *can* descend from both, so "they cannot merge" is **not** a
detector). The honest guarantee is therefore: **your own witnessed history cannot be rewound
undetectably (rollback), but a server can still withhold/partition branches you have never seen.** That
residual is **withholding**, and it is only surfaced — never prevented — when devices actually
reconcile out-of-band (a fresher signed head advertisement with a higher `head_counter`, a shared
checkpoint, or a direct device-to-device compare). A server that permanently partitions two devices
that never reconcile keeps them siloed; §G states this limit plainly.

**Provable equivocation vs. undetectable withholding.** The one *cryptographically provable* server
misbehaviour is a device **double-signing** (check 5): two conflicting records under one signer/parent
set are non-repudiable evidence. Everything else the malicious server can do reduces to **withholding**
— loud (your own writes stop advancing, or a peer reports a higher counter) but not preventable.

**Response to a detected fork/equivocation — decided (A0.5, I.8): hard-stop + quarantine.**
Automatic sync **halts** on detection; the client preserves **both** branches and the signed
evidence, alerts the user, and presents a **quarantine/recovery workflow**. "Warn and continue" is
rejected — it would normalize a detected integrity failure. Ordinary honest concurrency is **not** a
fork and does not trigger this: it is merged (§D, §F) without halting.

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

- **Lifecycle / reaping — decided (A0.5, I.9): signed dominance by every current-roster device.** A
  tombstone is retained until **every device in the current roster** has **signed acknowledgement /
  causal dominance** of `deleted_vv` (its vector happens-after the delete) **and** a **checkpoint**
  (§D) commits the deletion — only then may it be dropped without a lagging device resurrecting the
  object. "Recently-active devices only" is **rejected** as unsafe; an offline device **pins** the
  tombstone until it is explicitly removed from the roster by a **signed roster transition**
  ([ADR-020 §H](020-zero-knowledge-identity.md#h-enrollment-roles-invitation-revocation-epoch-rotation)),
  which is what bounds indefinite pinning without silently dropping data.
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
- **Cross-device split-view / partition is unpreventable.** A malicious server can show two devices
  different concurrent branches and **withhold** each from the other. In the DAG these are legitimate
  siblings that would merge if ever served together, so this is **not** an internal inconsistency any
  check can reject — it reduces to **withholding** (§E, "Rollback vs. withholding"): only surfaced,
  never prevented, when devices reconcile out-of-band (a higher signed `head_counter`, a shared
  checkpoint, or a direct device-to-device compare). The only *cryptographically provable* server
  misbehaviour is a device **double-signing** conflicting records under one parent set. Corroboration
  shrinks the window but does not close it.
- **A revoked member can still enumerate identifiers.** Because the `NS_objectid` namespace key is
  **epoch-invariant** (§B, decided per I.2), a revoked member who learned it can keep **recomputing
  and enumerating `object_id`s** after revocation — correlating which objects exist and when they
  change (**metadata**, never content; epoch rotation re-keys DEKs but not identifiers). This
  post-revocation metadata capability is the honest, **explicitly accepted** cost of stable
  cross-device identity (I.2). Should a household ever need to sever it, only a **coordinated re-id
  pass** (re-deriving `NS_objectid` and re-identifying every object) can — an expensive, optional
  operation, not part of ordinary rotation.
- **Roster metadata is server-visible.** Because the membership/wrap directory is authenticated but
  **not** confidential ([ADR-020 §G](020-zero-knowledge-identity.md#g-per-member-key-wrapping--the-roster),
  the N-02 cycle break), the server sees member **count, roles, and public keys**. This is an
  accepted metadata leak; content and the Vault Key stay confidential.
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

The A0.5 review adjudicated each item. This revision applies its recommendations: items are
**Resolved** (structural fix landed) or **Decided per review** (one option pinned), except the
OPAQUE crate-audit items, which **remain deferred** to a human reviewer. Section pointers are to the
(revised) sections.

| # | Item | New status | Where |
|---|---|---|---|
| I.1 | `object_id` input & width | Decided: 32-byte typed length-prefixed transcript | §B |
| I.2 | Object-id key rotation-invariance | Resolved: random vault-lifetime `NS_objectid`; metadata capability accepted | §B, §G |
| I.3 | Manifest / VV granularity | Resolved: authenticated multi-parent DAG | §C, §D |
| I.4 | `device_id` & per-device keys | Decided: random `device_id` + certified per-device signing keys | §C |
| I.5 | Manifest hash function | Decided: SHA-256, canonical full-record, whole-envelope `blob_hash` | §D |
| I.6 | Checkpoint cadence & authority | Decided: owner/admin, all-parent-heads, corroborated | §D |
| I.7 | Trusted-state home | Decided: MAC'd file outside `fond.db` + re-trust flow | §E |
| I.8 | Equivocation response | Decided: hard-stop + quarantine | §E |
| I.9 | Tombstone reap predicate | Decided: signed dominance by every current-roster device | §F |
| I.10 | OPAQUE crate & version pin | **Still deferred (human audit review)** | [ADR-021.2 §C](#c-rust-crate-evaluation--selection) |
| I.11 | OPAQUE ciphersuite & KSF binding | Decided (suite); crate features/adapter **deferred** | [ADR-021.2 §F](#f-chosen-ciphersuite-parameters--a05-sign-off) |
| I.12 | Identity ↔ OPAQUE binding | Resolved: client-anchored self-signed sidecar | [ADR-021.2 §E](#e-binding-vault-identity-keys-to-the-account-client-anchored-resolves-k10) |
| I.13 | Non-resettable-attribute invariant | Resolved: client-anchored; server enforcement is defense-in-depth | [ADR-021.2 §E](#e-binding-vault-identity-keys-to-the-account-client-anchored-resolves-k10) |
| I.14 | Vault-authorization signature format | Resolved: full pinned transcript + replay storage | [ADR-021.2 §D](#d-service-authentication-vs-vault-authorization) |
| I.15 | SRP-6a fallback conditions | Resolved: no SRP fallback | [ADR-021.2 §B](#b-why-opaque-rfc-9807-not-srp-6a) |
| I.16 | Passphrase two-use domain separation | Decided: length-prefixed labels + distinct profile ids | [ADR-021.2 §F](#f-chosen-ciphersuite-parameters--a05-sign-off) |

**Still open (deferred to the human cryptographer).** **I.10** (and the crate-features half of
**I.11**) — pinning the exact `opaque-ke` release/features and confirming the intervening changes
since the 2021 v0.5.0 audit — cannot be closed by a model review; it stays `[Validation Required]`
(ADR-021.2 §C, §F). Every other item above is decided or resolved per the review's recommendation.

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
[§G](020-zero-knowledge-identity.md#g-per-member-key-wrapping--the-roster) /
[§H](020-zero-knowledge-identity.md#h-enrollment-roles-invitation-revocation-epoch-rotation), and the
signed causal history in the
[ADR-021.1 appendix](#appendix-authenticated-causal-history-adr-0211) above. It reuses their exact
vocabulary (MUK, Vault Key, member KEK, epoch, roster, `wrapped_key_package`, Ed25519 roster/manifest
signatures, X25519, `fond/fondenc2/v2/...` labels) and **never hand-rolls a PAKE**.

**Gate reminder:** this is a paper spec. **No auth/sync code lands before the Epic A0 independent
review (A0.5) clears.** After the A0.5 remediation, items I.10–I.16 in the ADR-021.1
[§I](#i-open-questions-for-a05-independent-review) list carry an explicit status (resolved / decided
/ still-deferred); remaining `[Validation Required]` tags mark the choices a human reviewer must
still sign off on. Per the honesty rule of this spike, **no crate is "audited/validated" *for fond*
until A0.5 signs off** — a *prior* third-party review of a dependency is cited as evidence, never as
validation of the selection.

### A0.5 remediation-mapping (ADR-021.2)

Coverage for the review items landing in this appendix:

| Finding | Handled in | New status |
|---|---|---|
| I.15 / VR-020-D.1 / VR-0212-C.0 drop SRP | [§B](#b-why-opaque-rfc-9807-not-srp-6a), [§C](#c-rust-crate-evaluation--selection) | Resolved (A0.5): no fallback |
| VR-0212-C.1 / C.3 audit-provenance overclaim | [§C](#c-rust-crate-evaluation--selection) | Resolved (A0.5) |
| VR-0212-F.8 record-dump claim | [§F](#f-chosen-ciphersuite-parameters--a05-sign-off) | Resolved (A0.5): excludes OPRF seed |
| I.12 / I.13 / N-09 client-anchored identity | [§E](#e-binding-vault-identity-keys-to-the-account-client-anchored-resolves-k10) | Resolved (A0.5) |
| I.14 vault-authorization transcript | [§D](#d-service-authentication-vs-vault-authorization) | Resolved (A0.5) |
| VR-0212-F.1–F.6 ciphersuite | [§F](#f-chosen-ciphersuite-parameters--a05-sign-off) | Decided: OPAQUE-3DH ristretto255/SHA-512 |
| VR-0212-F.5 / F.7 KSF domain & profile ids | [§F](#f-chosen-ciphersuite-parameters--a05-sign-off) | Decided: distinct domain + profile ids |
| I.16 passphrase two-use separation | [§F](#f-chosen-ciphersuite-parameters--a05-sign-off) | Decided: length-prefixed labels |
| I.10 / I.11 crate audit & features | [§C](#c-rust-crate-evaluation--selection), [§F](#f-chosen-ciphersuite-parameters--a05-sign-off) | Still deferred (human audit) |

### Scope & pointer map

| This appendix subsection | Concretizes | Acceptance criterion (#119) |
|---|---|---|
| A. Threat model for account authentication | ADR-021 [Threat model](#threat-model); ADR-020 [Threat model](020-zero-knowledge-identity.md#threat-model) | #4 |
| B. Why OPAQUE (RFC 9807), not SRP-6a | ADR-021 Alternatives `SRP-6a for accounts`; ADR-020 Alternatives `SRP-6a as the primary PAKE` | #2 |
| C. Rust crate evaluation & selection | ADR-021 Decision (per-user OPAQUE accounts); ADR-020 [§J primitives](020-zero-knowledge-identity.md#j-primitives) | #1 |
| D. Service authentication vs. vault authorization | ADR-020 [§G](020-zero-knowledge-identity.md#g-per-member-key-wrapping--the-roster) / [§H](020-zero-knowledge-identity.md#h-enrollment-roles-invitation-revocation-epoch-rotation) roster & Ed25519 | #3 |
| E. Binding vault identity keys to the account (client-anchored) | ADR-020 [K.10](020-zero-knowledge-identity.md#k-open-questions--a05-remediation-status) | #3 |
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

**Decision — decided (A0.5, I.15 / VR-020-D.1 / VR-0212-C.0): OPAQUE (RFC 9807) is the sole account
aPAKE; there is no SRP-6a fallback.** A negotiated or availability-triggered fallback is a
**downgrade path** — an attacker who can suppress OPAQUE forces the weaker verifier-based protocol —
and the candidate `srp` crate has **never received an independent third-party aPAKE audit** and
permits offline guessing after verifier theft. If a reviewed OPAQUE implementation is genuinely
unavailable at implementation time, **the sync/account feature stays disabled** rather than
downgrading authentication. The SRP-6a analysis above is retained only as the **rationale for
rejecting it**, not as a specified fallback. Any claim that OPAQUE is "audited" or "validated" *for
fond* is downgraded to `[Validation Required]` pending A0.5 (§C).

### C. Rust crate evaluation & selection

**Decided: `opaque-ke` (facebook/novi) as the OPAQUE implementation, with no SRP-6a fallback (A0.5
I.15 / VR-0212-C.0).** Every maintenance/audit statement below is *evidence for A0.5*, not a
validation — the exact release/features are `[Validation Required]` until a human reviewer signs off
(I.10), and **fond never hand-rolls a PAKE**.

| Crate | Role | Maintenance / provenance | Audit provenance | RFC conformance | Dependency footprint |
|---|---|---|---|---|---|
| `opaque-ke` | OPAQUE aPAKE (sole account PAKE) | Facebook/novi; the reference-grade Rust OPAQUE, tracking the CFRG work that became RFC 9807 | The public NCC Group review ([archived PDF, 2021](https://web.archive.org/web/20211213145520id_/https://research.nccgroup.com/wp-content/uploads/2021/12/NCC_Group_WhatsAppLLC_OPAQUE_Report_2021-12-10_v1.3.pdf)) covered **v0.5.0 and an earlier draft, NOT the current v4.0.1**. Cite it as **ancestor-lineage evidence only**; a human MUST review the intervening changes for the pinned release. Do **not** call the pinned release "audited" `[Validation Required]` (I.10) | Implements OPAQUE-3DH per RFC 9807; exercises upstream RFC vectors — **conformance evidence, not an audit** `[Validation Required]` | Elliptic-curve group (ristretto255), its `voprf` OPRF dependency, plus KSF / `hkdf` / `hmac` / `sha2` / `rand` / `zeroize` — larger, justified by aPAKE properties |
| `voprf` | Oblivious PRF (transitive dep of `opaque-ke`) | Facebook/novi; the OPRF `opaque-ke` builds on, tracking RFC 9497 | The current standalone `voprf 0.5.0` **postdates** the 2021 OPAQUE audit; "reviewed alongside" would **overstate** exact-code coverage. Treat as **unaudited in its current form** `[Validation Required]` | Runs RFC 9497 VOPRF vectors — conformance evidence `[Validation Required]` | RustCrypto curve + hash primitives |

Rationale for `opaque-ke`:

- **It is the RFC 9807 implementation in Rust with an audit *lineage* to cite**, built by the group
  that authored much of the OPAQUE / VOPRF standards work. The 2021 NCC review is **lineage
  evidence** for an ancestor version, **not** an audit of the pinned release; the audit gap for
  intervening changes is an explicit human-review item (`[Validation Required]`, §G I.10).
- **It composes cleanly with the rest of FONDENC2:** its KSF slot accepts **Argon2id**, letting us
  bind account key-stretching to the **A0.3 Argon2id profile registry** rather than introducing a
  second Argon2 regime (§F).
- **The larger dependency footprint is the price of an aPAKE** and is acceptable given the
  server-compromise properties (§B).

**Explicit non-goal.** OPAQUE is **not** implemented by hand, and there is **no** hand-rolled or
SRP-6a fallback. If no maintained, reviewable OPAQUE crate is acceptable at implementation time, the
account/sync feature **stays disabled** — **never** a bespoke or unaudited-verifier construction.

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

**Vault-authorization signature transcript — decided (A0.5, I.14).** Every destructive / key-material
operation carries an Ed25519 signature over a **fully pinned, canonical, domain-separated transcript**
— no ambiguous or implementation-defined message:

```text
auth_sig = Sign_{ed25519}( canonical(
    label        = len_prefix("fond/fondenc2/v2/authz")   # domain-separation label
  ‖ protocol_ver = u16                                     # FONDENC2 protocol version
  ‖ vault_id ‖ account_id ‖ member_id ‖ device_id          # who
  ‖ op_type     = u8                                       # delete / roster / epoch / enroll / revoke
  ‖ roster_hash ‖ vault_epoch                              # authorization state in force
  ‖ request_nonce = 16 bytes ‖ monotonic_counter = u64     # replay protection
  ‖ not_after   = u64 (optional; 0 = none)                 # expiry where relevant
  ‖ payload_digest = SHA-256(op payload) ) )               # what
```

- **Which key signs.** The member/admin's **per-device** Ed25519 key (K.7), verified against the
  device certificate in the roster identified by `roster_hash` (historical authorization, ADR-021.1
  N-04) — so a later-revoked signer's *past* authorized ops still verify, and an unauthorized signer
  is rejected.
- **Replay storage & verification obligation.** The server (and every client) persists seen
  `(member_id, device_id, request_nonce)` / `monotonic_counter` and **rejects replays**; the canonical
  encoding is length-prefixed and fixed-width so two distinct operations can never share a transcript.
  This is the implementable transcript the earlier text lacked.

**The invariant that falls out of the separation.** A **login-layer reset restores account access but
can NEVER authorize vault destruction or membership change.** Concretely: a password reset replaces
the OPAQUE record (Layer 1); it does **not** — and cannot — reconstruct the member's **Secret Key**
(never uploaded, ADR-020) or their **Ed25519 vault identity key** (Layer 2). So a reset actor can log
back in and read ciphertext, but cannot unwrap the Vault Key, cannot sign a new roster, and cannot
delete or rewrite vault state. An attacker who fully compromises the *service* layer is likewise
capped: no forged vault authorization, by construction. Identity continuity is **anchored in
client-held trusted state** (the signed roster/account chain), with any server-side immutability
being **defense-in-depth only** — the client-anchored construction of §E (I.13).

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

### E. Binding vault identity keys to the account (client-anchored, resolves K.10)

ADR-020 **K.10** asks how the X25519 / Ed25519 vault identity keys bind to the account so a
login-layer reset cannot forge vault authorization. **Decided (A0.5, I.12 / I.13 / N-09): a
client-anchored, self-signed sidecar chained into the roster/account history** — *not* an
OPAQUE-envelope / `export_key` binding, and *not* a server-enforced immutable column, because the
threat model treats the server as **malicious**, not merely buggy.

- **Why not envelope/export-key.** OPAQUE `client_identity` authenticates an identity for **one**
  registration record and `export_key` is tied to **that** record; a password reset normally mints a
  **fresh** record and export key. Neither is a reset-independent anchor, so binding identity to the
  OPAQUE envelope would let a reset silently rebind identity.
- **Why not a server "non-resettable column".** A malicious server cannot be trusted to preserve an
  immutable attribute across a reset it performs. Server immutability is therefore **defense-in-depth
  only**, never the security invariant.

**Client-anchored construction.** The member's vault identity public keys (Ed25519 signing, X25519
transport — ADR-020 §G) are published as a **canonical self-signed sidecar attribute**:

```text
identity_commitment = { account_id, member_id, vault_identity_pubkeys, prev_commitment_hash }
sidecar_sig         = Sign_{member_ed25519}( len_prefix("fond/fondenc2/v2/identity"
                        ‖ account_id ‖ member_id ‖ vault_identity_pubkeys
                        ‖ prev_commitment_hash ‖ registration_context) )
```

- **Chained into roster/account history.** Each commitment carries `prev_commitment_hash`, chaining
  from the member's prior commitment and into the signed **roster directory** chain (ADR-020 §G) —
  so identity continuity is a **client-verifiable hash chain**, not a server promise.
- **Clients reject unchained identity changes.** A client **rejects** any identity/key change that
  does **not** chain (via `prev_commitment_hash` + a valid old-key signature) from the commitment it
  already trusts. A rotation is a new self-signed commitment **authorized by the old identity key**;
  a first-registration commitment is trusted on first sight (TOFU) and pinned thereafter.
- **Reset leaves identity untouched.** A password reset replaces only the resettable OPAQUE record;
  it neither produces a valid `sidecar_sig` (the server lacks the identity private key) nor a valid
  chain link, so it **cannot** rebind identity. The server MAY *also* mark the attribute append-only
  (defense-in-depth), but clients never rely on that.

The canonical encoding and label for both this sidecar signature and the §D authorization signature
are **pinned** (length-prefixed `fond/fondenc2/v2/...` labels, decided under I.14 in §D).

### F. Chosen ciphersuite, parameters & A0.5 sign-off

The **suite is pinned** (A0.5 VR-0212-F.1–F.6); the **Argon2id KSF triples remain deferred** to the
K.13 measured-device review, and the **exact `opaque-ke` types/features/dependency graph** remain a
human-audit item (I.10/I.11). "Pinned" below means fixed by decision; `[Validation Required]` marks
what still needs the human reviewer.

**OPAQUE ciphersuite (RFC 9807 / `opaque-ke` configuration) — pinned:**

| Component | Pinned choice | Note |
|---|---|---|
| AKE | **OPAQUE-3DH** | the primary RFC 9807 construction (F.1); no alternative AKE |
| OPRF group | **ristretto255** | RFC 9497 VOPRF; P-256 is **not** a runtime alternative (F.2) |
| Hash | **SHA-512** | the defined partner for ristretto255 (F.3) |
| KDF / MAC | **HKDF-SHA-512 / HMAC-SHA-512** | aligns with the profile; pin exact crate types/lengths (F.4) |
| **KSF (key-stretching)** | **Argon2id via the A0.3 profile registry, under a distinct OPAQUE domain** | shares the parameter table only, with its own protocol domain + fixed-salt adapter (F.5); triples deferred (K.13) |
| Nonce / seed lengths | **per RFC 9807** | exact suite lengths, no application override (F.6) |

**KSF binding — decided (A0.5, VR-0212-F.5 / F.7).** The OPAQUE key-stretching function reuses the
**A0.3 Argon2id profile registry**
([ADR-020 A0.3 appendix](020-zero-knowledge-identity.md#appendix-kdf-profiles-rotation--migration-a03))
for its **parameter table only**, but under an **explicit, distinct OPAQUE protocol domain** and the
crate adapter's pinned **fixed-salt** semantics — RFC conformance vectors use the identity KSF and do
**not** exercise this integration, so it is called out separately. The account KSF and the member MUK
use **distinct domain-specific profile ids even if their initial triples match** (F.7), so a
login-latency retune can never silently change vault-unlock policy.

**Passphrase used twice — domain-separated, decided (A0.5, I.16).** The *same* user passphrase feeds
the **OPAQUE login** (service auth) and the **MUK** Argon2id (vault unwrap;
[ADR-020 §C](020-zero-knowledge-identity.md#c-domain-separation--the-two-secret-muk)). These are
domain-separated by **explicit length-prefixed application labels** — `fond/fondenc2/v2/opaque-ksf`
for the OPAQUE KSF input and `fond/fondenc2/v2/muk` for the MUK — **in addition to** the structural
separation (the OPAQUE KSF runs over the **OPRF-transformed** password keyed by the server's OPRF
secret and never mixes in the Secret Key, while the MUK runs over the **raw passphrase + Secret-Key
pepper** under its own per-member salt). Independent salts and secret inputs are retained and no
derived value is ever reused, so neither derivation's output aids attacking the other. Reusing the
A0.3 registry shares parameters only, never a derived value.

**Threat-notes recap — what a FULL server compromise does and does not yield (for A0.5, from §A).**

- **(a) OPAQUE login records alone → no offline password guessing — decided qualification (A0.5,
  VR-0212-F.8).** A dump of the OPAQUE records **excluding the `oprf_seed`** yields no
  offline-guessable verifier and no OPRF oracle, so it does not enable offline verification;
  pre-computation is resisted by the OPRF secret. But a **corrupted single server that also holds the
  `oprf_seed`** **can** mount an offline dictionary attack, priced per guess by the KSF, exactly as
  [RFC 9807 §10.11](https://www.rfc-editor.org/rfc/rfc9807.html#section-10.11) states. Rate limits do
  **not** constrain a malicious operator; the honest claim is "records-without-seed give no offline
  guessing", not "no offline guessing ever".
- **(b) Exfiltrated `wrapped_key_package` blobs + OPAQUE records → still no vault decryption.** The
  **Secret Key is never on the server** (ADR-020 two-secret model), so even holding every wrapped key
  package and OPAQUE record the attacker cannot derive the **MUK** or unwrap the **Vault Key**; and
  without an Ed25519 vault key it cannot forge vault authorization (§D). The blast radius of a breach
  is capped by material the server never possesses.
- **(c) But metadata still leaks — do not overclaim.** Blob counts, sizes, change timestamps, and
  device ids remain visible to the server, and a revoked member can still enumerate opaque
  `object_id`s (cross-ref [ADR-021.1 §G](#g-honest-limits--what-this-cannot-do) and the ADR-021
  Threat model's documented residuals). Zero-knowledge covers **content**, not traffic analysis.

**A0.5 sign-off checklist.** The independent reviewer must still confirm: (1) the exact `opaque-ke`
`4.0.1` / `voprf` features and dependency graph despite the 2021 v0.5.0 audit gap (I.10); (2) the KSF
fixed-salt adapter and crate types against the pinned suite (I.11); (3) the client-anchored identity
construction (I.12/I.13); (4) the vault-authorization transcript (I.14); and (5) the Argon2id KSF
triples once measured (K.13). Items resolved in this revision — no SRP fallback (I.15), the pinned
suite (F.1–F.6), distinct KSF domain/profile ids (F.5/F.7), the qualified record-dump claim (F.8),
and the length-prefixed two-use labels (I.16) — no longer block, but the crate-audit and measured
figures remain gating. **No auth/sync code lands until the deferred items clear.**

### G. A0.5 remediation status & validation

- **ADR-021.2 items (I.10–I.16), post-remediation.** **Resolved:** **I.12/I.13** (client-anchored
  self-signed identity sidecar, §E), **I.14** (vault-authorization transcript, §D), **I.15** (no SRP
  fallback, §B/§C). **Decided per review:** **I.11** ciphersuite (OPAQUE-3DH ristretto255/SHA-512,
  §F) and **I.16** (length-prefixed two-use labels + distinct profile ids, §F). **Still deferred to
  the human reviewer:** **I.10** (exact `opaque-ke 4.0.1` / `voprf` features and dependency graph
  despite the 2021 v0.5.0 audit gap) and the crate-adapter half of **I.11** (fixed-salt KSF adapter),
  plus the Argon2id KSF triples (K.13). Full status is in the ADR-021.1
  [§I table](#i-open-questions-for-a05-independent-review).
- **Not a re-spec of the vault.** This appendix decides the *account-authentication* primitive and the
  auth-vs-authorization boundary only; the key hierarchy, envelope, roster, and causal history remain
  authoritative in ADR-020 and the ADR-021.1 appendix. The whole stays a **composition of reviewed
  primitives** (OPAQUE / RFC 9807, VOPRF / RFC 9497, Argon2id, Ed25519, X25519, HPKE) with **nothing
  hand-rolled** — which is exactly why the A0.5 independent review is mandatory before any
  implementation.
