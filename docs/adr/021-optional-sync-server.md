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

ADR-020 provides the identity/keys; ADR-015 (revised) provides client-side merge semantics; ADR-019
provides the crypto primitives. This ADR provides the optional hub.

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
  but cannot force a server to return data. Stated honestly to users.
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
  checked against the requesting member (no IDOR). Admin onboarding uses a **bootstrap token**.

### Sync protocol (client-driven, zero-knowledge)

- **Unit = an encrypted blob** addressed by an **opaque, keyed identifier** (HMAC under a namespace
  key), listed only in an **encrypted manifest**. Plaintext `recipe_slug`, content hashes, and
  UUIDv7 timestamps are **never** sent (slugs leak titles, content hashes enable known-file matching,
  UUIDv7 leaks time). Note: recipes are **slug-only today** (V010 gave UUIDs only to overlay rows) —
  ADR-020/A4 introduces a durable recipe UUID in `.cook` frontmatter for stable identity.
- Causal history is tracked by **per-device version vectors** (not wall-clock `updated_at`, which is
  skew-unsafe) and protected by a **signed, hash-chained manifest** with checkpoints. Clients verify
  the chain against **trusted local device state** to detect rollback, fork, and equivocation.
- **Merge is client-side and sibling-retaining** (revises ADR-015): a whole recipe body is **never**
  last-writer-wins. Concurrent edits produce retained siblings resolved by three-way / `.cook`-aware
  merge or an explicit user prompt; append-only logs union; deletes propagate via **tombstones**.
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
| SRP-6a for accounts | A stolen verifier enables offline guessing; OPAQUE (RFC 9807) preferred, SRP a reviewed fallback only. |
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
