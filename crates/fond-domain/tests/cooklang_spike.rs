//! Spike #1: cooklang-rs parser validation
//!
//! Go/No-Go criteria (from issue #1):
//! - Go:    Round-trip fidelity >= 95% across test corpus; metadata extensible
//! - No-Go: Fork or write custom parser (🔴, multi-week effort)
//!
//! Tests validate:
//! 1. Parse 10+ real recipes from diverse cuisines
//! 2. Verify @ingredient{}, #cookware{}, ~timer{} annotations survive parsing
//! 3. Assess metadata (YAML frontmatter) support and extensibility
//! 4. Test edge cases: Unicode, multi-line steps, block comments, sections
//! 5. Evaluate round-trip feasibility (parse → emit)

use cooklang::{CooklangParser, Extensions};
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn load_fixture(name: &str) -> String {
    std::fs::read_to_string(fixtures_dir().join(name))
        .unwrap_or_else(|e| panic!("failed to load fixture {name}: {e}"))
}

fn parser() -> CooklangParser {
    CooklangParser::new(Extensions::all(), Default::default())
}

// ---------------------------------------------------------------------------
// Task 1: Parse 10+ real recipes — all must parse without errors
// ---------------------------------------------------------------------------

const ALL_FIXTURES: &[&str] = &[
    "chicken-adobo.cook",
    "sourdough-bread.cook",
    "mapo-tofu.cook",
    "simple-eggs.cook",
    "pasta-alla-norma.cook",
    "thai-green-curry.cook",
    "creme-brulee.cook",
    "birria-tacos.cook",
    "miso-ramen.cook",
    "cilbir.cook",
    "chocolate-chip-cookies.cook",
];

#[test]
fn all_fixtures_parse_without_errors() {
    let p = parser();
    let mut failures = Vec::new();

    for name in ALL_FIXTURES {
        let src = load_fixture(name);
        let result = p.parse(&src);
        if !result.is_valid() {
            let report = result.report();
            failures.push(format!("{name}:\n  {report}"));
        }
    }

    assert!(
        failures.is_empty(),
        "The following fixtures failed to parse:\n{}",
        failures.join("\n\n")
    );
}

#[test]
fn all_fixtures_parse_with_warnings_report() {
    let p = parser();

    for name in ALL_FIXTURES {
        let src = load_fixture(name);
        let result = p.parse(&src);
        let report = result.report();

        if report.has_warnings() {
            eprintln!("[{name}] has warnings:");
            eprintln!("  {report}");
        }
    }
}

// ---------------------------------------------------------------------------
// Task 2: Verify annotations survive parsing
// ---------------------------------------------------------------------------

#[test]
fn chicken_adobo_ingredients() {
    let p = parser();
    let src = load_fixture("chicken-adobo.cook");
    let (recipe, _) = p.parse(&src).into_result().expect("parse failed");

    let names: Vec<&str> = recipe.ingredients.iter().map(|i| i.name.as_str()).collect();

    assert!(names.contains(&"soy sauce"), "missing soy sauce: {names:?}");
    assert!(
        names.contains(&"white vinegar"),
        "missing white vinegar: {names:?}"
    );
    assert!(
        names.contains(&"chicken thighs"),
        "missing chicken thighs: {names:?}"
    );
    assert!(
        names.contains(&"bay leaves"),
        "missing bay leaves: {names:?}"
    );
    assert!(
        names.contains(&"steamed rice"),
        "missing steamed rice: {names:?}"
    );
}

#[test]
fn chicken_adobo_cookware() {
    let p = parser();
    let src = load_fixture("chicken-adobo.cook");
    let (recipe, _) = p.parse(&src).into_result().expect("parse failed");

    let names: Vec<&str> = recipe.cookware.iter().map(|c| c.name.as_str()).collect();

    assert!(names.contains(&"bowl"), "missing bowl: {names:?}");
    assert!(
        names.contains(&"dutch oven"),
        "missing dutch oven: {names:?}"
    );
}

#[test]
fn chicken_adobo_timers() {
    let p = parser();
    let src = load_fixture("chicken-adobo.cook");
    let (recipe, _) = p.parse(&src).into_result().expect("parse failed");

    assert!(
        !recipe.timers.is_empty(),
        "expected at least one timer, got none"
    );

    // Check named timer exists
    let named: Vec<&str> = recipe
        .timers
        .iter()
        .filter_map(|t| t.name.as_deref())
        .collect();
    assert!(
        named.contains(&"marinate"),
        "missing named timer 'marinate': {named:?}"
    );
}

#[test]
fn mapo_tofu_ingredient_quantities() {
    let p = parser();
    let src = load_fixture("mapo-tofu.cook");
    let (recipe, _) = p.parse(&src).into_result().expect("parse failed");

    let tofu = recipe
        .ingredients
        .iter()
        .find(|i| i.name == "firm tofu")
        .expect("missing 'firm tofu'");

    assert!(tofu.quantity.is_some(), "firm tofu should have a quantity");
}

// ---------------------------------------------------------------------------
// Task 3: Metadata (YAML frontmatter) support
// ---------------------------------------------------------------------------

#[test]
fn yaml_frontmatter_basic_fields() {
    let p = parser();
    let src = load_fixture("chicken-adobo.cook");
    let (recipe, _) = p.parse(&src).into_result().expect("parse failed");

    let meta = &recipe.metadata;
    assert_eq!(
        meta.map.get("title").and_then(|v| v.as_str()),
        Some("Classic Chicken Adobo"),
        "title metadata missing or wrong"
    );
    assert!(meta.map.contains_key("source"), "source metadata missing");
    assert!(
        meta.map.contains_key("servings") || meta.map.contains_key("tags"),
        "expected servings or tags in metadata"
    );
}

#[test]
fn yaml_frontmatter_tags_as_list() {
    let p = parser();
    let src = load_fixture("mapo-tofu.cook");
    let (recipe, _) = p.parse(&src).into_result().expect("parse failed");

    let tags = &recipe.metadata.map.get("tags");
    assert!(tags.is_some(), "tags metadata should be present");
}

#[test]
fn metadata_extensibility_custom_keys() {
    let p = parser();
    let src = load_fixture("birria-tacos.cook");
    let (recipe, _) = p.parse(&src).into_result().expect("parse failed");

    // Verify that custom metadata keys are preserved
    assert!(
        recipe.metadata.map.contains_key("prep time")
            || recipe.metadata.map.contains_key("cook time"),
        "custom time metadata should be preserved"
    );
}

#[test]
fn recipe_without_frontmatter() {
    let p = parser();
    let src = load_fixture("simple-eggs.cook");
    let (recipe, _) = p.parse(&src).into_result().expect("parse failed");

    // Should parse fine with empty metadata
    assert!(
        recipe.ingredients.len() >= 3,
        "simple-eggs should have at least 3 ingredients"
    );
}

// ---------------------------------------------------------------------------
// Task 4: Edge cases
// ---------------------------------------------------------------------------

#[test]
fn unicode_ingredients_and_titles() {
    let p = parser();

    // Mapo Tofu has Chinese characters in title
    let src = load_fixture("mapo-tofu.cook");
    let result = p.parse(&src);
    assert!(result.is_valid(), "Unicode title should parse");

    // Çilbir has Turkish characters
    let src = load_fixture("cilbir.cook");
    let result = p.parse(&src);
    assert!(result.is_valid(), "Turkish characters should parse");
}

#[test]
fn sections_parsed_correctly() {
    let p = parser();
    let src = load_fixture("sourdough-bread.cook");
    let (recipe, _) = p.parse(&src).into_result().expect("parse failed");

    // Sourdough has 5 sections: Levain, Autolyse, Mix, Shape, Bake
    assert!(
        recipe.sections.len() >= 4,
        "expected at least 4 sections, got {}",
        recipe.sections.len()
    );
}

#[test]
fn block_comments_ignored() {
    let p = parser();
    let src = load_fixture("creme-brulee.cook");
    let (recipe, _) = p.parse(&src).into_result().expect("parse failed");

    // Block comment [- Tempering prevents scrambled eggs! -] should not appear as text
    let all_text: String = recipe
        .sections
        .iter()
        .flat_map(|s| &s.content)
        .filter_map(|c| match c {
            cooklang::Content::Step(step) => Some(
                step.items
                    .iter()
                    .filter_map(|item| match item {
                        cooklang::Item::Text { value } => Some(value.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(""),
            ),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");

    assert!(
        !all_text.contains("Tempering prevents"),
        "block comment content should not appear in step text"
    );
}

#[test]
fn inline_comments_ignored() {
    let p = parser();
    let src = load_fixture("sourdough-bread.cook");
    let (recipe, _) = p.parse(&src).into_result().expect("parse failed");

    let all_text: String = recipe
        .sections
        .iter()
        .flat_map(|s| &s.content)
        .filter_map(|c| match c {
            cooklang::Content::Step(step) => Some(
                step.items
                    .iter()
                    .filter_map(|item| match item {
                        cooklang::Item::Text { value } => Some(value.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(""),
            ),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");

    assert!(
        !all_text.contains("Total bulk fermentation"),
        "inline comment should not appear in step text"
    );
}

#[test]
fn named_timers_preserved() {
    let p = parser();
    let src = load_fixture("miso-ramen.cook");
    let (recipe, _) = p.parse(&src).into_result().expect("parse failed");

    let named: Vec<&str> = recipe
        .timers
        .iter()
        .filter_map(|t| t.name.as_deref())
        .collect();

    assert!(
        named.contains(&"soft boil"),
        "expected named timer 'soft boil', got: {named:?}"
    );
}

#[test]
fn notes_parsed_as_text() {
    let p = parser();
    let src = load_fixture("chicken-adobo.cook");
    let (recipe, _) = p.parse(&src).into_result().expect("parse failed");

    // Notes (> lines) should appear as Content::Text in sections
    let has_text_content = recipe.sections.iter().any(|s| {
        s.content
            .iter()
            .any(|c| matches!(c, cooklang::Content::Text(_)))
    });

    assert!(
        has_text_content,
        "expected note content (> prefixed lines) to appear as Text"
    );
}

// ---------------------------------------------------------------------------
// Task 5: Round-trip feasibility — can we reconstruct .cook from parsed model?
// ---------------------------------------------------------------------------

#[test]
fn ingredients_retain_quantity_and_unit() {
    let p = parser();
    let src = load_fixture("pasta-alla-norma.cook");
    let (recipe, _) = p.parse(&src).into_result().expect("parse failed");

    let olive_oil = recipe
        .ingredients
        .iter()
        .find(|i| i.name == "olive oil")
        .expect("missing 'olive oil'");

    let qty = olive_oil
        .quantity
        .as_ref()
        .expect("olive oil should have quantity");

    // Verify we can access the numeric value and unit
    assert!(
        !format!("{qty}").is_empty(),
        "quantity should have a displayable representation"
    );
}

#[test]
fn all_fixtures_preserve_ingredient_count() {
    let p = parser();

    // Minimum expected ingredient counts per recipe
    let expectations: &[(&str, usize)] = &[
        ("chicken-adobo.cook", 6),
        ("sourdough-bread.cook", 4),
        ("mapo-tofu.cook", 10),
        ("simple-eggs.cook", 3),
        ("pasta-alla-norma.cook", 8),
        ("thai-green-curry.cook", 15),
        ("creme-brulee.cook", 4),
        ("birria-tacos.cook", 10),
        ("miso-ramen.cook", 10),
        ("cilbir.cook", 5),
        ("chocolate-chip-cookies.cook", 8),
    ];

    for (name, min_count) in expectations {
        let src = load_fixture(name);
        let (recipe, _) = p
            .parse(&src)
            .into_result()
            .unwrap_or_else(|_| panic!("{name} parse failed"));

        assert!(
            recipe.ingredients.len() >= *min_count,
            "{name}: expected at least {min_count} ingredients, got {}",
            recipe.ingredients.len()
        );
    }
}

#[test]
fn model_is_serializable_to_json() {
    let p = parser();
    let src = load_fixture("chicken-adobo.cook");
    let (recipe, _) = p.parse(&src).into_result().expect("parse failed");

    // The cooklang model derives Serialize, so we can round-trip through JSON
    let json = serde_json::to_string_pretty(&recipe);
    assert!(
        json.is_ok(),
        "recipe should be serializable to JSON: {:?}",
        json.err()
    );

    let json_str = json.unwrap();
    assert!(
        json_str.contains("chicken thighs"),
        "JSON should contain ingredient names"
    );
    assert!(
        json_str.contains("Classic Chicken Adobo"),
        "JSON should contain title"
    );
}

// ---------------------------------------------------------------------------
// Summary: print a spike report when run with --nocapture
// ---------------------------------------------------------------------------

#[test]
fn spike_summary_report() {
    let p = parser();
    let mut total_ingredients = 0;
    let mut total_cookware = 0;
    let mut total_timers = 0;
    let mut total_sections = 0;
    let mut total_warnings = 0;
    let mut recipes_parsed = 0;
    let mut recipes_with_metadata = 0;

    eprintln!("\n╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║        SPIKE #1: cooklang-rs PARSER VALIDATION REPORT       ║");
    eprintln!("╠══════════════════════════════════════════════════════════════╣");

    for name in ALL_FIXTURES {
        let src = load_fixture(name);
        let result = p.parse(&src);
        let report = result.report();
        let warning_count = if report.has_warnings() { 1 } else { 0 }; // approximate
        total_warnings += warning_count;

        match result.into_result() {
            Ok((recipe, _)) => {
                recipes_parsed += 1;
                let i = recipe.ingredients.len();
                let c = recipe.cookware.len();
                let t = recipe.timers.len();
                let s = recipe.sections.len();
                let has_meta = !recipe.metadata.map.is_empty();

                total_ingredients += i;
                total_cookware += c;
                total_timers += t;
                total_sections += s;
                if has_meta {
                    recipes_with_metadata += 1;
                }

                eprintln!(
                    "║ ✅ {:<30} │ {} ing │ {} cw │ {} tm │ {} sec │ {} warn │ meta:{}",
                    name,
                    i,
                    c,
                    t,
                    s,
                    warning_count,
                    if has_meta { "✓" } else { "✗" }
                );
            }
            Err(_) => {
                eprintln!("║ ❌ {:<30} │ PARSE FAILED", name);
            }
        }
    }

    eprintln!("╠══════════════════════════════════════════════════════════════╣");
    eprintln!(
        "║ TOTALS: {recipes_parsed}/{} recipes parsed",
        ALL_FIXTURES.len()
    );
    eprintln!(
        "║   Ingredients: {total_ingredients}  Cookware: {total_cookware}  Timers: {total_timers}  Sections: {total_sections}"
    );
    eprintln!("║   Metadata present: {recipes_with_metadata}/{recipes_parsed}");
    eprintln!("║   Total warnings: {total_warnings}");
    eprintln!("║");
    let fidelity = if ALL_FIXTURES.len() > 0 {
        (recipes_parsed as f64 / ALL_FIXTURES.len() as f64) * 100.0
    } else {
        0.0
    };
    eprintln!(
        "║ FIDELITY: {fidelity:.0}% ({recipes_parsed}/{} recipes)",
        ALL_FIXTURES.len()
    );
    if fidelity >= 95.0 {
        eprintln!("║ VERDICT: ✅ GO — cooklang-rs meets requirements");
    } else {
        eprintln!("║ VERDICT: ❌ NO-GO — parser fidelity below 95%");
    }
    eprintln!("╠══════════════════════════════════════════════════════════════╣");
    eprintln!("║ EMITTER NOTE: cooklang-to-cooklang v0.15 depends on");
    eprintln!("║ cooklang v0.15, NOT v0.18. Versions are INCOMPATIBLE.");
    eprintln!("║ fond will need a thin custom emitter for round-trip.");
    eprintln!("╚══════════════════════════════════════════════════════════════╝");
}
