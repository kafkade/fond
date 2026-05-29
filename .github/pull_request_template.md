## Description

<!-- What does this PR do? Provide a brief summary of the changes. -->

## Related Issues

<!-- Link related issues: "Closes #123" or "Relates to #456" -->

## Type of Change

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Documentation update
- [ ] Refactoring (no functional changes)
- [ ] CI / infrastructure
- [ ] Other (describe below)

## Crate

<!-- Which crate(s) does this touch? -->

- [ ] `fond` — CLI binary
- [ ] `fond-core` — Shared domain logic
- [ ] `fond-domain` — Domain types and traits
- [ ] `fond-store` — SQLite persistence, migrations, FTS5
- [ ] `fond-import` — Import pipeline (Paprika, schema.org)
- [ ] `fond-scrape` — Web scraping (HTTP, JSON-LD)
- [ ] `fond-timeline` — Cooking timeline engine
- [ ] `docs/` — Documentation

## Data Integrity Checklist

<!-- fond is a local-first tool — user data ownership is non-negotiable -->

- [ ] No user data is sent to any server without explicit opt-in
- [ ] `.cook` files round-trip losslessly (parse → emit = identical)
- [ ] Import operations are idempotent (re-importing creates no duplicates)
- [ ] User edits to recipes are never overwritten by auto-import
- [ ] New fields track provenance (source + timestamp)
- [ ] `fond reindex` still produces a correct DB after this change

## Checklist

- [ ] I have read [CONTRIBUTING.md](CONTRIBUTING.md)
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] I have updated documentation (if applicable)
