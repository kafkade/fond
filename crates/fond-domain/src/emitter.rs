use crate::recipe::Recipe;

/// Emit a `.cook` file from a domain [`Recipe`].
///
/// If the recipe has a preserved `raw_source`, returns it as-is
/// (user-authored files are never silently rewritten).
///
/// For generated or imported recipes without `raw_source`, builds
/// Cooklang-formatted text with YAML-style frontmatter and step text.
pub fn emit_cook(recipe: &Recipe) -> String {
    // Preserve original source when available
    if let Some(ref source) = recipe.raw_source {
        return source.clone();
    }

    let mut out = String::new();

    // Frontmatter
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
    out.push_str("---\n\n");

    // Steps — grouped by section
    let mut current_section: Option<&str> = None;
    for step in &recipe.steps {
        let step_section = step.section.as_deref();
        if step_section != current_section {
            if let Some(name) = step_section
                && !name.is_empty()
            {
                out.push_str(&format!("== {name} ==\n\n"));
            }
            current_section = step_section;
        }
        // For generated recipes, emit plain text steps with inline annotations
        // where possible. This produces readable .cook files even though they
        // may not perfectly round-trip annotations.
        out.push_str(&emit_step_text(recipe, step));
        out.push_str("\n\n");
    }

    out.trim_end().to_string() + "\n"
}

/// Emit a single step's text with inline Cooklang annotations.
///
/// For imported recipes we reconstruct annotations from the structured
/// ingredient/timer data. This is best-effort — the result is valid
/// Cooklang but may differ from hand-authored formatting.
fn emit_step_text(recipe: &Recipe, step: &crate::recipe::Step) -> String {
    let mut text = step.body.clone();

    // Try to annotate known ingredients that appear in the step text
    for ing in &recipe.ingredients {
        if let Some(pos) = text.find(&ing.name) {
            let annotation = format_ingredient(ing);
            text = format!(
                "{}{}{}",
                &text[..pos],
                annotation,
                &text[pos + ing.name.len()..]
            );
        }
    }

    text
}

fn format_ingredient(ing: &crate::recipe::RecipeIngredient) -> String {
    match (&ing.quantity, &ing.unit) {
        (Some(qty), Some(unit)) => format!("@{}{{{}%{}}}", ing.name, qty, unit),
        (Some(qty), None) => format!("@{}{{{}}} ", ing.name, qty),
        _ => format!("@{}{{}}", ing.name),
    }
}
