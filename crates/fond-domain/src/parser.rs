use chrono::Utc;
use cooklang::{CooklangParser, Extensions};

use crate::error::DomainError;
use crate::recipe::{Cookware, Recipe, RecipeIngredient, Step, Timer};
use crate::slug::{slugify, title_from_stem};

/// Parse a `.cook` file's content into a domain [`Recipe`].
///
/// The original source is preserved in [`Recipe::raw_source`] so that
/// user-authored files can be written back without data loss.
///
/// If the file lacks a `title` metadata key, the title is derived
/// from `file_stem` (e.g., `"chicken-adobo"` → `"Chicken Adobo"`).
pub fn parse_cook(content: &str, file_stem: &str) -> Result<Recipe, DomainError> {
    let parser = CooklangParser::new(Extensions::all(), Default::default());
    let result = parser.parse(content);
    let scaled = result
        .into_output()
        .ok_or_else(|| DomainError::ParseCooklang {
            message: "failed to produce a valid recipe".into(),
        })?;

    let meta = &scaled.metadata;
    let get = |key: &str| -> Option<String> {
        meta.map
            .get(key)
            .and_then(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .or_else(|| v.as_u64().map(|n| n.to_string()))
                    .or_else(|| v.as_f64().map(|n| n.to_string()))
            })
            .filter(|s| !s.is_empty())
    };

    let title = get("title").unwrap_or_else(|| title_from_stem(file_stem));
    let slug = slugify(&title);

    // Extract prep_time with fallback key
    let prep_time = get("prep time").or_else(|| get("prep_time"));
    let cook_time = get("cook time").or_else(|| get("cook_time"));
    let total_time = get("total time").or_else(|| get("total_time"));

    let tags: Vec<String> = meta
        .tags()
        .map(|ts| ts.into_iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    // Extract ingredients
    let ingredients: Vec<RecipeIngredient> = scaled
        .ingredients
        .iter()
        .map(|ing| {
            let (quantity, unit) = match &ing.quantity {
                Some(q) => {
                    let qty_str = format!("{}", q.value());
                    let unit_str = q.unit().map(|u| u.to_string());
                    (
                        if qty_str.is_empty() {
                            None
                        } else {
                            Some(qty_str)
                        },
                        unit_str,
                    )
                }
                None => (None, None),
            };
            RecipeIngredient {
                name: ing.name.clone(),
                quantity,
                unit,
                note: ing.note.clone(),
                optional: false,
            }
        })
        .collect();

    // Extract cookware
    let cookware: Vec<Cookware> = scaled
        .cookware
        .iter()
        .map(|cw| Cookware {
            name: cw.name.clone(),
            quantity: cw.quantity.as_ref().map(|q| format!("{}", q.value())),
        })
        .collect();

    // Extract steps with timers
    let mut steps = Vec::new();
    let mut order = 0u32;
    for section in &scaled.sections {
        let section_name = section.name.clone();
        for item in &section.content {
            match item {
                cooklang::Content::Step(step) => {
                    let mut body = String::new();
                    let mut timers = Vec::new();

                    for si in &step.items {
                        match si {
                            cooklang::Item::Text { value } => body.push_str(value),
                            cooklang::Item::Ingredient { index } => {
                                if let Some(ing) = scaled.ingredients.get(*index) {
                                    body.push_str(&ing.name);
                                }
                            }
                            cooklang::Item::Cookware { index } => {
                                if let Some(cw) = scaled.cookware.get(*index) {
                                    body.push_str(&cw.name);
                                }
                            }
                            cooklang::Item::Timer { index } => {
                                if let Some(t) = scaled.timers.get(*index) {
                                    let duration = t.quantity.as_ref().map(|q| format!("{q}"));
                                    let name = t.name.clone();
                                    if let Some(d) = &duration {
                                        body.push_str(d);
                                    } else if let Some(n) = &name {
                                        body.push_str(n);
                                    }
                                    timers.push(Timer { name, duration });
                                }
                            }
                            _ => {}
                        }
                    }

                    steps.push(Step {
                        section: section_name.clone(),
                        body,
                        timers,
                        order,
                    });
                    order += 1;
                }
                cooklang::Content::Text(text) => {
                    steps.push(Step {
                        section: section_name.clone(),
                        body: text.clone(),
                        timers: Vec::new(),
                        order,
                    });
                    order += 1;
                }
            }
        }
    }

    let now = Utc::now();

    Ok(Recipe {
        slug,
        title,
        source: get("source"),
        source_url: get("source_url").or_else(|| get("source url")),
        description: get("description"),
        recipe_yield: get("yield"),
        prep_time,
        cook_time,
        total_time,
        servings: get("servings"),
        ingredients,
        steps,
        cookware,
        tags,
        created_at: now,
        updated_at: now,
        raw_source: Some(content.to_string()),
    })
}
