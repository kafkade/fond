# Contributing to fond

Thank you for your interest in contributing to fond!

## Code of Conduct

Be respectful, constructive, and inclusive. We're building a tool for home cooks — let's treat each other well.

## How to Contribute

### Reporting Bugs

- Open an issue using the bug report template
- Include steps to reproduce, expected behavior, and actual behavior
- Include platform (Linux/macOS/Windows) and version information

### Suggesting Features

- Open an issue using the feature request template
- Describe the use case and how it aligns with fond's local-first, data-ownership principles
- Note: features that require a server for core functionality will not be accepted

### Adding an Importer

- Importers are a great way to contribute! Check the `area/import` label for requested sources
- Each importer lives in `fond-import/` and implements the `Importer` trait
- Importers must support: `--dry-run`, idempotent re-import via source IDs, and provenance tracking
- Clean recipes write immediately; ambiguous ones queue for `fond review`
- Include test fixtures (real or anonymized export files) in `tests/fixtures/`

### Pull Requests

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Make your changes
4. Ensure all checks pass: `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace`
5. Submit a pull request with a clear description

### Security Vulnerabilities

If you discover a security vulnerability, **do not open a public issue**. Instead, please use GitHub's private vulnerability reporting feature.

## Development Setup

```sh
# Clone the repo
git clone https://github.com/kafkade/fond.git
cd fond

# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Run the CLI
cargo run -p fond -- --help
```

## Architecture

See `.github/copilot-instructions.md` for the full architecture overview and `docs/adr/` for Architecture Decision Records.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
