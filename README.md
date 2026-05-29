# 🍳 fond

**A local-first personal cooking & recipe management system.**

Your recipes, your data, your kitchen. No accounts. No cloud dependency. Family-shared from day one.

fond combines the web-clipping convenience of [Paprika](https://www.paprikaapp.com/),
the open-format philosophy of [Cooklang](https://cooklang.org/), and the meal-planning
depth of [Mealie](https://mealie.io/) — in a CLI-first tool that keeps everything on
your machine as plain-text files you own forever.

```sh
fond import paprika ~/recipes.paprikarecipes  # Bring your Paprika collection
fond import url https://example.com/recipe    # Import from any recipe site
fond search "braised pork"                    # Instant full-text search
fond view chicken-adobo                       # Render a recipe
fond cook chicken-adobo --serve-at 19:00      # Backward-scheduled cook mode
fond pantry check chicken-adobo               # What do you have? 85% coverage
fond grocery from-recipe chicken-adobo        # Pantry-aware shopping list
fond plan week --add monday:dinner=chicken-adobo
fond scoreboard --since 2025-01-01            # What have you cooked most?
```

> **Status**: Early development — scaffolding and spikes. See [ROADMAP.md](ROADMAP.md)
> for the full plan.

---

## Why "fond"?

**fond** has two meanings that capture what this project is about:

In French cooking, a **fond** (*fond de cuisine*) is the caramelized layer of browned
bits left at the bottom of a pan after searing. It's the flavor foundation — you deglaze
it to build a sauce. Every great dish starts with a good fond.

In English, **fond** means warmth and affection — the feeling embedded in a family's
recipes, the handwritten notes in the margins, the memory of cooking together.

The name captures both: the technical foundation of good cooking and the emotional
foundation of a family kitchen.

> *"The foundation of your kitchen."*

---

## Principles

- **Your data, your files.** Recipes live as plain-text `.cook` files you can read,
  edit, and back up with any tool. SQLite is a derived index — delete it and
  `fond reindex` rebuilds everything.
- **Local-first.** Works fully offline. Network access is only for optional URL imports.
- **Family-shared.** Designed for a household from day one. Personal notes, ratings,
  and dietary preferences are per-user; recipes are shared.
- **Cooklang-native.** Recipes use the open [Cooklang](https://cooklang.org/) format.
  No proprietary file formats. No lock-in.
- **CLI-first.** A fast, scriptable command-line tool. Web, iOS, macOS, and Apple Watch
  interfaces are planned — all built on the same Rust core.
- **Import everything.** Bring your Paprika collection, clip recipes from NYT Cooking,
  scrape food blogs. Years of recipes should transfer in minutes.
- **Open source.** MIT licensed. Contributions welcome.

## Planned Features

- 📝 Recipe management with Cooklang round-trip fidelity
- 📥 Import from Paprika, schema.org sites, NYT Cooking, Cook's Illustrated
- 📤 Export to JSON, Paprika, and plain copy
- 🔍 Instant full-text search with tag, cuisine, and time filters
- ⏱️ Realistic cooking timelines with backward scheduling from serve time
- 🍳 TUI cook mode with live timers and step-by-step guidance
- 🥘 Presence-based pantry with recipe coverage percentage
- 🛒 Pantry-aware, aisle-grouped grocery lists
- 📅 Weekly meal planning with consolidated shopping
- 👨‍👩‍👧‍👦 Family profiles with dietary preferences and allergen flags
- 📊 Personal scoreboard — most cooked, highest rated, cooking history
- ⚖️ Recipe scaling with non-linear ingredient warnings
- 🌐 Web UI (`fond serve`) for non-CLI household members
- 📱 Native Apple apps (iOS, iPad, macOS, Apple Watch)

## Architecture

```
~/fond/
  recipes/            ← .cook files (SOURCE OF TRUTH)
    chicken-adobo.cook
    sourdough.cook
  photos/             ← content-addressed images
  fond.db             ← SQLite index (DERIVED, rebuildable)
  config.toml
```

### Tech Stack

- **Language**: Rust (2021 edition)
- **Recipe format**: [Cooklang](https://cooklang.org/) (`.cook` plain text)
- **Database**: SQLite with FTS5 full-text search
- **CLI**: clap v4 with `--json` for scripting
- **TUI**: ratatui (cook mode)
- **Web**: Axum + HTMX (Phase 4)
- **Architecture**: Cargo workspace — `fond`, `fond-core`, `fond-domain`, `fond-store`,
  `fond-import`, `fond-scrape`, `fond-timeline`

## Documentation

- [Product Roadmap](ROADMAP.md) — 9-phase plan from first recipe to moonshots
- [Architecture Decision Records](docs/adr/) — 10 ADRs covering load-bearing decisions
- [Contributing Guide](CONTRIBUTING.md) — development setup and contribution guidelines

## License

[MIT](LICENSE)

---

_Built by [kafkade](https://github.com/kafkade)._
