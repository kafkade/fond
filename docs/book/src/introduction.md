# fond

**fond** (*French: fond de cuisine* — the browned bits in a pan that form the foundation of a sauce; *English: fondness* — warmth, affection) is a local-first, CLI-first, Cooklang-native personal cooking and recipe management application.

## Design Principles

1. **Local-first** — works fully offline. All core features function without an internet connection.
2. **Data ownership** — you own 100% of your data. Recipes are plain-text `.cook` files.
3. **Family-shared** — designed for a household from day one.
4. **Cooklang-native** — recipes stored as `.cook` files, the open plain-text recipe format.
5. **CLI-first** — the CLI is the primary interface and a first-class product.
6. **Import as a superpower** — importing from Paprika and the web is frictionless and lossless.

## What Can It Do?

- **Manage recipes** — add, view, search, edit, tag, and delete `.cook` files
- **Import recipes** — from Paprika archives (`.paprikarecipes`) or any website with schema.org recipe markup
- **Export recipes** — to JSON (with full metadata) or Paprika format (for sharing)
- **Track your pantry** — mark what you have on hand, check recipe coverage
- **Generate grocery lists** — see what you need to buy for a recipe, minus what's already in your pantry

## Quick Example

```bash
# Initialize your recipe collection
fond init

# Import from Paprika
fond import paprika ~/Downloads/recipes.paprikarecipes

# Import from a URL
fond import url https://www.seriouseats.com/chicken-adobo-recipe

# View a recipe
fond view chicken-adobo

# Check what you need from the store
fond pantry add "soy sauce" "vinegar" "garlic"
fond grocery from-recipe chicken-adobo
```
