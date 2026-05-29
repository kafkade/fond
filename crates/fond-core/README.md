# fond-core

Shared business logic and services for [fond](https://github.com/kafkade/fond) — a local-first, CLI-first personal cooking & recipe manager.

This crate contains pure application logic — orchestration, validation, and transformations — with no I/O. Storage and network access are provided via trait implementations in downstream crates.

Currently re-exports all types from `fond-domain`. As fond grows, this crate will house cross-cutting business rules, recipe scaling logic, and validation pipelines that don't belong in the domain layer or the storage layer.

## Usage

```rust
use fond_core::Recipe;
```

## License

[MIT](https://github.com/kafkade/fond/blob/main/LICENSE)

Part of the [fond](https://github.com/kafkade/fond) workspace.
