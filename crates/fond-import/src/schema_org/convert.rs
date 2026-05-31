use chrono::Utc;
use fond_domain::{Recipe, RecipeIngredient, Step, slugify};

use super::extract::{extract_author, extract_steps, normalize_url};
use super::types::{ExtractionConfidence, SchemaRecipe};
use crate::paprika::parse_single_ingredient;
use crate::{ImportReport, ImportResult, PreparedRecipe};

/// Import a single recipe from HTML content extracted from a URL.
///
/// Tries JSON-LD extraction first, then falls back to HTML scraping.
/// The caller is responsible for fetching the HTML — this function
/// is I/O-free.
pub fn import_html(
    html: &str,
    source_url: &str,
    existing_slugs: &[String],
    existing_source_urls: &[String],
) -> (Vec<PreparedRecipe>, ImportReport) {
    let mut report = ImportReport::new();
    let mut prepared = Vec::new();
    let mut used_slugs: Vec<String> = existing_slugs.to_vec();

    // Try JSON-LD first
    let mut recipes = super::extract::extract_recipes_from_html(html);
    let confidence = if !recipes.is_empty() {
        for r in &mut recipes {
            r.source_url = Some(source_url.to_string());
        }
        ExtractionConfidence::Structured
    } else {
        // Fallback to HTML scraping
        match super::extract::extract_recipe_from_html_fallback(html) {
            Some(mut r) => {
                r.source_url = Some(source_url.to_string());
                recipes.push(r);
                ExtractionConfidence::Fallback
            }
            None => {
                report.add(ImportResult::Failed {
                    entry_name: source_url.to_string(),
                    error: "no recipe found on page (tried JSON-LD and HTML fallback)".to_string(),
                });
                return (prepared, report);
            }
        }
    };

    let normalized_source = normalize_url(source_url);

    for schema_recipe in recipes {
        let title = schema_recipe.name.clone();

        // Dedup by normalized source URL
        if !normalized_source.is_empty() {
            let existing_normalized: Vec<String> = existing_source_urls
                .iter()
                .map(|u| normalize_url(u))
                .collect();
            if existing_normalized.contains(&normalized_source) {
                report.add(ImportResult::Skipped {
                    title,
                    reason: format!("duplicate source URL: {source_url}"),
                });
                continue;
            }
        }

        let recipe = schema_to_recipe(&schema_recipe);
        let base_slug = recipe.slug.clone();
        let final_slug = resolve_slug_collision(&base_slug, &used_slugs);
        let file_name = format!("{final_slug}.cook");

        let recipe = Recipe {
            slug: final_slug.clone(),
            ..recipe
        };

        let cook_text = emit_schema_cook(&recipe, &schema_recipe, confidence);

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

/// Convert a SchemaRecipe to a fond domain Recipe.
fn schema_to_recipe(sr: &SchemaRecipe) -> Recipe {
    let title = sr.name.clone();
    let slug = slugify(&title);

    let source = sr
        .author
        .as_ref()
        .and_then(extract_author)
        .filter(|s| !s.trim().is_empty());

    let source_url = sr.source_url.clone();

    let description = sr
        .description
        .as_ref()
        .map(|d| sanitize_metadata_value(d))
        .filter(|d| !d.is_empty());

    let recipe_yield = extract_yield_string(&sr.recipe_yield);

    let prep_time = sr
        .prep_time
        .as_ref()
        .and_then(|t| parse_iso8601_duration(t));
    let cook_time = sr
        .cook_time
        .as_ref()
        .and_then(|t| parse_iso8601_duration(t));
    let total_time = sr
        .total_time
        .as_ref()
        .and_then(|t| parse_iso8601_duration(t));

    let servings = extract_yield_string(&sr.recipe_yield);

    let ingredients: Vec<RecipeIngredient> = sr
        .recipe_ingredient
        .as_ref()
        .map(|ings| {
            ings.iter()
                .map(|line| parse_single_ingredient(line.trim()))
                .collect()
        })
        .unwrap_or_default();

    let steps: Vec<Step> = sr
        .recipe_instructions
        .as_ref()
        .map(|inst| {
            extract_steps(inst)
                .into_iter()
                .enumerate()
                .map(|(i, body)| Step {
                    section: None,
                    body,
                    timers: Vec::new(),
                    order: i as u32,
                })
                .collect()
        })
        .unwrap_or_default();

    let mut tags = Vec::new();
    if let Some(ref kw) = sr.keywords {
        tags.extend(extract_string_list(kw));
    }
    if let Some(ref cuisine) = sr.recipe_cuisine {
        tags.extend(extract_string_list(cuisine));
    }
    if let Some(ref category) = sr.recipe_category {
        tags.extend(extract_string_list(category));
    }
    // Deduplicate tags (case-insensitive)
    let mut seen = std::collections::HashSet::new();
    tags.retain(|t| {
        let lower = t.to_lowercase();
        seen.insert(lower)
    });

    let now = Utc::now();

    Recipe {
        slug,
        title,
        source,
        source_url,
        description,
        recipe_yield,
        prep_time,
        cook_time,
        total_time,
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

/// Parse an ISO 8601 duration string into a human-readable string.
///
/// Handles: `PT15M`, `PT1H30M`, `PT4H`, `PT45S`, `P1DT2H`,
/// and combinations. Returns `None` for unrecognized formats.
pub fn parse_iso8601_duration(s: &str) -> Option<String> {
    let s = s.trim();
    if !s.starts_with('P') {
        return None;
    }

    let rest = &s[1..]; // strip P
    let (date_part, time_part) = match rest.find('T') {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, ""),
    };

    let mut parts = Vec::new();

    // Parse date part (days)
    if !date_part.is_empty()
        && let Some(days) = parse_duration_component(date_part, 'D')
        && days > 0
    {
        parts.push(format!("{days} day{}", if days == 1 { "" } else { "s" }));
    }

    // Parse time part
    if !time_part.is_empty() {
        if let Some(hours) = parse_duration_component(time_part, 'H')
            && hours > 0
        {
            parts.push(format!("{hours} hr"));
        }
        if let Some(minutes) = parse_duration_component(time_part, 'M')
            && minutes > 0
        {
            parts.push(format!("{minutes} min"));
        }
        if let Some(seconds) = parse_duration_component(time_part, 'S')
            && seconds > 0
        {
            parts.push(format!("{seconds} sec"));
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

/// Parse a single numeric component from an ISO 8601 duration segment.
fn parse_duration_component(s: &str, marker: char) -> Option<u32> {
    let marker_pos = s.find(marker)?;
    // Find the start of the number (walk backwards from marker)
    let num_start = s[..marker_pos]
        .rfind(|c: char| !c.is_ascii_digit())
        .map(|i| i + 1)
        .unwrap_or(0);
    s[num_start..marker_pos].parse().ok()
}

/// Extract a yield/servings string from polymorphic recipeYield.
fn extract_yield_string(value: &Option<serde_json::Value>) -> Option<String> {
    value.as_ref().and_then(|v| match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Array(arr) => {
            arr.first().and_then(|item| item.as_str().map(String::from))
        }
        _ => None,
    })
}

/// Extract a list of strings from a polymorphic value.
///
/// Handles: plain string (split on commas), array of strings, single value.
fn extract_string_list(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(s) => s
            .split(',')
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// Sanitize a metadata value for .cook frontmatter:
/// - Strip HTML tags
/// - Decode common HTML entities
/// - Collapse whitespace to single spaces
/// - Trim
fn sanitize_metadata_value(s: &str) -> String {
    // Strip HTML tags
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }

    // Decode common HTML entities
    let result = result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ");

    // Collapse whitespace and trim
    let mut prev_space = false;
    let collapsed: String = result
        .chars()
        .filter_map(|c| {
            if c.is_whitespace() {
                if prev_space {
                    None
                } else {
                    prev_space = true;
                    Some(' ')
                }
            } else {
                prev_space = false;
                Some(c)
            }
        })
        .collect();

    collapsed.trim().to_string()
}

/// Generate `.cook` file text from a converted Recipe and its schema.org source.
fn emit_schema_cook(
    recipe: &Recipe,
    _schema: &SchemaRecipe,
    confidence: ExtractionConfidence,
) -> String {
    let mut out = String::new();

    // --- Frontmatter ---
    out.push_str("---\n");
    out.push_str(&format!("title: {}\n", recipe.title));

    if let Some(ref s) = recipe.source {
        out.push_str(&format!("source: {}\n", sanitize_metadata_value(s)));
    }
    if let Some(ref s) = recipe.source_url {
        out.push_str(&format!("source url: {s}\n"));
    }
    if let Some(ref s) = recipe.servings {
        out.push_str(&format!("servings: {s}\n"));
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
        out.push_str(&format!("description: {}\n", sanitize_metadata_value(s)));
    }
    if !recipe.tags.is_empty() {
        out.push_str(&format!("tags: {}\n", recipe.tags.join(", ")));
    }

    // Import provenance
    match confidence {
        ExtractionConfidence::Structured => {
            out.push_str("import source: schema.org\n");
        }
        ExtractionConfidence::Fallback => {
            out.push_str("import source: html-fallback\n");
            out.push_str("import confidence: low\n");
        }
    }
    out.push_str("---\n\n");

    // --- Ingredients ---
    if !recipe.ingredients.is_empty() {
        for ing in &recipe.ingredients {
            out.push_str(&format_ingredient_line(ing));
            out.push('\n');
        }
        out.push('\n');
    }

    // --- Steps ---
    for step in &recipe.steps {
        out.push_str(&step.body);
        out.push_str("\n\n");
    }

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

    // --- ISO 8601 duration parsing ---

    #[test]
    fn iso8601_minutes_only() {
        assert_eq!(parse_iso8601_duration("PT15M"), Some("15 min".to_string()));
    }

    #[test]
    fn iso8601_hours_and_minutes() {
        assert_eq!(
            parse_iso8601_duration("PT1H30M"),
            Some("1 hr 30 min".to_string())
        );
    }

    #[test]
    fn iso8601_hours_only() {
        assert_eq!(parse_iso8601_duration("PT4H"), Some("4 hr".to_string()));
    }

    #[test]
    fn iso8601_hours_minutes_seconds() {
        assert_eq!(
            parse_iso8601_duration("PT2H15M30S"),
            Some("2 hr 15 min 30 sec".to_string())
        );
    }

    #[test]
    fn iso8601_days_and_time() {
        assert_eq!(
            parse_iso8601_duration("P1DT2H"),
            Some("1 day 2 hr".to_string())
        );
    }

    #[test]
    fn iso8601_seconds_only() {
        assert_eq!(parse_iso8601_duration("PT45S"), Some("45 sec".to_string()));
    }

    #[test]
    fn iso8601_empty_returns_none() {
        assert_eq!(parse_iso8601_duration("PT"), None);
    }

    #[test]
    fn iso8601_invalid_returns_none() {
        assert_eq!(parse_iso8601_duration("not a duration"), None);
    }

    #[test]
    fn iso8601_complex_tiramisu() {
        assert_eq!(
            parse_iso8601_duration("PT4H30M"),
            Some("4 hr 30 min".to_string())
        );
    }

    #[test]
    fn iso8601_multiple_days() {
        assert_eq!(
            parse_iso8601_duration("P3DT12H"),
            Some("3 days 12 hr".to_string())
        );
    }

    // --- Metadata sanitization ---

    #[test]
    fn sanitize_strips_html() {
        assert_eq!(
            sanitize_metadata_value("A <b>bold</b> recipe"),
            "A bold recipe"
        );
    }

    #[test]
    fn sanitize_decodes_entities() {
        assert_eq!(sanitize_metadata_value("Mac &amp; Cheese"), "Mac & Cheese");
    }

    #[test]
    fn sanitize_collapses_whitespace() {
        assert_eq!(
            sanitize_metadata_value("  too   many    spaces  "),
            "too many spaces"
        );
    }

    // --- Full conversion ---

    #[test]
    fn schema_to_recipe_basic() {
        let sr = SchemaRecipe {
            name: "Test Recipe".to_string(),
            description: Some("A test.".to_string()),
            author: Some(serde_json::json!("Chef Bob")),
            date_published: None,
            image: None,
            recipe_yield: Some(serde_json::json!("4 servings")),
            prep_time: Some("PT10M".to_string()),
            cook_time: Some("PT20M".to_string()),
            total_time: Some("PT30M".to_string()),
            recipe_category: Some(serde_json::json!("Main Course")),
            recipe_cuisine: Some(serde_json::json!("Italian")),
            keywords: Some(serde_json::json!("easy, quick")),
            nutrition: None,
            aggregate_rating: None,
            recipe_ingredient: Some(vec!["2 cups flour".to_string(), "1 tsp salt".to_string()]),
            recipe_instructions: Some(serde_json::json!([
                {"@type": "HowToStep", "text": "Mix flour and salt."},
                {"@type": "HowToStep", "text": "Knead the dough."}
            ])),
            video: None,
            suitable_for_diet: None,
            source_url: Some("https://example.com/test".to_string()),
        };

        let recipe = schema_to_recipe(&sr);
        assert_eq!(recipe.title, "Test Recipe");
        assert_eq!(recipe.source.as_deref(), Some("Chef Bob"));
        assert_eq!(recipe.prep_time.as_deref(), Some("10 min"));
        assert_eq!(recipe.cook_time.as_deref(), Some("20 min"));
        assert_eq!(recipe.total_time.as_deref(), Some("30 min"));
        assert_eq!(recipe.servings.as_deref(), Some("4 servings"));
        assert_eq!(recipe.ingredients.len(), 2);
        assert_eq!(recipe.steps.len(), 2);
        // Tags: easy, quick (from keywords) + italian (cuisine) + main course (category)
        assert!(recipe.tags.contains(&"easy".to_string()));
        assert!(recipe.tags.contains(&"italian".to_string()));
    }

    #[test]
    fn schema_to_recipe_numeric_yield() {
        let sr = SchemaRecipe {
            name: "Numeric Yield".to_string(),
            description: None,
            author: None,
            date_published: None,
            image: None,
            recipe_yield: Some(serde_json::json!(4)),
            prep_time: None,
            cook_time: None,
            total_time: None,
            recipe_category: None,
            recipe_cuisine: None,
            keywords: None,
            nutrition: None,
            aggregate_rating: None,
            recipe_ingredient: None,
            recipe_instructions: None,
            video: None,
            suitable_for_diet: None,
            source_url: None,
        };

        let recipe = schema_to_recipe(&sr);
        assert_eq!(recipe.servings.as_deref(), Some("4"));
    }

    // --- import_html integration ---

    #[test]
    fn import_html_structured() {
        let html = r#"<!DOCTYPE html>
<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Quick Pasta",
  "prepTime": "PT5M",
  "recipeIngredient": ["200g pasta", "2 tbsp olive oil"],
  "recipeInstructions": [
    {"@type": "HowToStep", "text": "Boil pasta."},
    {"@type": "HowToStep", "text": "Toss with oil."}
  ]
}
</script>
</head><body></body></html>"#;

        let (prepared, report) = import_html(html, "https://example.com/pasta", &[], &[]);

        assert_eq!(report.imported, 1);
        assert_eq!(report.total, 1);
        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].recipe.title, "Quick Pasta");
        assert!(prepared[0].cook_text.contains("import source: schema.org"));
        assert!(prepared[0].cook_text.contains("@pasta{200%g}"));
    }

    #[test]
    fn import_html_fallback() {
        let html = r#"<!DOCTYPE html>
<html><body>
<h1 class="recipe-title">Simple Soup</h1>
<ul class="ingredients">
  <li>2 cups water</li>
  <li>1 onion</li>
</ul>
<ol class="instructions">
  <li>Boil water.</li>
  <li>Add onion.</li>
</ol>
</body></html>"#;

        let (prepared, report) = import_html(html, "https://example.com/soup", &[], &[]);

        assert_eq!(report.imported, 1);
        assert_eq!(prepared[0].recipe.title, "Simple Soup");
        assert!(
            prepared[0]
                .cook_text
                .contains("import source: html-fallback")
        );
        assert!(prepared[0].cook_text.contains("import confidence: low"));
    }

    #[test]
    fn import_html_no_recipe() {
        let html = "<html><body><h1>About Us</h1><p>Blog.</p></body></html>";
        let (prepared, report) = import_html(html, "https://example.com/about", &[], &[]);

        assert_eq!(report.failed, 1);
        assert!(prepared.is_empty());
    }

    #[test]
    fn import_html_dedup() {
        let html = r#"<html><head>
<script type="application/ld+json">
{"@context":"https://schema.org","@type":"Recipe","name":"Dup",
 "recipeIngredient":["1 egg"]}
</script></head></html>"#;

        let existing_urls = vec!["https://example.com/dup".to_string()];
        let (prepared, report) = import_html(html, "https://example.com/dup", &[], &existing_urls);

        assert_eq!(report.skipped, 1);
        assert!(prepared.is_empty());
    }

    #[test]
    fn import_html_slug_collision() {
        let html = r#"<html><head>
<script type="application/ld+json">
{"@context":"https://schema.org","@type":"Recipe","name":"My Recipe",
 "recipeIngredient":["1 egg"]}
</script></head></html>"#;

        let existing_slugs = vec!["my-recipe".to_string()];
        let (prepared, _) = import_html(html, "https://example.com/recipe", &existing_slugs, &[]);

        assert_eq!(prepared[0].recipe.slug, "my-recipe-2");
        assert_eq!(prepared[0].file_name, "my-recipe-2.cook");
    }

    #[test]
    fn emit_cook_contains_all_fields() {
        let html = r#"<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Full Recipe",
  "author": "Chef Test",
  "description": "A full test recipe.",
  "prepTime": "PT10M",
  "cookTime": "PT20M",
  "totalTime": "PT30M",
  "recipeYield": "4",
  "recipeCuisine": "Italian",
  "recipeCategory": "Dinner",
  "keywords": "easy, healthy",
  "recipeIngredient": ["2 cups flour", "1 tsp salt"],
  "recipeInstructions": [
    {"@type": "HowToStep", "text": "Mix dry ingredients."},
    {"@type": "HowToStep", "text": "Add water and stir."}
  ]
}
</script></head></html>"#;

        let (prepared, _) = import_html(html, "https://example.com/full", &[], &[]);
        let cook = &prepared[0].cook_text;

        assert!(cook.contains("title: Full Recipe"));
        assert!(cook.contains("source: Chef Test"));
        assert!(cook.contains("source url: https://example.com/full"));
        assert!(cook.contains("prep time: 10 min"));
        assert!(cook.contains("cook time: 20 min"));
        assert!(cook.contains("total time: 30 min"));
        assert!(cook.contains("servings: 4"));
        assert!(cook.contains("description: A full test recipe."));
        assert!(cook.contains("import source: schema.org"));
        assert!(cook.contains("@flour{2%cups}"));
        assert!(cook.contains("@salt{1%tsp}"));
        assert!(cook.contains("Mix dry ingredients."));
        assert!(cook.contains("Add water and stir."));
    }

    // --- extract_string_list ---

    #[test]
    fn string_list_from_csv() {
        let v = serde_json::json!("pasta, Italian, quick");
        let list = extract_string_list(&v);
        assert_eq!(list, vec!["pasta", "italian", "quick"]);
    }

    #[test]
    fn string_list_from_array() {
        let v = serde_json::json!(["Pasta", "Italian"]);
        let list = extract_string_list(&v);
        assert_eq!(list, vec!["pasta", "italian"]);
    }

    // --- extract_yield_string ---

    #[test]
    fn yield_from_string() {
        let v = Some(serde_json::json!("4 servings"));
        assert_eq!(extract_yield_string(&v).as_deref(), Some("4 servings"));
    }

    #[test]
    fn yield_from_number() {
        let v = Some(serde_json::json!(6));
        assert_eq!(extract_yield_string(&v).as_deref(), Some("6"));
    }

    #[test]
    fn yield_from_array() {
        let v = Some(serde_json::json!(["4 servings", "2 loaves"]));
        assert_eq!(extract_yield_string(&v).as_deref(), Some("4 servings"));
    }
}
