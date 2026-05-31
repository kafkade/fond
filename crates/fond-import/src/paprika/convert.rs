use chrono::Utc;
use fond_domain::{Recipe, RecipeIngredient, Step, slugify};

use super::types::PaprikaRecipe;
use crate::{ImportReport, ImportResult, PreparedRecipe};

/// Convert a batch of parsed Paprika recipes into prepared recipes
/// ready to be written to disk.
///
/// Handles duplicate detection (by slug collision within the batch and
/// against `existing_slugs`), conversion, and `.cook` text generation.
///
/// If `dry_run` is true, the same conversion and validation happens but
/// the returned `PreparedRecipe`s are intended for reporting only.
pub fn convert_paprika_batch(
    paprika_recipes: Vec<PaprikaRecipe>,
    existing_slugs: &[String],
    existing_source_urls: &[String],
) -> (Vec<PreparedRecipe>, ImportReport) {
    let mut report = ImportReport::new();
    let mut prepared = Vec::new();
    let mut used_slugs: Vec<String> = existing_slugs.to_vec();

    for pr in paprika_recipes {
        let title = pr.name.clone();

        // Duplicate detection by source_url
        if let Some(ref url) = pr.source_url {
            let normalized = url.trim().to_lowercase();
            if !normalized.is_empty() && existing_source_urls.contains(&normalized) {
                report.add(ImportResult::Skipped {
                    title,
                    reason: format!("duplicate source URL: {url}"),
                });
                continue;
            }
        }

        // Convert to domain Recipe
        let recipe = paprika_to_recipe(&pr);

        // Resolve slug collision with suffix
        let base_slug = recipe.slug.clone();
        let final_slug = resolve_slug_collision(&base_slug, &used_slugs);
        let file_name = format!("{final_slug}.cook");

        let recipe = Recipe {
            slug: final_slug.clone(),
            ..recipe
        };

        // Generate .cook text
        let cook_text = emit_paprika_cook(&recipe, &pr);

        // Set raw_source to the generated text so future writes preserve it
        let recipe = Recipe {
            raw_source: Some(cook_text.clone()),
            ..recipe
        };

        used_slugs.push(final_slug.clone());

        report.add(ImportResult::Imported {
            title: recipe.title.clone(),
            slug: final_slug,
            file_name: file_name.clone(),
        });

        prepared.push(PreparedRecipe {
            recipe,
            cook_text,
            file_name,
        });
    }

    (prepared, report)
}

/// Convert a single Paprika recipe to a fond domain `Recipe`.
fn paprika_to_recipe(pr: &PaprikaRecipe) -> Recipe {
    let title = pr.name.clone();
    let slug = slugify(&title);

    let ingredients = parse_ingredient_lines(pr.ingredients.as_deref().unwrap_or(""));

    let steps = parse_direction_lines(pr.directions.as_deref().unwrap_or(""));

    let tags: Vec<String> = pr
        .categories
        .as_ref()
        .map(|cats| {
            cats.iter()
                .map(|c| c.trim().to_lowercase())
                .filter(|c| !c.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Use `yield` as fallback for `servings`
    let servings = pr
        .servings
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .or_else(|| {
            pr.recipe_yield
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .cloned()
        });

    // Combine description and notes
    let description = build_description(pr.description.as_deref(), pr.notes.as_deref());

    let now = Utc::now();

    Recipe {
        slug,
        title,
        source: pr.source.clone().filter(|s| !s.trim().is_empty()),
        source_url: pr.source_url.clone().filter(|s| !s.trim().is_empty()),
        description,
        recipe_yield: pr.recipe_yield.clone().filter(|s| !s.trim().is_empty()),
        prep_time: pr.prep_time.clone().filter(|s| !s.trim().is_empty()),
        cook_time: pr.cook_time.clone().filter(|s| !s.trim().is_empty()),
        total_time: pr.total_time.clone().filter(|s| !s.trim().is_empty()),
        servings,
        ingredients,
        steps,
        cookware: Vec::new(),
        tags,
        created_at: now,
        updated_at: now,
        raw_source: None,
    }
}

/// Parse Paprika's newline-delimited ingredient text into structured ingredients.
///
/// Best-effort quantity/unit extraction. Lines that don't match the expected
/// pattern become ingredients with just a name (no quantity/unit).
///
/// Section headers (lines ending with `:` like "For the Sauce:") are preserved
/// but marked with the section name in the note field.
fn parse_ingredient_lines(text: &str) -> Vec<RecipeIngredient> {
    let mut ingredients = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Detect section headers: lines like "For the Sauce:" or "Dressing:"
        if is_section_header(trimmed) {
            continue;
        }

        ingredients.push(parse_single_ingredient(trimmed));
    }

    ingredients
}

/// Attempt to parse a single ingredient line into quantity, unit, and name.
///
/// Supports patterns like:
/// - "2 lbs chicken thighs"
/// - "1/2 cup soy sauce"
/// - "6 cloves garlic, crushed"
/// - "salt and pepper to taste"
///
/// This parser is shared by both the Paprika and schema.org importers.
pub fn parse_single_ingredient(line: &str) -> RecipeIngredient {
    // Try to extract a leading quantity
    let (qty_str, rest) = split_quantity(line);

    if qty_str.is_empty() {
        return RecipeIngredient {
            name: line.to_string(),
            quantity: None,
            unit: None,
            note: None,
            optional: false,
        };
    }

    // Try to extract a unit from the remaining text
    let rest = rest.trim_start();
    let (unit, name_part) = split_unit(rest);

    let name = name_part.trim().to_string();
    if name.is_empty() {
        // Only had quantity, no name — treat whole line as name
        return RecipeIngredient {
            name: line.to_string(),
            quantity: None,
            unit: None,
            note: None,
            optional: false,
        };
    }

    RecipeIngredient {
        name,
        quantity: Some(qty_str.to_string()),
        unit: if unit.is_empty() { None } else { Some(unit) },
        note: None,
        optional: false,
    }
}

/// Split a leading numeric quantity from ingredient text.
///
/// Handles integers, decimals, fractions (1/2), mixed (1 1/2), and Unicode
/// fractions (½, ¼, ¾).
fn split_quantity(s: &str) -> (&str, &str) {
    let bytes = s.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_digit() || b == b'.' || b == b'/' {
            i += 1;
        } else if b == b' ' {
            // Allow space in mixed fractions like "1 1/2"
            // Check if the next part is a fraction
            let rest = &s[i + 1..];
            if rest.starts_with(|c: char| c.is_ascii_digit()) && rest.contains('/') {
                // Find end of the fraction part
                let frac_end = rest
                    .find(|c: char| !c.is_ascii_digit() && c != '/')
                    .unwrap_or(rest.len());
                let frac = &rest[..frac_end];
                if frac.contains('/') {
                    i += 1 + frac_end;
                    continue;
                }
            }
            break;
        } else {
            // Check for Unicode fractions
            let remaining = &s[i..];
            if remaining.starts_with('½')
                || remaining.starts_with('¼')
                || remaining.starts_with('¾')
                || remaining.starts_with('⅓')
                || remaining.starts_with('⅔')
                || remaining.starts_with('⅛')
            {
                // Unicode fractions are multi-byte
                let c = remaining.chars().next().unwrap();
                i += c.len_utf8();
            } else {
                break;
            }
        }
    }

    if i == 0 {
        // Check if it starts with a Unicode fraction
        let c = s.chars().next();
        if let Some(c) = c
            && "½¼¾⅓⅔⅛".contains(c)
        {
            let len = c.len_utf8();
            return (&s[..len], &s[len..]);
        }
        ("", s)
    } else {
        (&s[..i], &s[i..])
    }
}

/// Known cooking units for ingredient parsing.
const KNOWN_UNITS: &[&str] = &[
    "tsp",
    "teaspoon",
    "teaspoons",
    "tbsp",
    "tablespoon",
    "tablespoons",
    "cup",
    "cups",
    "oz",
    "ounce",
    "ounces",
    "lb",
    "lbs",
    "pound",
    "pounds",
    "g",
    "gram",
    "grams",
    "kg",
    "kilogram",
    "kilograms",
    "ml",
    "milliliter",
    "milliliters",
    "l",
    "liter",
    "liters",
    "clove",
    "cloves",
    "can",
    "cans",
    "bunch",
    "bunches",
    "pinch",
    "dash",
    "package",
    "packages",
    "pkg",
    "large",
    "medium",
    "small",
    "whole",
    "slice",
    "slices",
    "piece",
    "pieces",
    "head",
    "heads",
    "sprig",
    "sprigs",
    "stalk",
    "stalks",
    "stick",
    "sticks",
];

/// Split a leading unit from ingredient name text.
fn split_unit(s: &str) -> (String, &str) {
    // Find the first word
    let first_word_end = s.find(|c: char| c.is_ascii_whitespace()).unwrap_or(s.len());
    let first_word = &s[..first_word_end];

    // Strip trailing punctuation for matching
    let cleaned = first_word.trim_end_matches(',').trim_end_matches('.');

    if KNOWN_UNITS.contains(&cleaned.to_lowercase().as_str()) {
        (cleaned.to_string(), &s[first_word_end..])
    } else {
        (String::new(), s)
    }
}

/// Check if a line is an ingredient section header.
fn is_section_header(line: &str) -> bool {
    // "For the Sauce:", "Dressing:", etc.
    line.ends_with(':') && !line.contains('@') && line.len() > 1 && line.len() < 80
}

/// Parse Paprika's newline-delimited directions into Steps.
fn parse_direction_lines(text: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut order = 0u32;
    let mut current_section: Option<String> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Detect section headers in directions
        if is_section_header(trimmed) {
            current_section = Some(trimmed.trim_end_matches(':').to_string());
            continue;
        }

        steps.push(Step {
            section: current_section.clone(),
            body: trimmed.to_string(),
            timers: Vec::new(),
            order,
        });
        order += 1;
    }

    steps
}

/// Build description from Paprika's description and notes fields.
fn build_description(description: Option<&str>, notes: Option<&str>) -> Option<String> {
    let desc = description.filter(|s| !s.trim().is_empty());
    let notes = notes.filter(|s| !s.trim().is_empty());

    match (desc, notes) {
        (Some(d), None) => Some(d.to_string()),
        (None, _) => None,
        (Some(d), Some(_)) => Some(d.to_string()),
    }
}

/// Generate `.cook` file text from a converted Recipe and its Paprika source.
///
/// This is a Paprika-specific emitter that ensures all ingredients are
/// explicitly listed in the output. The generic `emit_cook()` only inlines
/// ingredients found in step text, which would drop ingredients not
/// mentioned in directions.
fn emit_paprika_cook(recipe: &Recipe, paprika: &PaprikaRecipe) -> String {
    let mut out = String::new();

    // --- Frontmatter ---
    out.push_str("---\n");
    out.push_str(&format!("title: {}\n", recipe.title));

    if let Some(ref s) = recipe.source {
        out.push_str(&format!("source: {s}\n"));
    }
    if let Some(ref s) = recipe.source_url {
        out.push_str(&format!("source url: {s}\n"));
    }
    if let Some(ref s) = recipe.servings {
        out.push_str(&format!("servings: {s}\n"));
    }
    if let Some(ref s) = recipe.recipe_yield {
        out.push_str(&format!("yield: {s}\n"));
    }
    if let Some(ref s) = recipe.prep_time {
        out.push_str(&format!("prep time: {s}\n"));
    }
    if let Some(ref s) = recipe.cook_time {
        out.push_str(&format!("cook time: {s}\n"));
    }
    if let Some(ref s) = recipe.total_time {
        out.push_str(&format!("total time: {s}\n"));
    }
    if let Some(ref s) = recipe.description {
        out.push_str(&format!("description: {s}\n"));
    }
    if !recipe.tags.is_empty() {
        out.push_str(&format!("tags: {}\n", recipe.tags.join(", ")));
    }

    // Import provenance
    out.push_str("import source: paprika\n");
    if let Some(ref uid) = paprika.uid {
        out.push_str(&format!("paprika uid: {uid}\n"));
    }
    if let Some(ref hash) = paprika.hash {
        out.push_str(&format!("paprika hash: {hash}\n"));
    }
    if let Some(ref d) = paprika.difficulty
        && !d.trim().is_empty()
    {
        out.push_str(&format!("difficulty: {d}\n"));
    }
    out.push_str("---\n\n");

    // --- Ingredients ---
    let ingredient_text = paprika.ingredients.as_deref().unwrap_or("");
    if !ingredient_text.trim().is_empty() {
        let mut in_section = false;
        for line in ingredient_text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if in_section {
                    out.push('\n');
                }
                continue;
            }

            if is_section_header(trimmed) {
                let section_name = trimmed.trim_end_matches(':');
                out.push_str(&format!("== {section_name} ==\n\n"));
                in_section = true;
                continue;
            }

            // Emit ingredient with Cooklang annotation
            let ing = parse_single_ingredient(trimmed);
            out.push_str(&format_ingredient_line(&ing));
            out.push('\n');
        }
        out.push('\n');
    }

    // --- Directions ---
    let directions_text = paprika.directions.as_deref().unwrap_or("");
    if !directions_text.trim().is_empty() {
        for line in directions_text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                out.push('\n');
                continue;
            }

            if is_section_header(trimmed) {
                let section_name = trimmed.trim_end_matches(':');
                out.push_str(&format!("== {section_name} ==\n\n"));
                continue;
            }

            out.push_str(trimmed);
            out.push_str("\n\n");
        }
    }

    // --- Notes ---
    let notes_text = paprika.notes.as_deref().unwrap_or("");
    if !notes_text.trim().is_empty() {
        out.push_str("-- Notes --\n\n");
        for line in notes_text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                out.push('\n');
            } else {
                out.push_str(&format!("-- {trimmed}\n"));
            }
        }
    }

    // Trim trailing whitespace and ensure single trailing newline
    let trimmed = out.trim_end();
    format!("{trimmed}\n")
}

/// Format a single ingredient line in Cooklang notation.
fn format_ingredient_line(ing: &RecipeIngredient) -> String {
    match (&ing.quantity, &ing.unit) {
        (Some(qty), Some(unit)) => format!("@{}{{{}%{}}}", ing.name, qty, unit),
        (Some(qty), None) => format!("@{}{{{}}}", ing.name, qty),
        _ => format!("@{}{{}}", ing.name),
    }
}

/// Resolve a slug collision by appending a numeric suffix.
fn resolve_slug_collision(slug: &str, existing: &[String]) -> String {
    if !existing.contains(&slug.to_string()) {
        return slug.to_string();
    }

    let mut suffix = 2;
    loop {
        let candidate = format!("{slug}-{suffix}");
        if !existing.contains(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Ingredient parsing ---

    #[test]
    fn parse_simple_ingredient() {
        let ing = parse_single_ingredient("2 lbs chicken thighs");
        assert_eq!(ing.quantity.as_deref(), Some("2"));
        assert_eq!(ing.unit.as_deref(), Some("lbs"));
        assert_eq!(ing.name, "chicken thighs");
    }

    #[test]
    fn parse_fraction_ingredient() {
        let ing = parse_single_ingredient("1/2 cup soy sauce");
        assert_eq!(ing.quantity.as_deref(), Some("1/2"));
        assert_eq!(ing.unit.as_deref(), Some("cup"));
        assert_eq!(ing.name, "soy sauce");
    }

    #[test]
    fn parse_mixed_fraction_ingredient() {
        let ing = parse_single_ingredient("1 1/2 cups flour");
        assert_eq!(ing.quantity.as_deref(), Some("1 1/2"));
        assert_eq!(ing.unit.as_deref(), Some("cups"));
        assert_eq!(ing.name, "flour");
    }

    #[test]
    fn parse_no_unit_ingredient() {
        let ing = parse_single_ingredient("6 cloves garlic, crushed");
        assert_eq!(ing.quantity.as_deref(), Some("6"));
        assert_eq!(ing.unit.as_deref(), Some("cloves"));
        assert_eq!(ing.name, "garlic, crushed");
    }

    #[test]
    fn parse_bare_name_ingredient() {
        let ing = parse_single_ingredient("salt and pepper to taste");
        assert_eq!(ing.quantity, None);
        assert_eq!(ing.unit, None);
        assert_eq!(ing.name, "salt and pepper to taste");
    }

    #[test]
    fn parse_unicode_fraction() {
        let ing = parse_single_ingredient("½ cup sugar");
        assert_eq!(ing.quantity.as_deref(), Some("½"));
        assert_eq!(ing.unit.as_deref(), Some("cup"));
        assert_eq!(ing.name, "sugar");
    }

    // --- Section header detection ---

    #[test]
    fn detect_section_headers() {
        assert!(is_section_header("For the Sauce:"));
        assert!(is_section_header("Dressing:"));
        assert!(!is_section_header("2 cups flour"));
        assert!(!is_section_header(""));
        assert!(!is_section_header(":"));
    }

    // --- Direction parsing ---

    #[test]
    fn parse_basic_directions() {
        let steps = parse_direction_lines("Step 1.\nStep 2.\nStep 3.");
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].body, "Step 1.");
        assert_eq!(steps[0].order, 0);
        assert_eq!(steps[2].body, "Step 3.");
        assert_eq!(steps[2].order, 2);
    }

    #[test]
    fn parse_directions_with_sections() {
        let text = "Prepare:\nMix ingredients.\nCook:\nBake at 350.";
        let steps = parse_direction_lines(text);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].section.as_deref(), Some("Prepare"));
        assert_eq!(steps[0].body, "Mix ingredients.");
        assert_eq!(steps[1].section.as_deref(), Some("Cook"));
        assert_eq!(steps[1].body, "Bake at 350.");
    }

    // --- Slug collision ---

    #[test]
    fn resolve_no_collision() {
        let result = resolve_slug_collision("chicken-adobo", &["pasta-carbonara".into()]);
        assert_eq!(result, "chicken-adobo");
    }

    #[test]
    fn resolve_with_collision() {
        let result = resolve_slug_collision("chicken-adobo", &["chicken-adobo".into()]);
        assert_eq!(result, "chicken-adobo-2");
    }

    #[test]
    fn resolve_multiple_collisions() {
        let existing = vec!["chicken-adobo".to_string(), "chicken-adobo-2".to_string()];
        let result = resolve_slug_collision("chicken-adobo", &existing);
        assert_eq!(result, "chicken-adobo-3");
    }

    // --- Full conversion ---

    #[test]
    fn convert_full_paprika_recipe() {
        let pr = PaprikaRecipe {
            name: "Classic Chicken Adobo".into(),
            uid: Some("A1B2C3D4".into()),
            description: Some("A tangy Filipino dish".into()),
            ingredients: Some("2 lbs chicken thighs\n1/2 cup soy sauce\n6 cloves garlic".into()),
            directions: Some("Combine chicken and sauce.\nSimmer for 35 minutes.".into()),
            notes: Some("Great with rice.".into()),
            servings: Some("4".into()),
            prep_time: Some("15 min".into()),
            cook_time: Some("50 min".into()),
            total_time: Some("1 hr 5 min".into()),
            source: Some("Lola's Kitchen".into()),
            source_url: Some("https://example.com/adobo".into()),
            categories: Some(vec!["Filipino".into(), "Chicken".into()]),
            rating: Some(5),
            difficulty: Some("Easy".into()),
            recipe_yield: Some("4 servings".into()),
            on_favorites: Some(true),
            created: Some("2024-03-15".into()),
            hash: Some("abc123".into()),
            nutrition: None,
            image_url: None,
            photo: None,
            photo_url: None,
            photo_hash: None,
            scale: None,
            extra: serde_json::Map::new(),
        };

        let recipe = paprika_to_recipe(&pr);

        assert_eq!(recipe.title, "Classic Chicken Adobo");
        assert_eq!(recipe.slug, "classic-chicken-adobo");
        assert_eq!(recipe.source.as_deref(), Some("Lola's Kitchen"));
        assert_eq!(
            recipe.source_url.as_deref(),
            Some("https://example.com/adobo")
        );
        assert_eq!(recipe.servings.as_deref(), Some("4"));
        assert_eq!(recipe.prep_time.as_deref(), Some("15 min"));
        assert_eq!(recipe.tags, vec!["filipino", "chicken"]);
        assert_eq!(recipe.ingredients.len(), 3);
        assert_eq!(recipe.steps.len(), 2);
        assert_eq!(recipe.description.as_deref(), Some("A tangy Filipino dish"));
    }

    #[test]
    fn convert_minimal_paprika_recipe() {
        let pr = PaprikaRecipe {
            name: "Quick Eggs".into(),
            uid: None,
            description: None,
            ingredients: None,
            directions: None,
            notes: None,
            servings: None,
            prep_time: None,
            cook_time: None,
            total_time: None,
            source: None,
            source_url: None,
            categories: None,
            rating: None,
            difficulty: None,
            recipe_yield: None,
            on_favorites: None,
            created: None,
            hash: None,
            nutrition: None,
            image_url: None,
            photo: None,
            photo_url: None,
            photo_hash: None,
            scale: None,
            extra: serde_json::Map::new(),
        };

        let recipe = paprika_to_recipe(&pr);

        assert_eq!(recipe.title, "Quick Eggs");
        assert_eq!(recipe.slug, "quick-eggs");
        assert!(recipe.ingredients.is_empty());
        assert!(recipe.steps.is_empty());
        assert!(recipe.tags.is_empty());
    }

    // --- .cook text generation ---

    #[test]
    fn emit_cook_contains_all_ingredients() {
        let pr = PaprikaRecipe {
            name: "Test Recipe".into(),
            uid: Some("TEST-123".into()),
            ingredients: Some("2 cups flour\n1 tsp salt\nwater".into()),
            directions: Some("Mix and bake.".into()),
            ..minimal_paprika()
        };

        let recipe = paprika_to_recipe(&pr);
        let cook_text = emit_paprika_cook(&recipe, &pr);

        assert!(
            cook_text.contains("@flour{2%cups}"),
            "should contain flour: {cook_text}"
        );
        assert!(
            cook_text.contains("@salt{1%tsp}"),
            "should contain salt: {cook_text}"
        );
        assert!(
            cook_text.contains("@water{}"),
            "should contain water: {cook_text}"
        );
        assert!(cook_text.contains("Mix and bake."), "should contain step");
    }

    #[test]
    fn emit_cook_contains_provenance() {
        let pr = PaprikaRecipe {
            name: "Provenance Test".into(),
            uid: Some("UID-456".into()),
            hash: Some("hash789".into()),
            ..minimal_paprika()
        };

        let recipe = paprika_to_recipe(&pr);
        let cook_text = emit_paprika_cook(&recipe, &pr);

        assert!(cook_text.contains("import source: paprika"));
        assert!(cook_text.contains("paprika uid: UID-456"));
        assert!(cook_text.contains("paprika hash: hash789"));
    }

    #[test]
    fn emit_cook_contains_notes() {
        let pr = PaprikaRecipe {
            name: "Notes Test".into(),
            notes: Some("This is a note.\nAnother note.".into()),
            ..minimal_paprika()
        };

        let recipe = paprika_to_recipe(&pr);
        let cook_text = emit_paprika_cook(&recipe, &pr);

        assert!(cook_text.contains("-- Notes --"));
        assert!(cook_text.contains("-- This is a note."));
        assert!(cook_text.contains("-- Another note."));
    }

    #[test]
    fn parse_after_emit_preserves_title_and_tags() {
        let pr = PaprikaRecipe {
            name: "Round-Trip Test".into(),
            uid: Some("RT-001".into()),
            categories: Some(vec!["Italian".into(), "Pasta".into()]),
            ingredients: Some("1 lb spaghetti\n2 cups marinara sauce".into()),
            directions: Some("Cook pasta.\nAdd sauce.".into()),
            source: Some("Nonna's Book".into()),
            source_url: Some("https://example.com/pasta".into()),
            servings: Some("4".into()),
            ..minimal_paprika()
        };

        let recipe = paprika_to_recipe(&pr);
        let cook_text = emit_paprika_cook(&recipe, &pr);

        // Parse the generated .cook text back
        let parsed = fond_domain::parse_cook(&cook_text, "round-trip-test").unwrap();

        assert_eq!(parsed.title, "Round-Trip Test");
        assert_eq!(parsed.source.as_deref(), Some("Nonna's Book"));
        assert_eq!(
            parsed.source_url.as_deref(),
            Some("https://example.com/pasta")
        );
        assert_eq!(parsed.servings.as_deref(), Some("4"));
        // Tags should survive round-trip
        assert!(parsed.tags.contains(&"italian".to_string()));
        assert!(parsed.tags.contains(&"pasta".to_string()));
    }

    // --- Batch conversion ---

    #[test]
    fn batch_conversion_deduplicates_by_source_url() {
        let recipes = vec![
            PaprikaRecipe {
                name: "Recipe A".into(),
                source_url: Some("https://example.com/a".into()),
                ..minimal_paprika()
            },
            PaprikaRecipe {
                name: "Recipe B".into(),
                source_url: Some("https://example.com/b".into()),
                ..minimal_paprika()
            },
        ];

        let existing_urls = vec!["https://example.com/a".to_string()];
        let (prepared, report) = convert_paprika_batch(recipes, &[], &existing_urls);

        assert_eq!(report.imported, 1);
        assert_eq!(report.skipped, 1);
        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].recipe.title, "Recipe B");
    }

    #[test]
    fn batch_conversion_resolves_slug_collisions() {
        let recipes = vec![
            PaprikaRecipe {
                name: "Chicken Adobo".into(),
                ..minimal_paprika()
            },
            PaprikaRecipe {
                name: "Chicken Adobo".into(),
                source_url: Some("https://other.com/adobo".into()),
                ..minimal_paprika()
            },
        ];

        let (prepared, report) = convert_paprika_batch(recipes, &[], &[]);

        assert_eq!(report.imported, 2);
        assert_eq!(prepared[0].file_name, "chicken-adobo.cook");
        assert_eq!(prepared[1].file_name, "chicken-adobo-2.cook");
    }

    /// Helper to create a minimal PaprikaRecipe for tests.
    fn minimal_paprika() -> PaprikaRecipe {
        PaprikaRecipe {
            name: String::new(),
            uid: None,
            description: None,
            ingredients: None,
            directions: None,
            notes: None,
            servings: None,
            prep_time: None,
            cook_time: None,
            total_time: None,
            source: None,
            source_url: None,
            categories: None,
            rating: None,
            difficulty: None,
            recipe_yield: None,
            on_favorites: None,
            created: None,
            hash: None,
            nutrition: None,
            image_url: None,
            photo: None,
            photo_url: None,
            photo_hash: None,
            scale: None,
            extra: serde_json::Map::new(),
        }
    }
}
