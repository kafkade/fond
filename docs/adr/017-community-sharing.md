# ADR-017: Community Recipe Sharing — Ownership-Preserving Bundles, No Central Server

**Status**: Proposed
**Date**: 2025-07-13
**Decision**: Share recipes as self-contained, portable `.fondshare` **bundles** (a ZIP of verbatim `.cook` files + a provenance/attribution/license manifest) that move over any transport the user chooses. fond never performs a network upload; imports always flow through the existing review pipeline (ADR-010); every publish is an explicit, per-action consent.

## Context

Phase 8 (ROADMAP §13) lists "community recipe sharing (opt-in, ownership-preserving)" as a moonshot. The obvious way to build recipe sharing — a hosted service where users upload recipes to a central catalog and others download them — is exactly the shape this project exists to avoid. It would contradict the two load-bearing principles:

- **Principle #1 (local-first):** core features must work fully offline; the network is optional.
- **Principle #2 (data ownership):** the user owns 100% of their data forever, `.cook` files are the source of truth, and there is no vendor lock-in.

A central server also creates hard secondary problems that are off-mission for a household cooking tool: account systems, moderation, a redistribution surface for copyrighted imports (Paprika/NYT/ATK content the user imported for personal use must not be re-published), takedown handling, and an availability dependency that breaks the offline guarantee.

At the same time, sharing has a real, mundane use: sending a recipe to a family member or friend so it lands cleanly in *their* fond, with attribution and license intact, and without silently overwriting anything they already have. The design challenge is to make sharing frictionless and lossless (Principle #6, import-as-superpower) **without** building the centralized thing.

## Decision

### The bundle is the unit of sharing

A shared recipe travels as a **`.fondshare` bundle** — a ZIP archive:

```text
manifest.json          # trust + provenance record (ShareManifest)
recipes/<slug>.cook    # source-of-truth files, verbatim (lossless)
photos/<name>          # optional linked photo blobs (content-addressed)
```

`manifest.json` (schema v1) records, per recipe: `slug`, `title`, `cook_file`, a deterministic `cook_sha`, and the **provenance fields** `source`, `source_url`, `license`, and `attribution`; the manifest header carries `bundle_id` (UUIDv7), `fond_version`, `created_at`, and optional `shared_by`.

Because a bundle is just a file, it moves over **any channel the user already trusts**: git, a synced folder (Syncthing/Dropbox/iCloud, per ADR-012), a USB stick, or email. fond writes and reads the file; the user owns the transport.

### Provenance travels *in the file*, not only in the manifest

On export, provenance is **stamped into the `.cook` frontmatter** (`source`, `source url`, `license`, `shared by`) using the lossless `CookDocument` edit layer (ADR-015/ADR-011): existing keys are never clobbered, and the rest of the file (steps, sections, comments, unknown keys) round-trips faithfully. This means origin and license survive even if the recipe is later re-exported, converted, or separated from its bundle — the attribution is not metadata that can fall off.

### Import goes through the review pipeline (ADR-010)

`fond share import` never writes recipes directly. Each recipe in a bundle is enqueued into the existing **review queue** (`source_type = "shared-bundle"`) with its attribution and license carried as review warnings, so a human always confirms before a shared recipe becomes a canonical `.cook` file. `--dry-run` previews without writing, consistent with every other importer.

Import is **idempotent**. Dedup uses the same signal the Paprika/URL importers use — the source URL — against both the existing library and drafts already waiting in review; URL-less recipes fall back to a content digest of the `.cook` text. Re-importing the same bundle skips what is already present or queued rather than piling up duplicates.

### Distribution model: git-based / static-index exchange — no mandatory server

`fond share publish` copies a bundle into a **static index directory** (default `<data-dir>/shared/outbox/`, overridable with `--to`). That directory is designed to be a git repo, a synced folder, or a plain shared drive: a fully decentralized, "static index" exchange in the spirit of file-sync-first (ADR-012). There is **no central fond server**, no account, and no federation protocol to run.

Publishing is a **network-adjacent action**, so it is gated by **explicit, per-action consent**: fond prints exactly which recipes (and licenses) would leave the device and requires either an interactive confirmation or an explicit `--yes`. fond itself performs **no upload** — it stages the bundle and the user pushes/syncs it. This keeps "nothing leaves a device without explicit consent" literally true.

## Rationale

- **Upholds #1 and #2 by construction:** everything works offline; the `.cook` files stay the source of truth; there is no service to depend on or lock into.
- **Attribution is durable:** stamping provenance into the frontmatter means license/credit cannot be lost by later processing, addressing the copyright/attribution risk in the issue.
- **Reuses the trusted path:** import lands in the ADR-010 review queue, so shared content gets the same human gate, dedup, and no-data-loss guarantees as every other import — no new "write directly" code path to trust.
- **Consent is per-action and legible:** publish shows what would be shared and refuses to act without an explicit yes; there is no background sync of user recipes.
- **Decentralized distribution is a directory, not a protocol:** a git/synced/static-index folder is something users already understand and control, and it needs zero server operation.

## Alternatives Considered

| Alternative | Rejected Because |
|------------|-----------------|
| Central hosted catalog (upload/browse/download) | Breaks local-first and ownership; introduces accounts, moderation, availability dependency, and a redistribution surface for copyrighted imports. |
| Federation protocol (ActivityPub-style) between instances | Large protocol + operational surface for a household tool; premature for a moonshot with unproven demand. |
| Write imported shared recipes straight to `recipes/` | Bypasses the ADR-010 human review gate and risks silent overwrite/duplication; inconsistent with every other importer. |
| Attribution only in a sidecar/manifest | Attribution falls off the moment a `.cook` file is separated from its bundle; fails the "carry their origin" requirement. |
| Auto-sync a "shared" folder in the background | Would move data off-device without a clear per-action consent; violates the acceptance criterion. |

## Consequences

- **Upside:** sharing is a pure file exchange — offline, ownership-preserving, transport-agnostic, and trivially decentralized via git/sync.
- **Upside:** provenance and license ride inside the `.cook` file and are preserved losslessly on both export and accept.
- **Upside:** re-import is safe and idempotent; the review queue keeps a human in the loop.
- **Tradeoff:** no built-in discovery — finding bundles to import is left to the user's own channels (a repo, a link, a message). This is a deliberate consequence of refusing a central catalog and can be layered on later (e.g. a community-run static index) without changing the bundle format.
- **Tradeoff:** the `cook_sha` digest is a non-cryptographic, machine-stable content hash for integrity/dedup, not tamper resistance; bundles are not signed. Signing/trust can be added to the manifest schema (bumping `schema_version`) if a real threat model emerges.
- **Non-goal for now:** enforcing license *compatibility* on import — fond records and displays the asserted license and leaves the human to honor it, rather than making legal judgments.
