# ADR-004: CLI Design — Scriptable Human-First Output

**Status**: Accepted
**Date**: 2025-07-13
**Decision**: Build the CLI with `clap` v4 derive, default to readable table/text output, and provide `--json` as a stable scripting contract across commands.

## Context

fond is CLI-first through MVP and Beta. The primary user is comfortable in a terminal, and even later web/native interfaces are supposed to sit on top of the same core rather than replace the CLI's role as the reference interface.

The CLI therefore has two jobs at once. It must be pleasant enough for daily interactive use at the stove or during planning, but it must also be predictable for scripts, tests, and future front-ends. The roadmap's examples span both simple commands like `fond view` and structured flows like `fond pantry check`, `fond plan`, and `fond import ... --dry-run`.

Section 9 and Section 12 already point to the shape: `clap` for parsing, `comfy-table`/`tabled` for human output, `$EDITOR` for editing, and `--json` for machine-readable output. This ADR turns that tooling preference into a product-level interface rule.

## Decision

fond will use **`clap` v4 derive** for command parsing and help generation. Command design stays action-oriented and discoverable, with grouped subcommands where the domain needs them.

Human-readable output is the default for terminal use, typically via clean tables or formatted recipe views. Any command with meaningful structured output should also support **`--json`**, and that JSON shape is treated as a stable contract that tests, scripts, and later UI layers can rely on. Editing commands open the user's configured **`$EDITOR`** rather than inventing a custom editor.

## Rationale

- **Discoverability**: `clap` provides robust help text, completions, validation, and error messages out of the box.
- **Dual-mode UX**: readable defaults serve interactive cooks, while `--json` preserves automation and composability.
- **Consistency**: one CLI grammar across recipe, pantry, planning, and import features reduces cognitive load.
- **Future-proofing**: tests and later interfaces can consume structured output without scraping tables.
- **Unix-friendly editing**: `$EDITOR` respects existing workflows instead of forcing an in-app editor.

## Alternatives Considered

| Alternative | Rejected Because |
|------------|-----------------|
| Hand-rolled argument parsing | More fragile, worse help output, and unnecessary given mature ecosystem tooling. |
| `structopt` | Historically good, but superseded by `clap` derive and no longer the best current choice. |
| Interactive-only TUI | Useful for cook mode later, but not scriptable and a poor primary interface for import, planning, and automation. |
| Ad hoc text output without `--json` | Forces downstream tools and tests to parse presentation output, which is brittle by design. |

## Consequences

- Strong upside: the CLI becomes both the best manual interface and the canonical automation surface.
- Strong upside: help text, completions, and errors arrive mostly for free through `clap`.
- Tradeoff: maintaining a stable `--json` contract slows careless CLI changes and needs deliberate versioning discipline.
- Tradeoff: some commands may need to maintain both a polished human renderer and a structured serializer.
