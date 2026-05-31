# Pantry & Grocery Lists

fond tracks what you have on hand and generates grocery lists for recipes.

## Pantry

The pantry is a simple presence-based tracker — you mark what you have, and fond tells you how much of a recipe you can cover.

### Adding Items

```bash
fond pantry add "soy sauce" "vinegar" "garlic" "rice" "chicken thighs"
```

Items are matched using fuzzy logic:

- Case-insensitive: "Soy Sauce" matches "soy sauce"
- Plurals handled: "eggs" matches "egg"
- Common forms: "garlic cloves" matches "garlic"

### Removing Items

```bash
fond pantry rm "rice" "chicken thighs"
```

### Listing Your Pantry

```bash
fond pantry list         # active items only
fond pantry list --all   # include removed items
```

### Checking Recipe Coverage

See how much of a recipe you can already make:

```bash
fond pantry check chicken-adobo
```

Output shows a coverage percentage and which ingredients are matched vs. missing:

```text
Coverage: 60% (3/5 ingredients)

✓ soy sauce
✓ vinegar
✓ garlic
✗ chicken thighs
✗ bay leaves
```

### How Matching Works

Pantry matching is bidirectional — a pantry item "garlic" matches recipe ingredient "garlic cloves", and a pantry item "garlic cloves" also matches "garlic". This gives you the most generous coverage estimate.

## Grocery Lists

Generate a shopping list from a recipe, automatically subtracting what you already have in your pantry.

### Basic Usage

```bash
fond grocery from-recipe chicken-adobo
```

This shows only the items you **need to buy** — pantry items are automatically excluded.

### Including Pantry Items

To see the full ingredient list with pantry coverage noted:

```bash
fond grocery from-recipe chicken-adobo --include-pantry
```

### Category Grouping

Grocery items are automatically grouped by store aisle/category:

- **Produce** — fruits, vegetables, herbs
- **Meat & Seafood** — chicken, beef, fish, etc.
- **Dairy & Eggs** — milk, cheese, butter, eggs
- **Spices & Seasonings** — salt, pepper, cumin, etc.
- **Oils & Vinegars** — olive oil, sesame oil, vinegar
- **Condiments & Sauces** — soy sauce, fish sauce, ketchup
- **Canned & Jarred** — tomato paste, coconut milk, broth
- **Grains & Pasta** — rice, pasta, flour, bread
- **Baking** — sugar, baking powder, vanilla
- **Other** — items that don't fit other categories

### Grocery vs. Pantry Matching

Grocery list matching is **stricter** than pantry coverage checking — the pantry item must be a subset of the ingredient name (unidirectional). This avoids false exclusions where, for example, having "pepper" in your pantry would incorrectly exclude "bell pepper" from your grocery list.
