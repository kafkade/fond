use askama::Template;
use axum::Router;
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::routing::get;

use fond_store::{MealPlanRecord, MealPlanRepository, MealPlanSummary};

use crate::error::WebError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/plans", get(plan_list))
        .route("/plans/{name}", get(plan_detail))
}

// ── Templates ─────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "plan_list.html")]
struct PlanListTemplate {
    plans: Vec<MealPlanSummary>,
}

#[derive(Template)]
#[template(path = "plan_detail.html")]
struct PlanDetailTemplate {
    plan: MealPlanRecord,
}

fn render<T: Template>(tmpl: T) -> Result<Html<String>, WebError> {
    tmpl.render()
        .map(Html)
        .map_err(|e| WebError::internal(format!("template error: {e}")))
}

// ── Handlers ──────────────────────────────────────────────────────

async fn plan_list(State(state): State<AppState>) -> Result<impl IntoResponse, WebError> {
    let plans = state.with_db(|db| {
        let repo = MealPlanRepository::new(db);
        repo.list_plans().unwrap_or_default()
    });

    render(PlanListTemplate { plans })
}

async fn plan_detail(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<impl IntoResponse, WebError> {
    let plan = state.with_db(|db| {
        let repo = MealPlanRepository::new(db);
        repo.get_plan(&name).map_err(WebError::from)
    })?;

    let plan = plan.ok_or_else(|| WebError::not_found(format!("Meal plan '{name}' not found")))?;

    render(PlanDetailTemplate { plan })
}
