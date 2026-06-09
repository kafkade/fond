use askama::Template;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use serde::Deserialize;

use fond_domain::RecipeFilter;
use fond_store::{RecipeRecord, RecipeRepository, RecipeSummary, TagCount};

use crate::error::WebError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/recipes", get(recipe_list))
        .route("/recipes/{slug}", get(recipe_detail))
        .route("/search", get(search))
        .route("/tags", get(tags))
}

// ── Query params ──────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub struct SearchQuery {
    q: Option<String>,
    tag: Option<String>,
}

// ── Templates ─────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    recipes: Vec<RecipeSummary>,
    #[allow(dead_code)]
    tags: Vec<TagCount>,
    recipe_count: usize,
    query: String,
}

#[derive(Template)]
#[template(path = "recipe_list_partial.html")]
struct RecipeListPartialTemplate {
    recipes: Vec<RecipeSummary>,
    #[allow(dead_code)]
    recipe_count: usize,
    query: String,
}

#[derive(Template)]
#[template(path = "recipe_detail.html")]
struct RecipeDetailTemplate {
    recipe: RecipeRecord,
    tags: Vec<String>,
    ingredients: Vec<IngredientView>,
    steps: Vec<StepView>,
    notes: Vec<NoteView>,
    rating: Option<f64>,
}

struct IngredientView {
    name: String,
    quantity: String,
    unit: String,
    note: String,
    optional: bool,
}

struct StepView {
    number: u32,
    section: String,
    body: String,
}

struct NoteView {
    body: String,
    created_at: String,
}

#[derive(Template)]
#[template(path = "tags.html")]
struct TagsTemplate {
    tags: Vec<TagCount>,
}

fn render<T: Template>(tmpl: T) -> Result<Html<String>, WebError> {
    tmpl.render()
        .map(Html)
        .map_err(|e| WebError::internal(format!("template error: {e}")))
}

// ── Handlers ──────────────────────────────────────────────────────

async fn index(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<impl IntoResponse, WebError> {
    let q = params.q.unwrap_or_default();
    let tag = params.tag;

    let (recipes, tags) = state.with_db(|db| {
        let repo = RecipeRepository::new(db);
        let recipes = if q.is_empty() {
            let filter = RecipeFilter {
                tags: tag.iter().cloned().collect(),
                ..Default::default()
            };
            repo.list_recipes_filtered(&filter).unwrap_or_default()
        } else {
            let filter = RecipeFilter {
                tags: tag.iter().cloned().collect(),
                ..Default::default()
            };
            let results = repo
                .search_filtered(&fond_domain::escape_fts5_query(&q), &filter)
                .unwrap_or_default();
            results
                .into_iter()
                .map(|r| RecipeSummary {
                    id: r.recipe_id,
                    slug: r.slug,
                    title: r.title,
                    source: r.source,
                    tags: r.tags,
                    total_time: String::new(),
                    total_time_minutes: None,
                })
                .collect()
        };
        let tags = repo.list_tags().unwrap_or_default();
        (recipes, tags)
    });

    let recipe_count = recipes.len();
    render(IndexTemplate {
        recipes,
        tags,
        recipe_count,
        query: q,
    })
}

async fn recipe_list(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<impl IntoResponse, WebError> {
    let q = params.q.unwrap_or_default();
    let tag = params.tag;

    let recipes = state.with_db(|db| {
        let repo = RecipeRepository::new(db);
        if q.is_empty() {
            let filter = RecipeFilter {
                tags: tag.iter().cloned().collect(),
                ..Default::default()
            };
            repo.list_recipes_filtered(&filter).unwrap_or_default()
        } else {
            let filter = RecipeFilter {
                tags: tag.iter().cloned().collect(),
                ..Default::default()
            };
            let results = repo
                .search_filtered(&fond_domain::escape_fts5_query(&q), &filter)
                .unwrap_or_default();
            results
                .into_iter()
                .map(|r| RecipeSummary {
                    id: r.recipe_id,
                    slug: r.slug,
                    title: r.title,
                    source: r.source,
                    tags: r.tags,
                    total_time: String::new(),
                    total_time_minutes: None,
                })
                .collect()
        }
    });

    let recipe_count = recipes.len();
    render(RecipeListPartialTemplate {
        recipes,
        recipe_count,
        query: q,
    })
}

async fn recipe_detail(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, WebError> {
    let result = state.with_db(|db| {
        let repo = RecipeRepository::new(db);

        let recipe = repo.get_recipe_by_slug(&slug).map_err(WebError::from)?;
        let recipe =
            recipe.ok_or_else(|| WebError::not_found(format!("Recipe '{slug}' not found")))?;

        // Get tags
        let tags = repo
            .get_tags_for_slug(&slug)
            .unwrap_or(None)
            .map(|(_, t)| t)
            .unwrap_or_default();

        // Get ingredients from DB
        let ingredients = {
            let conn = db.conn();
            let mut stmt = conn
                .prepare(
                    "SELECT name, quantity, unit, note, optional
                     FROM recipe_ingredients
                     WHERE recipe_id = ?1
                     ORDER BY sort_order",
                )
                .unwrap();
            stmt.query_map(rusqlite::params![recipe.id], |row| {
                Ok(IngredientView {
                    name: row.get(0)?,
                    quantity: row.get(1)?,
                    unit: row.get(2)?,
                    note: row.get(3)?,
                    optional: row.get::<_, i32>(4)? != 0,
                })
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>()
        };

        // Get steps from DB
        let steps = {
            let conn = db.conn();
            let mut stmt = conn
                .prepare(
                    "SELECT section, body, sort_order
                     FROM steps
                     WHERE recipe_id = ?1
                     ORDER BY sort_order",
                )
                .unwrap();
            stmt.query_map(rusqlite::params![recipe.id], |row| {
                Ok(StepView {
                    section: row.get(0)?,
                    body: row.get(1)?,
                    number: row.get::<_, i32>(2)? as u32 + 1,
                })
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>()
        };

        // Get notes
        let notes = {
            let note_repo = fond_store::NoteRepository::new(db);
            note_repo
                .list_for_recipe(recipe.id, None)
                .unwrap_or_default()
                .into_iter()
                .map(|n| NoteView {
                    body: n.body,
                    created_at: n.created_at,
                })
                .collect::<Vec<_>>()
        };

        // Get average rating
        let rating = {
            let conn = db.conn();
            conn.query_row(
                "SELECT AVG(CAST(score AS REAL)) FROM ratings WHERE recipe_id = ?1",
                rusqlite::params![recipe.id],
                |row| row.get::<_, Option<f64>>(0),
            )
            .unwrap_or(None)
        };

        Ok::<_, WebError>((recipe, tags, ingredients, steps, notes, rating))
    })?;

    let (recipe, tags, ingredients, steps, notes, rating) = result;

    render(RecipeDetailTemplate {
        recipe,
        tags,
        ingredients,
        steps,
        notes,
        rating,
    })
}

async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<impl IntoResponse, WebError> {
    recipe_list(State(state), Query(params)).await
}

async fn tags(State(state): State<AppState>) -> Result<impl IntoResponse, WebError> {
    let tags = state.with_db(|db| {
        let repo = RecipeRepository::new(db);
        repo.list_tags().unwrap_or_default()
    });

    render(TagsTemplate { tags })
}
