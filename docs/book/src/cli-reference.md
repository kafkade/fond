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

### `fond doctor`

Check your setup for common problems. Currently it warns if your data directory (and therefore `fond.db`) appears to be inside a folder managed by a file-sync tool (Syncthing, Dropbox, iCloud Drive, Google Drive, OneDrive, or git). The derived database must never be synced — see [Syncing Your Recipes](./syncing.md). Advisory only; supports `--format json`.

```bash
fond doctor
fond doctor --format json
```

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

### `fond import photo <path>`

OCR a local recipe photo or scanned page into a queued Cooklang draft.

```bash
fond import photo ~/Downloads/grandma-card.jpg
fond import photo ~/Downloads/printed-recipe.png --dry-run
```

Printed recipes are the primary target. Handwriting is best-effort and always review-gated.

### `fond review list`

List pending OCR/import drafts that still need human review.

### `fond review show <id>`

Show one queued draft, including its Cooklang draft, warnings, and raw OCR text.

### `fond review edit <id>`

Open a queued draft in your editor so you can fix the Cooklang before importing it.

### `fond review accept <id>`

Write the queued draft to `recipes/` and index it as a normal recipe.

### `fond review reject <id>`

Reject a queued draft without importing it.

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

### `fond suggest`

"What can I cook now?" — rank recipes by how much of each one your pantry
already covers. Deterministic (no ML) and fully offline: recipes are sorted by
coverage % then by total time, and each suggestion lists the required
ingredients you're still missing.

By default it shows recipes missing at most 2 required ingredients. Use
`--max-missing N` to widen or narrow that, and `--limit N` to cap the list.

```bash
fond suggest                          # near-makeable recipes, ranked
fond suggest --max-missing 0          # only recipes you can make right now
fond suggest --cuisine italian        # filter by cuisine (a tag)
fond suggest --max-time 30 --limit 5  # quick options, top 5
fond suggest --format json            # machine-readable output
```

### `fond grocery from-recipe <slug>`

Generate a grocery list from a recipe, subtracting pantry items.

```bash
fond grocery from-recipe chicken-adobo
fond grocery from-recipe chicken-adobo --include-pantry
```

### `fond completions <shell>`

Generate shell completions for `bash`, `zsh`, `fish`, or `powershell`.

### `fond serve`

Launch the server-rendered web UI (Axum + HTMX) for household members who
prefer a browser to the CLI.

```bash
fond serve                                    # http://127.0.0.1:3000, no auth
fond serve --port 8080                         # custom port
FOND_AUTH_TOKEN=$(openssl rand -base64 24) \
  fond serve --bind 0.0.0.0                     # LAN, Basic Auth required
fond serve --bind 0.0.0.0 \
  --tls-cert cert.pem --tls-key key.pem \
  --auth-token "$FOND_AUTH_TOKEN"               # native HTTPS + auth
```

**Binding to anything other than loopback requires authentication.** fond
refuses to start on a non-loopback address (e.g. `0.0.0.0`, a LAN IP) unless you
set a shared token or explicitly opt out.

| Flag / env | Purpose |
|---|---|
| `--port` / `FOND_PORT` | Port to listen on (default `3000`). |
| `--bind` / `FOND_BIND` | Address to bind (default `127.0.0.1`). Use `0.0.0.0` for LAN. |
| `--auth-token` / `FOND_AUTH_TOKEN` | Shared secret required as the HTTP Basic Auth *password* (any username). Enables auth. |
| `--tls-cert` / `FOND_TLS_CERT` | PEM certificate chain for native HTTPS (requires `--tls-key`). |
| `--tls-key` / `FOND_TLS_KEY` | PEM private key for native HTTPS (requires `--tls-cert`). |
| `--insecure-allow-no-auth` | Bind non-loopback with **no** auth. Unsafe — exposes everything. |

For a full recommended deployment (VPN + reverse proxy + token) and the threat
model, see [Self-hosting fond securely](./self-hosting.md).
