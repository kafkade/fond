# ADR-001: Core Language — Rust

**Status**: Accepted
**Date**: 2025-07-13
**Decision**: Use Rust 2021 for the CLI and all shared crates, following the proven toku portfolio pattern.

## Context

fond is explicitly CLI-first, local-first, and multi-platform. The core must ship as a fast, portable single binary that behaves the same on Windows, macOS, and Linux, while still leaving a path open for later web and Apple front-ends.

The roadmap's architecture in §2 centers on a shared Rust core (`fond-core`, `fond-domain`, `fond-store`, `fond-import`, `fond-timeline`) with thin interfaces layered on top. That architecture only works if the core language is equally good at systems programming, CLI ergonomics, embedded storage, and later FFI/server reuse.

Section 12 already mirrors toku's proven stack: `clap`, `rusqlite`, `refinery`, `serde`, `reqwest`, `axum`, and `cargo-dist`. Keeping fond on the same language foundation as toku improves portfolio consistency, reduces switching costs for the maintainer, and lets patterns from one project transfer cleanly to the other.

## Decision

fond will be built in **Rust (2021 edition)**. The CLI binary and all shared libraries use Rust as the single implementation language, with future interfaces consuming the same core rather than re-implementing business logic.

This follows the same portfolio strategy already used successfully in `toku`: a Rust core first, then additional interfaces only after the domain model is stable.

## Rationale

- **Single-binary distribution** fits the product promise: fast startup, no runtime dependency, easy cross-platform installers via `cargo-dist`.
- **Strong domain modeling** matters for cooking rules: enums, exhaustiveness, traits, and ownership help model quantities, timelines, import states, and household scoping safely.
- **Cross-platform reuse** supports the roadmap: CLI now, Axum web later, and UniFFI/C ABI bridges for Apple after the core stabilizes.
- **Ecosystem fit** is already validated in the stack choices for SQLite, FTS5, CLI parsing, testing, and serialization.
- **Portfolio consistency** with toku lowers maintenance friction and makes the architecture more legible across the maintainer's projects.
- **Maintainer fit** is already validated in the roadmap: Rust is the developer's strongest language.

## Alternatives Considered

| Alternative | Rejected Because |
|------------|-----------------|
| Go | Simpler operationally, but weaker for rich domain modeling and not a good fit for the later UniFFI/Swift bridge. |
| TypeScript/Node | Adds a runtime dependency, hurts the single-binary CLI story, and is a weaker fit for local-first systems code. |
| Swift | Attractive for Apple clients, but too Apple-centric for a Windows/Linux CLI-first product. |
| Python | Packaging and performance tradeoffs are too costly for a distributable cross-platform CLI. |

## Consequences

- Strong upside: one language and one core can serve CLI, web, and native layers without logic drift.
- Strong upside: distribution, testing, and persistence choices align with toku and established Rust tooling.
- Tradeoff: early development velocity is slower than with Python or Go, especially while shaping the domain model.
- Tradeoff: compile times and the Rust learning curve can raise contributor friction, so documentation and clear crate boundaries matter.
