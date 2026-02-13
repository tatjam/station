use std::fmt::Display;

use axum::{
    Form,
    extract::State,
    response::{Html, IntoResponse},
};
use maud::html;
use serde::Deserialize;
use sqlx::QueryBuilder;
use tracing::{error, info};

const ALL_CATEGORIES_STR: &'static str = "All Categories";
const ALL_FOOTPRINTS_STR: &'static str = "All Footprints";

#[derive(Debug, Deserialize)]
pub struct SearchForm {
    category: String,
    footprint: String,
    min_val: String,
    max_val: String,
    in_stock: String,
    search: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct InventoryItem {
    id: i32,
    mpn: String,
    category: String,
    footprint: String,
    value: f64,
    location: String,
    quantity: i32,
    comment: String,
}

use crate::state::AppState;

pub fn handle_generic_inventory_error<E: Display>(e: E) -> Html<String> {
    error!("Error while processing inventory API call: {}", e);
    return Html(
        html! {
            article {
                "Error while processing, try again later."
            }
        }
        .into_string(),
    );
}

pub async fn search_handler(
    State(state): State<AppState>,
    Form(search): Form<SearchForm>,
) -> impl IntoResponse {
    info!("Performing search query: {:?}", search);

    let mut db_conn = match state.pool.acquire().await {
        Ok(conn) => conn,
        Err(e) => {
            return handle_generic_inventory_error(e);
        }
    };

    let mut query = QueryBuilder::new("SELECT * FROM inventory WHERE 1=1");
    if search.category != ALL_CATEGORIES_STR && !search.category.is_empty() {
        query.push(" AND category = ");
        query.push_bind(&search.category);
    }

    if search.footprint != ALL_FOOTPRINTS_STR && !search.footprint.is_empty() {
        query.push(" AND footprint = ");
        query.push_bind(&search.category);
    }

    if search.in_stock == "on" {
        query.push(" AND quantity > 0");
    }

    if !search.min_val.is_empty() {
        if let Ok(min) = search.min_val.parse::<f64>() {
            query.push(" AND value >= ");
            query.push_bind(min);
        }
    }

    if !search.max_val.is_empty() {
        if let Ok(max) = search.max_val.parse::<f64>() {
            query.push(" AND value <= ");
            query.push_bind(max);
        }
    }

    if !search.search.is_empty() {
        query.push(" AND (mpn ILIKE ");
        query.push_bind(format!("%{}%", search.search));
        query.push(" OR category ILIKE ");
        query.push_bind(format!("%{}%", search.search));
        query.push(" OR comment ILIKE ");
        query.push_bind(format!("%{}%", search.search));
        query.push(")");
    }

    query.push(" LIMIT 100");

    let sql = query.sql();

    info!("Database query: {}", sql);

    let results = match query
        .build_query_as::<InventoryItem>()
        .fetch_all(&mut *db_conn)
        .await
    {
        Ok(items) => items,
        Err(e) => return handle_generic_inventory_error(e),
    };

    Html(format!("Hi"))
}
