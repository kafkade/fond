use scraper::{Html, Selector};

use super::types::SchemaRecipe;

/// Extract all schema.org Recipe objects from an HTML page's JSON-LD blocks.
///
/// Looks for `<script type="application/ld+json">` tags, parses the JSON,
/// and walks the structure to find Recipe objects — handling direct objects,
/// `@graph` wrappers, and `@type` as array.
pub fn extract_recipes_from_html(html: &str) -> Vec<SchemaRecipe> {
    let document = Html::parse_document(html);
    let selector = Selector::parse(r#"script[type="application/ld+json"]"#).unwrap();
    let mut recipes = Vec::new();

    for element in document.select(&selector) {
        let json_text = element.text().collect::<String>();
        let json_text = json_text.trim();

        if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_text) {
            extract_recipes_from_value(&value, &mut recipes);
        }
    }

    recipes
}

/// Recursively extract Recipe objects from a JSON-LD value.
/// Handles: direct Recipe, @graph arrays, @type as string or array.
fn extract_recipes_from_value(value: &serde_json::Value, recipes: &mut Vec<SchemaRecipe>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(graph) = map.get("@graph") {
                if let serde_json::Value::Array(items) = graph {
                    for item in items {
                        extract_recipes_from_value(item, recipes);
                    }
                }
                return;
            }

            if is_recipe_type(map.get("@type"))
                && let Ok(recipe) = serde_json::from_value::<SchemaRecipe>(value.clone())
            {
                recipes.push(recipe);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                extract_recipes_from_value(item, recipes);
            }
        }
        _ => {}
    }
}

/// Check if a @type value indicates a Recipe.
/// Handles: "Recipe" (string), ["Recipe", "HowTo"] (array).
fn is_recipe_type(type_val: Option<&serde_json::Value>) -> bool {
    match type_val {
        Some(serde_json::Value::String(s)) => s == "Recipe",
        Some(serde_json::Value::Array(arr)) => arr.iter().any(|v| v.as_str() == Some("Recipe")),
        _ => false,
    }
}

/// Extract step text from `recipeInstructions`, handling all known variants:
///
/// - `HowToStep` array (most common)
/// - `HowToSection` groups with nested `itemListElement`
/// - Plain string array
/// - Single concatenated string (split on newlines)
pub fn extract_steps(instructions: &serde_json::Value) -> Vec<String> {
    match instructions {
        serde_json::Value::String(s) => s
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),

        serde_json::Value::Array(items) => {
            let mut steps = Vec::new();
            for item in items {
                match item {
                    serde_json::Value::String(s) => {
                        steps.push(s.trim().to_string());
                    }
                    serde_json::Value::Object(map)
                        if map
                            .get("@type")
                            .and_then(|t| t.as_str())
                            .is_some_and(|t| t == "HowToStep") =>
                    {
                        if let Some(text) = map.get("text").and_then(|t| t.as_str()) {
                            steps.push(text.trim().to_string());
                        }
                    }
                    serde_json::Value::Object(map)
                        if map
                            .get("@type")
                            .and_then(|t| t.as_str())
                            .is_some_and(|t| t == "HowToSection") =>
                    {
                        if let Some(list) = map.get("itemListElement") {
                            steps.extend(extract_steps(list));
                        }
                    }
                    _ => {}
                }
            }
            steps
        }
        _ => Vec::new(),
    }
}

/// Extract author name from the polymorphic `author` field.
///
/// Handles: plain string, Person/Organization object with `name`, array.
pub fn extract_author(author: &serde_json::Value) -> Option<String> {
    match author {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(map) => {
            map.get("name").and_then(|n| n.as_str()).map(String::from)
        }
        serde_json::Value::Array(arr) => arr.first().and_then(extract_author),
        _ => None,
    }
}

/// Basic HTML fallback: extract recipe info from common HTML patterns
/// when no JSON-LD is available.
///
/// Lower confidence than JSON-LD extraction. Returns `None` if the page
/// doesn't look like a recipe (no ingredients found).
pub fn extract_recipe_from_html_fallback(html: &str) -> Option<SchemaRecipe> {
    let document = Html::parse_document(html);

    let title = extract_first_text(&document, "h1.recipe-title, h2.recipe-title, h1");

    let ingredients = extract_all_text(
        &document,
        ".ingredients li, .recipe-ingredients li, [class*=\"ingredient\"] li",
    );

    let steps = extract_all_text(
        &document,
        ".instructions li, .directions li, .recipe-steps li, [class*=\"instruction\"] li, [class*=\"direction\"] li",
    );

    if ingredients.is_empty() {
        return None;
    }

    // Build a SchemaRecipe from fallback data — title is required for a valid import
    let name = title.unwrap_or_else(|| "Untitled Recipe".to_string());

    let step_values: Vec<serde_json::Value> =
        steps.into_iter().map(serde_json::Value::String).collect();

    Some(SchemaRecipe {
        name,
        description: None,
        author: None,
        date_published: None,
        image: None,
        recipe_yield: None,
        prep_time: None,
        cook_time: None,
        total_time: None,
        recipe_category: None,
        recipe_cuisine: None,
        keywords: None,
        nutrition: None,
        aggregate_rating: None,
        recipe_ingredient: Some(ingredients),
        recipe_instructions: if step_values.is_empty() {
            None
        } else {
            Some(serde_json::Value::Array(step_values))
        },
        video: None,
        suitable_for_diet: None,
        source_url: None,
    })
}

fn extract_first_text(document: &Html, selector_str: &str) -> Option<String> {
    let selector = Selector::parse(selector_str).ok()?;
    document
        .select(&selector)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
}

fn extract_all_text(document: &Html, selector_str: &str) -> Vec<String> {
    let Ok(selector) = Selector::parse(selector_str) else {
        return Vec::new();
    };
    document
        .select(&selector)
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Normalize a URL for dedup: lowercase scheme/host, strip fragment,
/// remove common tracking params, normalize trailing slash.
pub fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if url.is_empty() {
        return String::new();
    }

    // Split off fragment
    let url = url.split('#').next().unwrap_or(url);

    // Split into base and query
    let (base, query) = match url.split_once('?') {
        Some((b, q)) => (b, Some(q)),
        None => (url, None),
    };

    // Lowercase scheme + host (everything up to first path /)
    let lower = base.to_lowercase();
    let normalized_base = if lower.starts_with("https://") || lower.starts_with("http://") {
        let scheme_end = lower.find("://").unwrap() + 3;
        let scheme = &lower[..scheme_end]; // "https://" or "http://"
        let after_scheme = &base[scheme_end..];
        let (host_and_port, path) = match after_scheme.find('/') {
            Some(i) => (&after_scheme[..i], &after_scheme[i..]),
            None => (after_scheme, ""),
        };
        format!("{scheme}{}{path}", host_and_port.to_lowercase())
    } else {
        base.to_string()
    };

    // Strip trailing slash, but only from non-root paths
    // "https://example.com/" is root → keep
    // "https://example.com/recipe/" → strip
    let normalized_base = if normalized_base.ends_with('/') {
        let after_scheme = if let Some(i) = normalized_base.find("://") {
            &normalized_base[i + 3..]
        } else {
            &normalized_base
        };
        // Count slashes after host — root has exactly one
        let slash_count = after_scheme.chars().filter(|&c| c == '/').count();
        if slash_count > 1 {
            normalized_base[..normalized_base.len() - 1].to_string()
        } else {
            normalized_base
        }
    } else {
        normalized_base
    };

    // Filter tracking params from query string
    let tracking_params = [
        "utm_source",
        "utm_medium",
        "utm_campaign",
        "utm_term",
        "utm_content",
        "fbclid",
        "gclid",
        "ref",
        "source",
    ];

    match query {
        Some(q) => {
            let filtered: Vec<&str> = q
                .split('&')
                .filter(|param| {
                    let key = param.split('=').next().unwrap_or("");
                    !tracking_params.contains(&key)
                })
                .collect();

            if filtered.is_empty() {
                normalized_base
            } else {
                format!("{normalized_base}?{}", filtered.join("&"))
            }
        }
        None => normalized_base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- JSON-LD extraction ---

    #[test]
    fn extract_direct_recipe() {
        let html = direct_howto_steps_html();
        let recipes = extract_recipes_from_html(html);
        assert_eq!(recipes.len(), 1);
        assert_eq!(recipes[0].name, "Spaghetti Carbonara");
    }

    #[test]
    fn extract_from_graph_wrapper() {
        let html = graph_wrapper_html();
        let recipes = extract_recipes_from_html(html);
        assert_eq!(recipes.len(), 1);
        assert_eq!(recipes[0].name, "Thai Basil Chicken (Pad Krapao Gai)");
    }

    #[test]
    fn extract_type_as_array() {
        let html = type_array_html();
        let recipes = extract_recipes_from_html(html);
        assert_eq!(recipes.len(), 1);
        assert_eq!(recipes[0].name, "Miso Glazed Salmon");
    }

    #[test]
    fn skip_non_recipe_blocks() {
        let html = type_array_html();
        let recipes = extract_recipes_from_html(html);
        assert_eq!(
            recipes.len(),
            1,
            "WebSite block should not produce a recipe"
        );
    }

    #[test]
    fn malformed_jsonld_skipped() {
        let html = malformed_jsonld_html();
        let recipes = extract_recipes_from_html(html);
        assert_eq!(recipes.len(), 1);
        assert_eq!(recipes[0].name, "Survivor Recipe");
    }

    #[test]
    fn no_recipes_from_html_only() {
        let html = html_only_page();
        let recipes = extract_recipes_from_html(html);
        assert!(recipes.is_empty());
    }

    // --- Instruction variants ---

    #[test]
    fn instructions_howto_steps() {
        let recipes = extract_recipes_from_html(direct_howto_steps_html());
        let steps = extract_steps(recipes[0].recipe_instructions.as_ref().unwrap());
        assert_eq!(steps.len(), 4);
        assert!(steps[0].contains("Bring a large pot"));
    }

    #[test]
    fn instructions_plain_strings() {
        let html = plain_string_instructions_html();
        let recipes = extract_recipes_from_html(html);
        let steps = extract_steps(recipes[0].recipe_instructions.as_ref().unwrap());
        assert_eq!(steps.len(), 6);
        assert!(steps[0].contains("Chop tomatoes"));
    }

    #[test]
    fn instructions_howto_sections() {
        let html = howto_sections_html();
        let recipes = extract_recipes_from_html(html);
        let steps = extract_steps(recipes[0].recipe_instructions.as_ref().unwrap());
        assert_eq!(steps.len(), 10);
    }

    #[test]
    fn instructions_single_string() {
        let html = single_string_instructions_html();
        let recipes = extract_recipes_from_html(html);
        let steps = extract_steps(recipes[0].recipe_instructions.as_ref().unwrap());
        assert_eq!(steps.len(), 4);
        assert!(steps[0].contains("Halve and pit"));
    }

    // --- Author extraction ---

    #[test]
    fn author_from_person_object() {
        let recipes = extract_recipes_from_html(direct_howto_steps_html());
        let author = extract_author(recipes[0].author.as_ref().unwrap());
        assert_eq!(author.as_deref(), Some("Marco Rossi"));
    }

    #[test]
    fn author_from_plain_string() {
        let recipes = extract_recipes_from_html(plain_string_instructions_html());
        let author = extract_author(recipes[0].author.as_ref().unwrap());
        assert_eq!(author.as_deref(), Some("Elena's Kitchen"));
    }

    #[test]
    fn author_from_array() {
        let v = serde_json::json!([{"@type": "Person", "name": "Alice"}]);
        assert_eq!(extract_author(&v).as_deref(), Some("Alice"));
    }

    // --- Field extraction ---

    #[test]
    fn extract_times() {
        let recipes = extract_recipes_from_html(direct_howto_steps_html());
        let r = &recipes[0];
        assert_eq!(r.prep_time.as_deref(), Some("PT15M"));
        assert_eq!(r.cook_time.as_deref(), Some("PT20M"));
        assert_eq!(r.total_time.as_deref(), Some("PT35M"));
    }

    #[test]
    fn extract_ingredients() {
        let recipes = extract_recipes_from_html(direct_howto_steps_html());
        let ings = recipes[0].recipe_ingredient.as_ref().unwrap();
        assert_eq!(ings.len(), 7);
        assert!(ings[0].contains("spaghetti"));
    }

    #[test]
    fn extract_yield() {
        let recipes = extract_recipes_from_html(direct_howto_steps_html());
        assert_eq!(
            recipes[0].recipe_yield.as_ref().and_then(|v| v.as_str()),
            Some("4 servings")
        );
    }

    // --- HTML fallback ---

    #[test]
    fn fallback_extracts_recipe() {
        let recipe = extract_recipe_from_html_fallback(html_only_page()).unwrap();
        assert_eq!(recipe.name, "Grandma's Peanut Butter Cookies");
        assert_eq!(recipe.recipe_ingredient.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn fallback_returns_none_for_non_recipe() {
        let html = "<html><body><h1>About Us</h1><p>We are a blog.</p></body></html>";
        assert!(extract_recipe_from_html_fallback(html).is_none());
    }

    // --- URL normalization ---

    #[test]
    fn normalize_url_strips_fragment() {
        assert_eq!(
            normalize_url("https://example.com/recipe#top"),
            "https://example.com/recipe"
        );
    }

    #[test]
    fn normalize_url_strips_tracking() {
        assert_eq!(
            normalize_url("https://example.com/recipe?utm_source=twitter&id=5"),
            "https://example.com/recipe?id=5"
        );
    }

    #[test]
    fn normalize_url_lowercases_host() {
        assert_eq!(
            normalize_url("HTTPS://Example.COM/Recipe"),
            "https://example.com/Recipe"
        );
    }

    #[test]
    fn normalize_url_strips_trailing_slash() {
        assert_eq!(
            normalize_url("https://example.com/recipe/"),
            "https://example.com/recipe"
        );
    }

    #[test]
    fn normalize_url_preserves_root() {
        assert_eq!(
            normalize_url("https://example.com/"),
            "https://example.com/"
        );
    }

    #[test]
    fn normalize_url_empty() {
        assert_eq!(normalize_url(""), "");
    }

    // --- Test fixtures ---

    fn direct_howto_steps_html() -> &'static str {
        r#"<!DOCTYPE html>
<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Spaghetti Carbonara",
  "author": {"@type": "Person", "name": "Marco Rossi"},
  "description": "A classic Roman pasta dish with eggs, cheese, and guanciale.",
  "recipeYield": "4 servings",
  "prepTime": "PT15M",
  "cookTime": "PT20M",
  "totalTime": "PT35M",
  "recipeCategory": "Main Course",
  "recipeCuisine": "Italian",
  "keywords": "pasta, carbonara, Italian, Roman",
  "recipeIngredient": [
    "400g spaghetti",
    "200g guanciale, cut into strips",
    "4 large egg yolks",
    "2 whole eggs",
    "100g Pecorino Romano, finely grated",
    "50g Parmigiano-Reggiano, finely grated",
    "Freshly ground black pepper"
  ],
  "recipeInstructions": [
    {"@type": "HowToStep", "text": "Bring a large pot of salted water to a boil. Cook spaghetti until al dente."},
    {"@type": "HowToStep", "text": "Cook guanciale in a cold pan over medium heat until crispy, about 8 minutes."},
    {"@type": "HowToStep", "text": "Whisk egg yolks, whole eggs, and grated cheeses together. Season generously with black pepper."},
    {"@type": "HowToStep", "text": "Add drained pasta to guanciale pan off heat. Pour egg mixture over and toss vigorously until creamy."}
  ]
}
</script>
</head><body><h1>Spaghetti Carbonara</h1></body></html>"#
    }

    fn graph_wrapper_html() -> &'static str {
        r#"<!DOCTYPE html>
<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@graph": [
    {"@type": "WebPage", "name": "Thai Basil Chicken"},
    {
      "@type": "Recipe",
      "name": "Thai Basil Chicken (Pad Krapao Gai)",
      "author": {"@type": "Person", "name": "Siri Cooks"},
      "prepTime": "PT10M",
      "cookTime": "PT8M",
      "recipeYield": "2",
      "recipeIngredient": ["500g chicken thigh, minced", "3 cloves garlic, minced"],
      "recipeInstructions": [
        {"@type": "HowToStep", "text": "Heat oil in a wok over high heat."},
        {"@type": "HowToStep", "text": "Add garlic and stir-fry."}
      ]
    }
  ]
}
</script>
</head><body></body></html>"#
    }

    fn type_array_html() -> &'static str {
        r#"<!DOCTYPE html>
<html><head>
<script type="application/ld+json">
{"@context": "https://schema.org", "@type": "WebSite", "name": "Blog"}
</script>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": ["Recipe", "HowTo"],
  "name": "Miso Glazed Salmon",
  "prepTime": "PT5M",
  "cookTime": "PT12M",
  "recipeIngredient": ["2 salmon fillets"],
  "recipeInstructions": [{"@type": "HowToStep", "text": "Broil salmon."}]
}
</script>
</head><body></body></html>"#
    }

    fn plain_string_instructions_html() -> &'static str {
        r#"<!DOCTYPE html>
<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Simple Greek Salad",
  "author": "Elena's Kitchen",
  "prepTime": "PT10M",
  "recipeIngredient": ["3 large tomatoes", "1 cucumber"],
  "recipeInstructions": [
    "Chop tomatoes and cucumber.",
    "Slice red onion.",
    "Combine vegetables.",
    "Top with feta.",
    "Drizzle with olive oil.",
    "Toss gently and serve."
  ]
}
</script>
</head><body></body></html>"#
    }

    fn howto_sections_html() -> &'static str {
        r#"<!DOCTYPE html>
<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Classic Tiramisu",
  "prepTime": "PT30M",
  "totalTime": "PT4H30M",
  "recipeIngredient": ["6 egg yolks", "500g mascarpone"],
  "recipeInstructions": [
    {
      "@type": "HowToSection",
      "name": "Make the Cream",
      "itemListElement": [
        {"@type": "HowToStep", "text": "Beat egg yolks and sugar."},
        {"@type": "HowToStep", "text": "Add mascarpone."},
        {"@type": "HowToStep", "text": "Whip cream to stiff peaks."},
        {"@type": "HowToStep", "text": "Fold whipped cream into mixture."}
      ]
    },
    {
      "@type": "HowToSection",
      "name": "Assemble",
      "itemListElement": [
        {"@type": "HowToStep", "text": "Mix espresso and liqueur."},
        {"@type": "HowToStep", "text": "Dip ladyfingers."},
        {"@type": "HowToStep", "text": "Spread half the cream."},
        {"@type": "HowToStep", "text": "Repeat layers."}
      ]
    },
    {
      "@type": "HowToSection",
      "name": "Chill",
      "itemListElement": [
        {"@type": "HowToStep", "text": "Refrigerate 4 hours."},
        {"@type": "HowToStep", "text": "Dust with cocoa."}
      ]
    }
  ]
}
</script>
</head><body></body></html>"#
    }

    fn single_string_instructions_html() -> &'static str {
        r#"<!DOCTYPE html>
<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Quick Guacamole",
  "recipeIngredient": ["3 ripe avocados", "1 lime"],
  "recipeInstructions": "Halve and pit avocados.\nMash with a fork.\nStir in lime juice.\nServe immediately."
}
</script>
</head><body></body></html>"#
    }

    fn malformed_jsonld_html() -> &'static str {
        r#"<!DOCTYPE html>
<html><head>
<script type="application/ld+json">
{ this is not valid JSON }}}
</script>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Survivor Recipe",
  "recipeIngredient": ["1 cup resilience"],
  "recipeInstructions": [{"@type": "HowToStep", "text": "Keep going."}]
}
</script>
</head><body></body></html>"#
    }

    fn html_only_page() -> &'static str {
        r#"<!DOCTYPE html>
<html><body>
<h1 class="recipe-title">Grandma's Peanut Butter Cookies</h1>
<ul class="ingredients">
  <li>1 cup peanut butter</li>
  <li>1 cup sugar</li>
  <li>1 large egg</li>
</ul>
<ol class="instructions">
  <li>Preheat oven to 350°F.</li>
  <li>Mix ingredients until smooth.</li>
  <li>Roll into balls.</li>
  <li>Press with a fork.</li>
  <li>Bake for 10-12 minutes.</li>
</ol>
</body></html>"#
    }
}
