use crate::paprika::parse_single_ingredient;
use crate::pipeline::ReviewDraft;

pub fn build_review_draft(ocr_text: &str, source_name: &str) -> ReviewDraft {
    let normalized = normalize_lines(ocr_text);
    let mut warnings = Vec::new();

    let title = guess_title(&normalized, source_name);
    let parsed = partition_sections(&normalized, &title, &mut warnings);

    if parsed.ingredients.is_empty() {
        warnings.push("No ingredient section was confidently identified.".to_string());
    }
    if parsed.steps.is_empty() {
        warnings.push("No instruction section was confidently identified.".to_string());
    }

    ReviewDraft {
        title: title.clone(),
        source_name: source_name.to_string(),
        cook_text: emit_ocr_cook(
            &title,
            source_name,
            &parsed.ingredients,
            &parsed.steps,
            &parsed.notes,
        ),
        raw_text: ocr_text.trim().to_string(),
        warnings,
    }
}

struct ParsedSections {
    ingredients: Vec<String>,
    steps: Vec<String>,
    notes: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Unknown,
    Ingredients,
    Steps,
    Notes,
}

fn normalize_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(clean_line)
        .filter(|line| !line.is_empty())
        .collect()
}

fn clean_line(line: &str) -> String {
    let trimmed = line.trim();
    let trimmed = trimmed
        .trim_start_matches(|c: char| {
            matches!(
                c,
                '-' | '*' | '\u{2022}' | '\u{2023}' | '\u{25E6}' | '\u{2043}'
            )
        })
        .trim();

    collapse_whitespace(trimmed)
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn guess_title(lines: &[String], source_name: &str) -> String {
    for line in lines {
        if is_heading(line) || looks_like_step(line) {
            continue;
        }
        if line.len() >= 3 {
            return strip_trailing_punctuation(line);
        }
    }

    title_from_source_name(source_name)
}

fn title_from_source_name(source_name: &str) -> String {
    let stem = source_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(source_name);
    let words = stem
        .split(['-', '_', '.'])
        .filter(|part| !part.is_empty())
        .map(capitalize_word)
        .collect::<Vec<_>>();

    if words.is_empty() {
        "Imported Recipe".to_string()
    } else {
        words.join(" ")
    }
}

fn capitalize_word(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

fn strip_trailing_punctuation(line: &str) -> String {
    line.trim_end_matches(['.', ':']).trim().to_string()
}

fn partition_sections(lines: &[String], title: &str, warnings: &mut Vec<String>) -> ParsedSections {
    let mut section = Section::Unknown;
    let mut leading = Vec::new();
    let mut ingredients = Vec::new();
    let mut steps = Vec::new();
    let mut notes = Vec::new();
    let mut skipped_title = false;
    let mut saw_ingredient_heading = false;
    let mut saw_step_heading = false;

    for line in lines {
        if !skipped_title && line == title {
            skipped_title = true;
            continue;
        }

        if let Some(next_section) = heading_section(line) {
            match next_section {
                Section::Ingredients => saw_ingredient_heading = true,
                Section::Steps => saw_step_heading = true,
                Section::Notes | Section::Unknown => {}
            }
            section = next_section;
            continue;
        }

        match section {
            Section::Ingredients => ingredients.push(line.clone()),
            Section::Steps => push_steps(line, &mut steps),
            Section::Notes => notes.push(line.clone()),
            Section::Unknown => leading.push(line.clone()),
        }
    }

    if saw_step_heading && !saw_ingredient_heading {
        for line in leading {
            if looks_like_ingredient_line(&line) {
                ingredients.push(line);
            } else {
                push_steps(&line, &mut steps);
            }
        }
        warnings.push(
            "Ingredients heading not found; leading lines were classified heuristically."
                .to_string(),
        );
    } else if !saw_step_heading && !saw_ingredient_heading {
        for line in leading {
            if looks_like_ingredient_line(&line) {
                ingredients.push(line);
            } else {
                push_steps(&line, &mut steps);
            }
        }
        warnings.push(
            "OCR text did not include clear section headings; ingredients and steps were inferred heuristically."
                .to_string(),
        );
    } else if saw_ingredient_heading && !saw_step_heading {
        ingredients.extend(leading);
        warnings.push(
            "Instruction heading not found; OCR text after ingredients may need manual cleanup."
                .to_string(),
        );
    } else {
        ingredients.extend(
            leading
                .into_iter()
                .filter(|line| looks_like_ingredient_line(line)),
        );
    }

    ParsedSections {
        ingredients,
        steps,
        notes,
    }
}

fn heading_section(line: &str) -> Option<Section> {
    let normalized = normalize_heading(line);
    match normalized.as_str() {
        "ingredients" | "ingredient" => Some(Section::Ingredients),
        "directions" | "direction" | "instructions" | "instruction" | "method" | "preparation"
        | "prep" => Some(Section::Steps),
        "notes" | "note" => Some(Section::Notes),
        _ => None,
    }
}

fn normalize_heading(line: &str) -> String {
    line.chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphabetic() {
                Some(ch.to_ascii_lowercase())
            } else {
                None
            }
        })
        .collect()
}

fn is_heading(line: &str) -> bool {
    heading_section(line).is_some()
}

fn looks_like_ingredient_line(line: &str) -> bool {
    let parsed = parse_single_ingredient(line);
    if parsed.quantity.is_some() || parsed.unit.is_some() {
        return true;
    }

    let lower = line.to_ascii_lowercase();
    const UNITS: &[&str] = &[
        "cup",
        "cups",
        "tbsp",
        "tsp",
        "teaspoon",
        "tablespoon",
        "oz",
        "ounce",
        "ounces",
        "lb",
        "lbs",
        "gram",
        "grams",
        "g",
        "kg",
        "ml",
        "l",
        "pinch",
        "clove",
        "cloves",
    ];

    UNITS.iter().any(|unit| lower.contains(unit))
        || (!looks_like_step(line) && line.len() <= 48 && !line.contains('.'))
}

fn looks_like_step(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    line.ends_with('.')
        || line.contains(". ")
        || lower.starts_with("mix ")
        || lower.starts_with("stir ")
        || lower.starts_with("add ")
        || lower.starts_with("bake ")
        || lower.starts_with("cook ")
        || lower.starts_with("preheat ")
        || lower.starts_with("heat ")
        || lower.starts_with("serve ")
}

fn push_steps(line: &str, steps: &mut Vec<String>) {
    let cleaned = line
        .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ')' || c == ' ')
        .trim();
    if !cleaned.is_empty() {
        steps.push(cleaned.to_string());
    }
}

fn emit_ocr_cook(
    title: &str,
    source_name: &str,
    ingredients: &[String],
    steps: &[String],
    notes: &[String],
) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("title: {title}\n"));
    out.push_str("import source: ocr-photo\n");
    out.push_str("import confidence: review-required\n");
    out.push_str(&format!("source image: {source_name}\n"));
    out.push_str("---\n\n");

    if !ingredients.is_empty() {
        for line in ingredients {
            let ingredient = parse_single_ingredient(line);
            out.push_str(&format_ingredient_line(&ingredient));
            out.push('\n');
        }
        out.push('\n');
    }

    if !steps.is_empty() {
        for line in steps {
            out.push_str(line);
            out.push_str("\n\n");
        }
    } else {
        out.push_str("Review the OCR text and add preparation steps here.\n");
    }

    if !notes.is_empty() {
        out.push_str("-- Notes --\n\n");
        for line in notes {
            out.push_str(line);
            out.push('\n');
        }
    }

    format!("{}\n", out.trim_end())
}

fn format_ingredient_line(ing: &fond_domain::RecipeIngredient) -> String {
    match (&ing.quantity, &ing.unit) {
        (Some(qty), Some(unit)) => format!("@{}{{{}%{}}}", ing.name, qty, unit),
        (Some(qty), None) => format!("@{}{{{}}}", ing.name, qty),
        _ => format!("@{}{{}}", ing.name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_draft_from_headed_recipe_text() {
        let text = "\
Grandma Pancakes

Ingredients
2 cups flour
1 tbsp sugar
1 tsp salt

Directions
Mix the dry ingredients.
Whisk in the milk and eggs.
Bake until golden.
";

        let draft = build_review_draft(text, "grandma-pancakes.jpg");
        assert_eq!(draft.title, "Grandma Pancakes");
        assert!(draft.cook_text.contains("@flour{2%cups}"));
        assert!(draft.cook_text.contains("Mix the dry ingredients."));
        assert!(draft.warnings.is_empty());
    }

    #[test]
    fn falls_back_to_heuristics_without_headings() {
        let text = "\
Weekend Waffles
2 cups flour
2 eggs
Mix everything together.
Cook in a waffle iron.
";

        let draft = build_review_draft(text, "weekend-waffles.jpg");
        assert_eq!(draft.title, "Weekend Waffles");
        assert!(draft.cook_text.contains("@flour{2%cups}"));
        assert!(draft.cook_text.contains("Cook in a waffle iron."));
        assert!(!draft.warnings.is_empty());
    }
}
