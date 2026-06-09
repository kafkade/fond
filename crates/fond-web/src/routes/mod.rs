mod grocery;
mod meal_plan;
mod recipes;

use axum::Router;

use crate::state::AppState;

/// Build the complete router with all route groups.
pub fn build(state: AppState) -> Router {
    Router::new()
        .merge(recipes::router())
        .merge(meal_plan::router())
        .merge(grocery::router())
        .with_state(state)
}
