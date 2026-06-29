# Exporting Recipes

fond can export your recipe collection in two formats: JSON and Paprika.

## JSON Export

The JSON export produces a structured envelope containing your recipes with full metadata.

### Full Collection

```bash
# To stdout
fond export

# To a file
fond export --output recipes.json
```

### Single Recipe

```bash
fond export --recipe chicken-adobo
fond export --recipe chicken-adobo --output chicken.json
```

### JSON Format

The export envelope structure:

```json
{
  "schema_version": 1,
  "fond_version": "1.0.0",
  "exported_at": "2025-01-15T12:00:00Z",
  "recipe_count": 42,
  "recipes": [
    {
      "slug": "chicken-adobo",
      "title": "Chicken Adobo",
      "description": "A Filipino braised chicken dish",
      "source": "Serious Eats",
      "source_url": "https://...",
      "servings": "4",
      "prep_time": "15 minutes",
      "cook_time": "45 minutes",
      "ingredients": [
        { "name": "chicken thighs", "quantity": "2", "unit": "lbs" },
        { "name": "soy sauce", "quantity": "1/2", "unit": "cup" }
      ],
      "steps": [
        { "body": "Combine soy sauce and vinegar...", "order": 0 }
      ],
      "tags": ["filipino", "braised"],
      "created_at": "2025-01-10T08:00:00Z",
      "updated_at": "2025-01-10T08:00:00Z"
    }
  ]
}
```

### What's Included

- Recipe title, slug, and description
- All ingredients with quantities and units
- All steps in order
- Tags
- Source attribution (source name and URL)
- Timing metadata (prep, cook, total time)
- Timestamps (created, updated)

### Current Limitations

- Ratings, notes, and cook logs are not yet part of the export (tables not yet in schema)
- Photos are not included in the JSON export

## Paprika Export

Export to Paprika format for sharing with Paprika users or as a backup.

### Full Collection (`.paprikarecipes`)

```bash
fond export --export-format paprika --output recipes.paprikarecipes
```

This creates a ZIP archive where each recipe is a gzip-compressed JSON file — the same format Paprika uses.

### Single Recipe (`.paprikarecipe`)

```bash
fond export --export-format paprika --recipe chicken-adobo --output chicken.paprikarecipe
```

### Round-Trip Compatibility

Paprika exports are designed to be importable back into fond (or Paprika itself):

```bash
# Export
fond export --export-format paprika --output backup.paprikarecipes

# Re-import (on another machine or after a reset)
fond import paprika backup.paprikarecipes
```

### Field Mapping

| fond Field | Paprika Field |
|-----------|---------------|
| Title | name |
| Ingredients | ingredients (one per line) |
| Steps | directions |
| Description | description |
| Tags | categories |
| Prep/Cook/Total Time | prep_time / cook_time / total_time |
| Servings | servings |
| Source / Source URL | source / source_url |
