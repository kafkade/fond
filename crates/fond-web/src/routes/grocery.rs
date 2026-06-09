use askama::Template;
use axum::Router;
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use serde::Deserialize;

use fond_store::{GroceryList, GroceryRepository};

use crate::error::WebError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/grocery", get(grocery_from_recipe))
}

#[derive(Deserialize)]
pub struct GroceryQuery {
    recipe: Option<String>,
    #[allow(dead_code)]
    plan: Option<String>,
}

#[derive(Template)]
#[template(path = "grocery.html")]
struct GroceryTemplate {
    list: Option<GroceryList>,
    recipe_slug: String,
    error: Option<String>,
}

fn render<T: Template>(tmpl: T) -> Result<Html<String>, WebError> {
    tmpl.render()
        .map(Html)
        .map_err(|e| WebError::internal(format!("template error: {e}")))
}

async fn grocery_from_recipe(
    State(state): State<AppState>,
    Query(params): Query<GroceryQuery>,
) -> Result<impl IntoResponse, WebError> {
    let slug = params.recipe.unwrap_or_default();

    if slug.is_empty() {
        return render(GroceryTemplate {
            list: None,
            recipe_slug: String::new(),
            error: None,
        });
    }

    let result = state.with_db(|db| {
        let repo = GroceryRepository::new(db);
        repo.from_recipe(&slug, true)
    });

    match result {
        Ok(list) => render(GroceryTemplate {
            list,
            recipe_slug: slug,
            error: None,
        }),
        Err(e) => render(GroceryTemplate {
            list: None,
            recipe_slug: slug,
            error: Some(format!("{e}")),
        }),
    }
}
