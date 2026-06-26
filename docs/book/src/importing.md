# Importing Recipes

fond can import recipes from three sources: Paprika archives, websites with schema.org markup, and local photo/scanned images.

## From Paprika

Paprika is a popular recipe manager. fond can import `.paprikarecipes` archives (multiple recipes) or `.paprikarecipe` files (single recipe).

```bash
fond import paprika ~/Downloads/recipes.paprikarecipes
```

### What Gets Imported

| Paprika Field | fond Mapping |
|---------------|-------------|
| Name | Recipe title |
| Ingredients | Parsed into structured ingredients (quantity, unit, name) |
| Directions | Recipe steps |
| Description | Recipe description metadata |
| Prep/Cook/Total Time | Time metadata |
| Servings | Servings metadata |
| Source / Source URL | Source attribution |
| Categories | Tags |

### Import Behavior

- **Idempotent**: Re-importing the same archive skips already-imported recipes (matched by title and source URL).
- **Non-destructive**: User edits to previously imported recipes are never overwritten.
- **Lossless**: Original Paprika data is preserved in import provenance metadata.
- **Best-effort parsing**: Ingredient lines that can't be parsed into quantity/unit/name are preserved as-is.

### Dry Run

Preview what would be imported without writing anything:

```bash
fond import paprika recipes.paprikarecipes --dry-run
```

## From URLs

fond can import recipes from any website that includes [schema.org Recipe](https://schema.org/Recipe) structured data (JSON-LD or microdata).

```bash
fond import url https://www.seriouseats.com/chicken-adobo-recipe
```

### Supported Sites

Any website with schema.org Recipe markup works, including:

- Serious Eats
- NYT Cooking (public recipes only — see [limitations](#subscription-sites) below)
- Bon Appétit
- Food Network
- AllRecipes
- Most food blogs using WordPress recipe plugins

### Extraction Process

1. Fetch the page HTML
2. Extract JSON-LD `@type: Recipe` data (primary method)
3. Fall back to microdata/RDFa attributes if no JSON-LD found
4. Parse ISO 8601 durations (e.g., `PT30M` → `30 minutes`)
5. Convert to `.cook` format and save

### Dry Run

```bash
fond import url https://example.com/recipe --dry-run
```

## From Photos or Scans

fond can OCR a local recipe photo or scanned page into an editable Cooklang draft.

```bash
fond import photo ~/Downloads/grandma-card.jpg
fond import photo ~/Downloads/printed-recipe.png --dry-run
```

### Behavior

- **Printed-first**: clean printed cards/pages are the primary target.
- **Handwriting is best-effort**: handwritten cards are supported, but usually need more cleanup.
- **Never auto-saves**: OCR imports always land in the review queue first.
- **Offline-first**: OCR runs locally through a Tesseract-compatible binary.

### Review Queue

After OCR import, inspect and finalize the draft before writing a canonical `.cook` file:

```bash
fond review list
fond review show <review-id>
fond review edit <review-id>
fond review accept <review-id>
```

Reject a bad draft without importing it:

```bash
fond review reject <review-id>
```

### OCR Requirements

fond expects a local `tesseract` binary on your `PATH`. If it lives elsewhere, point fond at it with `FOND_TESSERACT_BIN=/path/to/tesseract`.

## Tips

- Use `fond list` after importing to verify your recipes
- Use `fond view <slug>` to check the conversion quality
- Use `fond review edit <id>` to clean up OCR drafts before accepting them
- Imported recipes are regular `.cook` files — edit them freely
- Run `fond reindex` if you ever need to rebuild the database

## Subscription Sites

### NYT Cooking & Cook's Illustrated / ATK

fond **does not** provide authenticated importers for NYT Cooking or America's Test Kitchen (Cook's Illustrated, Cook's Country). Both services explicitly prohibit automated access and scraping in their Terms of Service.

**What works:**

- Public (non-paywalled) recipe pages with schema.org markup can be imported via `fond import url`
- If you use Paprika, you can save NYT/ATK recipes using Paprika's built-in browser clipper, then import them via `fond import paprika`

**What fond will not do:**

- Automate login to subscription services
- Circumvent paywalls or access controls
- Bulk-download recipes from these services

This is a deliberate architectural decision, not a technical limitation. See [ADR-006](https://github.com/kafkade/fond/blob/main/docs/adr/006-web-scraping.md) and the [due-diligence review](https://github.com/kafkade/fond/blob/main/docs/due-diligence/nyt-atk-scraping-review.md) for details.

### The Paprika Bridge

If you have a Paprika subscription and have saved recipes from NYT Cooking or Cook's Illustrated using Paprika's clipper, you can import those recipes into fond:

1. Export your Paprika recipes (Settings → Export)
2. Import the archive: `fond import paprika ~/Downloads/recipes.paprikarecipes`

This is the recommended path for getting subscription recipes into fond. You already had legitimate access to save those recipes in Paprika, and fond imports the local copy you own.
