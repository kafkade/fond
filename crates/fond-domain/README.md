# fond-domain

Domain types, traits, and errors for [fond](https://github.com/kafkade/fond) — a local-first, CLI-first personal cooking & recipe manager.

This crate contains pure data structures and type definitions with no I/O or side effects. All entities that flow through fond are defined here.

## Key Types

- **`Recipe`** — Parsed representation of a `.cook` file: title, slug, ingredients, steps, cookware, tags, timers, and metadata.
- **`Ingredient`**, **`Step`**, **`Cookware`** — Components of a recipe.
- **`RecipeFilter`** — Filter criteria (tags, max time, source) for search and list queries.

## Utilities

- `parse()` / `emit_cook()` — Cooklang round-trip: parse a `.cook` file into a `Recipe` and emit it back without data loss.
- **`CookDocument`** — Lossless structured **edit** layer over raw `.cook` text: splits a file into ordered frontmatter + body blocks, offers surgical metadata setters (title, servings, tags, times, description, source, `image`) and body/step edits, and re-emits **byte-for-byte when unedited**. Powers native app editing via `fond-ffi`.
- `slugify()` / `title_from_stem()` — Derive URL-safe slugs from titles and vice versa.
- `parse_time_minutes()` — Normalize human-readable time strings ("1 hour 30 min") to minutes.
- `escape_fts5_query()` — Sanitize user input for safe FTS5 full-text queries.
- `update_tags_in_cook_source()` — Edit tags in `.cook` file source text while preserving all other content.

## Usage

```rust
use fond_domain::{parse, emit_cook, Recipe, RecipeFilter};

let source = std::fs::read_to_string("chicken-adobo.cook").unwrap();
let recipe = parse(&source, "chicken-adobo").unwrap();
println!("{}: {} ingredients", recipe.title, recipe.ingredients.len());
```

## License

[MIT](https://github.com/kafkade/fond/blob/main/LICENSE)

Part of the [fond](https://github.com/kafkade/fond) workspace.
