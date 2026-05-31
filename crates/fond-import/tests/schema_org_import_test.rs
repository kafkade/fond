//! Integration tests for the schema.org/URL import pipeline.

use fond_import::schema_org;

const CARBONARA_HTML: &str = r#"<!DOCTYPE html>
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
    {"@type": "HowToStep", "text": "Whisk egg yolks, whole eggs, and grated cheeses together."},
    {"@type": "HowToStep", "text": "Add drained pasta to guanciale pan off heat. Pour egg mixture over and toss vigorously."}
  ]
}
</script>
</head><body></body></html>"#;

#[test]
fn import_carbonara_produces_complete_recipe() {
    let (prepared, report) =
        schema_org::import_html(CARBONARA_HTML, "https://example.com/carbonara", &[], &[]);

    assert_eq!(report.imported, 1);
    assert_eq!(report.total, 1);
    assert_eq!(prepared.len(), 1);

    let recipe = &prepared[0].recipe;
    assert_eq!(recipe.title, "Spaghetti Carbonara");
    assert_eq!(recipe.slug, "spaghetti-carbonara");
    assert_eq!(recipe.source.as_deref(), Some("Marco Rossi"));
    assert_eq!(
        recipe.source_url.as_deref(),
        Some("https://example.com/carbonara")
    );
    assert_eq!(recipe.prep_time.as_deref(), Some("15 min"));
    assert_eq!(recipe.cook_time.as_deref(), Some("20 min"));
    assert_eq!(recipe.total_time.as_deref(), Some("35 min"));
    assert_eq!(recipe.servings.as_deref(), Some("4 servings"));
    assert_eq!(recipe.ingredients.len(), 7);
    assert_eq!(recipe.steps.len(), 4);
}

#[test]
fn import_carbonara_cook_text_is_valid() {
    let (prepared, _) =
        schema_org::import_html(CARBONARA_HTML, "https://example.com/carbonara", &[], &[]);

    let cook = &prepared[0].cook_text;
    assert!(cook.starts_with("---\n"));
    assert!(cook.contains("title: Spaghetti Carbonara"));
    assert!(cook.contains("source: Marco Rossi"));
    assert!(cook.contains("source url: https://example.com/carbonara"));
    assert!(cook.contains("import source: schema.org"));
    assert!(cook.contains("prep time: 15 min"));
    assert!(cook.contains("@spaghetti{400%g}"));
    assert!(cook.contains("Bring a large pot"));
}

#[test]
fn import_dedup_by_source_url() {
    let existing = vec!["https://example.com/carbonara".to_string()];

    let (prepared, report) = schema_org::import_html(
        CARBONARA_HTML,
        "https://example.com/carbonara",
        &[],
        &existing,
    );

    assert_eq!(report.skipped, 1);
    assert!(prepared.is_empty());
}

#[test]
fn import_dedup_url_normalization() {
    // Existing URL with trailing slash and tracking params
    let existing = vec!["https://example.com/carbonara/?utm_source=twitter".to_string()];

    let (prepared, report) = schema_org::import_html(
        CARBONARA_HTML,
        "https://example.com/carbonara",
        &[],
        &existing,
    );

    assert_eq!(
        report.skipped, 1,
        "should detect normalized URL as duplicate"
    );
    assert!(prepared.is_empty());
}

#[test]
fn import_slug_collision_resolved() {
    let existing_slugs = vec!["spaghetti-carbonara".to_string()];

    let (prepared, report) = schema_org::import_html(
        CARBONARA_HTML,
        "https://example.com/carbonara-new",
        &existing_slugs,
        &[],
    );

    assert_eq!(report.imported, 1);
    assert_eq!(prepared[0].recipe.slug, "spaghetti-carbonara-2");
    assert_eq!(prepared[0].file_name, "spaghetti-carbonara-2.cook");
}

#[test]
fn import_no_recipe_page() {
    let html = "<html><body><h1>About Us</h1><p>We make food.</p></body></html>";

    let (prepared, report) = schema_org::import_html(html, "https://example.com/about", &[], &[]);

    assert_eq!(report.failed, 1);
    assert!(prepared.is_empty());
}

#[test]
fn import_html_fallback_works() {
    let html = r#"<!DOCTYPE html>
<html><body>
<h1 class="recipe-title">Grandma's Cookies</h1>
<ul class="ingredients">
  <li>1 cup peanut butter</li>
  <li>1 cup sugar</li>
  <li>1 large egg</li>
</ul>
<ol class="instructions">
  <li>Preheat oven.</li>
  <li>Mix and bake.</li>
</ol>
</body></html>"#;

    let (prepared, report) = schema_org::import_html(html, "https://example.com/cookies", &[], &[]);

    assert_eq!(report.imported, 1);
    let cook = &prepared[0].cook_text;
    assert!(cook.contains("import source: html-fallback"));
    assert!(cook.contains("import confidence: low"));
    assert!(cook.contains("Grandma's Cookies"));
}

#[test]
fn import_graph_wrapper_wordpress() {
    let html = r#"<!DOCTYPE html>
<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@graph": [
    {"@type": "WebPage", "name": "Blog Post"},
    {
      "@type": "Recipe",
      "name": "Thai Basil Chicken",
      "author": {"@type": "Person", "name": "Chef Thai"},
      "prepTime": "PT10M",
      "cookTime": "PT8M",
      "recipeIngredient": ["500g chicken", "2 cups basil"],
      "recipeInstructions": [
        {"@type": "HowToStep", "text": "Heat wok."},
        {"@type": "HowToStep", "text": "Stir-fry chicken."}
      ]
    }
  ]
}
</script></head></html>"#;

    let (prepared, report) =
        schema_org::import_html(html, "https://example.com/thai-basil", &[], &[]);

    assert_eq!(report.imported, 1);
    assert_eq!(prepared[0].recipe.title, "Thai Basil Chicken");
    assert!(prepared[0].cook_text.contains("import source: schema.org"));
}

#[test]
fn import_malformed_json_skipped_gracefully() {
    let html = r#"<!DOCTYPE html>
<html><head>
<script type="application/ld+json">
{ broken JSON here }}}
</script>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Survivor",
  "recipeIngredient": ["1 cup resilience"],
  "recipeInstructions": [{"@type": "HowToStep", "text": "Persist."}]
}
</script></head></html>"#;

    let (prepared, report) =
        schema_org::import_html(html, "https://example.com/survivor", &[], &[]);

    assert_eq!(report.imported, 1);
    assert_eq!(prepared[0].recipe.title, "Survivor");
}

#[test]
fn import_howto_sections_produces_steps() {
    let html = r#"<!DOCTYPE html>
<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Tiramisu",
  "recipeIngredient": ["6 egg yolks", "500g mascarpone"],
  "recipeInstructions": [
    {
      "@type": "HowToSection",
      "name": "Cream",
      "itemListElement": [
        {"@type": "HowToStep", "text": "Beat yolks."},
        {"@type": "HowToStep", "text": "Add mascarpone."}
      ]
    },
    {
      "@type": "HowToSection",
      "name": "Assemble",
      "itemListElement": [
        {"@type": "HowToStep", "text": "Dip ladyfingers."},
        {"@type": "HowToStep", "text": "Layer cream."}
      ]
    }
  ]
}
</script></head></html>"#;

    let (prepared, _) = schema_org::import_html(html, "https://example.com/tiramisu", &[], &[]);

    assert_eq!(prepared[0].recipe.steps.len(), 4);
}

#[test]
fn import_iso_8601_times_converted() {
    let html = r#"<!DOCTYPE html>
<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Quick Dish",
  "prepTime": "PT5M",
  "cookTime": "PT1H15M",
  "totalTime": "PT1H20M",
  "recipeIngredient": ["1 onion"]
}
</script></head></html>"#;

    let (prepared, _) = schema_org::import_html(html, "https://example.com/quick", &[], &[]);

    let recipe = &prepared[0].recipe;
    assert_eq!(recipe.prep_time.as_deref(), Some("5 min"));
    assert_eq!(recipe.cook_time.as_deref(), Some("1 hr 15 min"));
    assert_eq!(recipe.total_time.as_deref(), Some("1 hr 20 min"));
}

#[test]
fn import_tags_from_keywords_cuisine_category() {
    let html = r#"<!DOCTYPE html>
<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Tagged Dish",
  "keywords": "easy, healthy",
  "recipeCuisine": "Mexican",
  "recipeCategory": "Dinner",
  "recipeIngredient": ["1 tortilla"]
}
</script></head></html>"#;

    let (prepared, _) = schema_org::import_html(html, "https://example.com/tagged", &[], &[]);

    let tags = &prepared[0].recipe.tags;
    assert!(tags.contains(&"easy".to_string()));
    assert!(tags.contains(&"healthy".to_string()));
    assert!(tags.contains(&"mexican".to_string()));
    assert!(tags.contains(&"dinner".to_string()));
}
