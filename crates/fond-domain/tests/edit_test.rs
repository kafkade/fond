//! Tests for the `CookDocument` lossless edit layer.

use std::path::PathBuf;

use fond_domain::{Block, BlockKind, CookDocument, parse_cook};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn load_fixture(name: &str) -> String {
    std::fs::read_to_string(fixtures_dir().join(name))
        .unwrap_or_else(|e| panic!("failed to load fixture {name}: {e}"))
}

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

// ═══════════════════════════════════════════════════════════════════
// Byte-for-byte round-trip when nothing is edited
// ═══════════════════════════════════════════════════════════════════

#[test]
fn parse_emit_is_byte_identical_for_all_fixtures() {
    for name in ALL_FIXTURES {
        let content = load_fixture(name);
        let doc = CookDocument::parse(&content);
        assert_eq!(doc.emit(), content, "{name}: not byte-identical");
    }
}

// ═══════════════════════════════════════════════════════════════════
// Metadata edits preserve the body byte-for-byte
// ═══════════════════════════════════════════════════════════════════

#[test]
fn metadata_edit_preserves_body_verbatim() {
    for name in ALL_FIXTURES {
        let content = load_fixture(name);
        let mut doc = CookDocument::parse(&content);
        let before = doc.blocks().to_vec();

        doc.set_scalar("servings", &["servings"], Some("8"));

        // Body blocks are untouched by a metadata edit.
        assert_eq!(doc.blocks(), before.as_slice(), "{name}: body changed");
        // The edit is visible and re-parses.
        let reparsed = parse_cook(&doc.emit(), name.trim_end_matches(".cook")).unwrap();
        assert_eq!(reparsed.servings.as_deref(), Some("8"), "{name}");
    }
}

#[test]
fn metadata_edit_keeps_body_bytes_on_frontmatter_recipe() {
    let content = load_fixture("chicken-adobo.cook");
    let (_, body_original) = content.split_once("\n---\n").expect("frontmatter");

    let mut doc = CookDocument::parse(&content);
    doc.set_scalar("servings", &["servings"], Some("8"));
    let emitted = doc.emit();

    assert!(
        emitted.ends_with(body_original),
        "body was not preserved verbatim after metadata edit"
    );
}

#[test]
fn setting_title_changes_slug() {
    let content = load_fixture("chicken-adobo.cook");
    let mut doc = CookDocument::parse(&content);
    assert_eq!(doc.slug(), "classic-chicken-adobo");
    doc.set_scalar("title", &["title"], Some("Weeknight Adobo"));
    assert_eq!(doc.slug(), "weeknight-adobo");
    let reparsed = parse_cook(&doc.emit(), "x").unwrap();
    assert_eq!(reparsed.title, "Weeknight Adobo");
}

#[test]
fn no_op_metadata_set_keeps_document_byte_identical() {
    let content = load_fixture("chicken-adobo.cook");
    let mut doc = CookDocument::parse(&content);
    // Set to the value it already has — must not dirty formatting.
    let current_title = doc.title().unwrap();
    doc.set_scalar("title", &["title"], Some(&current_title));
    assert_eq!(doc.emit(), content);
}

#[test]
fn inserting_new_metadata_key_places_after_title() {
    let content = load_fixture("simple-eggs.cook");
    let mut doc = CookDocument::parse(&content);
    doc.set_scalar("image", &["image"], Some("photos/ab/cdef.jpg"));
    let emitted = doc.emit();
    assert!(emitted.contains("image: photos/ab/cdef.jpg"));
    // simple-eggs has no frontmatter title, so it derives from the stem.
    let reparsed = parse_cook(&emitted, "simple-eggs").unwrap();
    assert_eq!(reparsed.title, "Simple Eggs");
}

// ═══════════════════════════════════════════════════════════════════
// Tags: inline and YAML block styles
// ═══════════════════════════════════════════════════════════════════

#[test]
fn reads_block_style_tags() {
    // chicken-adobo uses a YAML block list for tags.
    let content = load_fixture("chicken-adobo.cook");
    let doc = CookDocument::parse(&content);
    assert_eq!(doc.tags(), vec!["filipino", "chicken", "comfort food"]);
}

#[test]
fn set_tags_preserves_block_style() {
    let content = load_fixture("chicken-adobo.cook");
    let mut doc = CookDocument::parse(&content);
    doc.set_tags(&["filipino".into(), "braise".into()]);
    let emitted = doc.emit();
    assert!(
        emitted.contains("tags:\n  - filipino\n  - braise"),
        "{emitted}"
    );
    let reparsed = parse_cook(&emitted, "chicken-adobo").unwrap();
    assert_eq!(reparsed.tags, vec!["filipino", "braise"]);
}

#[test]
fn set_tags_on_inline_style_recipe() {
    let raw = "---\ntitle: Test\ntags: a, b\n---\n\nDo a thing.\n";
    let mut doc = CookDocument::parse(raw);
    assert_eq!(doc.tags(), vec!["a", "b"]);
    doc.set_tags(&["c".into(), "d".into(), "e".into()]);
    assert!(doc.emit().contains("tags: c, d, e"));
}

// ═══════════════════════════════════════════════════════════════════
// Step (body block) edits
// ═══════════════════════════════════════════════════════════════════

#[test]
fn editing_a_step_preserves_other_blocks() {
    // chicken-adobo contains a `>` quote block that must survive step edits.
    let content = load_fixture("chicken-adobo.cook");
    let mut doc = CookDocument::parse(&content);

    let mut blocks: Vec<Block> = doc.blocks().to_vec();
    // Find the first step and tweak its inline ingredient markup.
    let step_idx = blocks
        .iter()
        .position(|b| b.kind == BlockKind::Step)
        .unwrap();
    blocks[step_idx] = Block::new("Combine @soy sauce{3/4%cup} and stir.");
    doc.set_blocks(blocks);

    let emitted = doc.emit();
    assert!(emitted.contains("@soy sauce{3/4%cup}"));
    // The quote block is preserved.
    assert!(
        emitted.contains("> The sauce should coat the back of a spoon."),
        "quote block lost:\n{emitted}"
    );
    // Re-parses cleanly.
    let reparsed = parse_cook(&emitted, "chicken-adobo").unwrap();
    assert!(reparsed.ingredients.iter().any(|i| i.name == "soy sauce"));
}

#[test]
fn appending_a_step_reparses_with_new_ingredient() {
    let content = load_fixture("simple-eggs.cook");
    let mut doc = CookDocument::parse(&content);
    let mut blocks = doc.blocks().to_vec();
    blocks.push(Block::new("Garnish with @chives{1%tbsp}."));
    doc.set_blocks(blocks);

    let reparsed = parse_cook(&doc.emit(), "simple-eggs").unwrap();
    assert!(reparsed.ingredients.iter().any(|i| i.name == "chives"));
}

#[test]
fn section_context_is_resolved() {
    let content = load_fixture("chocolate-chip-cookies.cook");
    let doc = CookDocument::parse(&content);
    let sectioned = doc.sectioned_blocks();
    // The first step after "= Brown Butter" is in that section.
    let brown_butter_step = sectioned
        .iter()
        .find(|b| b.kind == BlockKind::Step)
        .unwrap();
    assert_eq!(brown_butter_step.section.as_deref(), Some("Brown Butter"));
}

// ═══════════════════════════════════════════════════════════════════
// New recipes
// ═══════════════════════════════════════════════════════════════════

#[test]
fn new_recipe_is_parseable_and_lossless() {
    let doc = CookDocument::new_recipe(
        "Test Soup",
        Some("4"),
        &["soup".into(), "quick".into()],
        Some("A quick test soup."),
        Some("Me"),
        &[
            "Simmer @water{1%L} with @salt{1%tsp}.".into(),
            "Serve hot.".into(),
        ],
    );
    let emitted = doc.emit();
    let recipe = parse_cook(&emitted, "test-soup").unwrap();
    assert_eq!(recipe.title, "Test Soup");
    assert_eq!(recipe.servings.as_deref(), Some("4"));
    assert_eq!(recipe.tags, vec!["soup", "quick"]);
    assert_eq!(recipe.steps.len(), 2);
    assert!(recipe.ingredients.iter().any(|i| i.name == "water"));

    // Emitting again is stable.
    let doc2 = CookDocument::parse(&emitted);
    assert_eq!(doc2.emit(), emitted);
}

#[test]
fn title_with_colon_is_quoted_and_reparses() {
    let doc = CookDocument::new_recipe(
        "Soup: The Reckoning",
        None,
        &[],
        None,
        None,
        &["Do a thing.".into()],
    );
    let emitted = doc.emit();
    let recipe = parse_cook(&emitted, "x").unwrap();
    assert_eq!(recipe.title, "Soup: The Reckoning");
}
