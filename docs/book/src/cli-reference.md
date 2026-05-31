# CLI Reference

fond uses a subcommand-based CLI. All list commands support `--format table|json` (or `--json` shorthand).

## Global Options

| Option | Description |
|--------|-------------|
| `--data-dir <PATH>` | Override the data directory (also: `FOND_DATA_DIR` env var) |
| `--format <table\|json>` | Output format (default: `table`) |
| `--json` | Shorthand for `--format json` |

## Commands

### `fond init`

Initialize the fond data directory. Creates `recipes/`, `photos/`, `config.toml`, and `fond.db`.

### `fond add <path>`

Add a `.cook` file to your collection. Copies the file into `recipes/` and indexes it.

### `fond list`

List all recipes.

```bash
fond list                  # all recipes
fond list --tag dinner     # filter by tag
```

### `fond view <slug>`

View a recipe in full detail, including metadata, ingredients, steps, and tags.

### `fond search <query>`

Full-text search across recipe titles, ingredients, and steps. Uses SQLite FTS5.

### `fond edit <slug>`

Open a recipe in your `$EDITOR` for editing, then re-index.

### `fond tag <slug>`

Manage tags for a recipe.

```bash
fond tag chicken-adobo --add comfort-food
fond tag chicken-adobo --remove spicy
fond tag chicken-adobo --list
```

### `fond rm <slug>`

Delete a recipe (file and index entry). Use `--yes` to skip confirmation.

### `fond reindex`

Rebuild the SQLite database from `.cook` files. The database is a derived index — your `.cook` files are the source of truth, so this is always safe.

### `fond import paprika <path>`

Import recipes from a Paprika archive (`.paprikarecipes` or `.paprikarecipe`).

```bash
fond import paprika ~/Downloads/recipes.paprikarecipes
fond import paprika recipe.paprikarecipe --dry-run
```

### `fond import url <url>`

Import a recipe from a website with schema.org Recipe markup.

```bash
fond import url https://www.seriouseats.com/chicken-adobo-recipe
fond import url https://example.com/recipe --dry-run
```

### `fond export`

Export recipes to JSON or Paprika format.

```bash
# JSON to stdout (all recipes)
fond export

# JSON single recipe to file
fond export --recipe chicken-adobo --output recipe.json

# Paprika archive
fond export --export-format paprika --output recipes.paprikarecipes

# Single Paprika recipe
fond export --export-format paprika --recipe chicken-adobo --output chicken.paprikarecipe
```

### `fond pantry add <items...>`

Mark items as available in your pantry.

```bash
fond pantry add "soy sauce" "vinegar" "garlic" "rice"
```

### `fond pantry rm <items...>`

Remove items from your pantry.

### `fond pantry list`

List pantry items. Use `--all` to include inactive items.

### `fond pantry check <slug>`

Check pantry coverage for a recipe — shows which ingredients you have and what's missing.

```bash
fond pantry check chicken-adobo
# Output: Coverage: 75% (3/4 ingredients)
```

### `fond grocery from-recipe <slug>`

Generate a grocery list from a recipe, subtracting pantry items.

```bash
fond grocery from-recipe chicken-adobo
fond grocery from-recipe chicken-adobo --include-pantry
```

### `fond completions <shell>`

Generate shell completions for `bash`, `zsh`, `fish`, or `powershell`.
