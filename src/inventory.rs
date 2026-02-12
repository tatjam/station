use axum::{
    Form,
    extract::State,
    response::{Html, IntoResponse},
};
use serde::Deserialize;
use tracing::info;

#[derive(Debug, Deserialize)]
pub struct SearchForm {
    category: String,
    footprint: String,
    min_val: String,
    max_val: String,
    search: String,
}

use crate::state::AppState;

pub async fn search_handler(
    State(state): State<AppState>,
    Form(search): Form<SearchForm>,
) -> impl IntoResponse {
    info!("Search query: {:?}", search);
    Html(format!("Hi"))
}
