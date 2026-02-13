use std::fmt::Display;

use axum::{
    Form,
    extract::State,
    response::{Html, IntoResponse},
};
use maud::html;
use serde::Deserialize;
use sqlx::{Postgres, QueryBuilder, pool::PoolConnection};
use tracing::{error, info};

const ALL_CATEGORIES_STR: &'static str = "All Categories";
const ALL_FOOTPRINTS_STR: &'static str = "All Footprints";
const NO_FOOTPRINT_STR: &'static str = "No Footprint";

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
    id: i64,
    mpn: Option<String>,
    category: String,
    footprint: Option<String>,
    value: Option<f32>,
    location: Option<String>,
    quantity: i64,
    comments: Option<String>,
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

async fn query_inventory(
    search: &SearchForm,
    db_conn: &mut PoolConnection<Postgres>,
) -> Result<Vec<InventoryItem>, sqlx::Error> {
    let mut query = QueryBuilder::new("SELECT * FROM inventory WHERE 1=1");
    if search.category != ALL_CATEGORIES_STR && !search.category.is_empty() {
        query.push(" AND category = ");
        query.push_bind(&search.category);
    }

    if search.footprint != ALL_FOOTPRINTS_STR && !search.footprint.is_empty() {
        if search.footprint == NO_FOOTPRINT_STR {
            query.push(" AND footprint IS NULL");
        } else {
            query.push(" AND footprint = ");
            query.push_bind(&search.footprint);
        }
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
        query.push(" OR comments ILIKE ");
        query.push_bind(format!("%{}%", search.search));
        query.push(")");
    }

    query.push(" LIMIT 100");

    let sql = query.sql();

    info!("Database query: {}", sql);

    query
        .build_query_as::<InventoryItem>()
        .fetch_all(db_conn.as_mut())
        .await
}

fn format_mult_value(value: f32) -> String {
    if value < 1e-9 {
        format!("{:.2} p", value * 1e12)
    } else if value < 1e-6 {
        format!("{:.2} n", value * 1e9)
    } else if value < 1e-3 {
        format!("{:.2} µ", value * 1e6)
    } else if value < 1e0 {
        format!("{:.2} m", value * 1e3)
    } else if value < 1e3 {
        format!("{:.2}  ", value * 1e0)
    } else if value < 1e6 {
        format!("{:.2} k", value * 1e-3)
    } else if value < 1e9 {
        format!("{:.2} M", value * 1e-6)
    } else {
        format!("{:.2} G", value * 1e-9)
    }
}
fn format_value(category: &String, value: f32) -> String {
    let (unit, mult) = match category.as_str() {
        "CapCeramic" => ("F", true),
        "CapElectro" => ("F", true),
        "Resistor" => ("Ω", true),
        "Inductor" => ("H", true),
        _ => ("", false),
    };
    let value = if mult {
        format_mult_value(value)
    } else {
        format!("{:.2}  ", value)
    };

    format!("{}{}", value, unit)
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

    let results = match query_inventory(&search, &mut db_conn).await {
        Ok(results) => results,
        Err(e) => {
            return handle_generic_inventory_error(e);
        }
    };

    let response = html! {
        table class="striped" {
            thead {
                tr {
                    th id="sort-mpn" onclick="sortBy('mpn')" style="cursor: pointer; font-weight: normal" scope="col" {
                        "MPN"
                    }
                    th id="sort-category" onclick="sortBy('category')" style="cursor: pointer; font-weight: normal" scope="col" {
                        "Category"
                    }
                    th id="sort-footprint" onclick="sortBy('footprint')" style="cursor: pointer; font-weight: normal" scope="col" {
                        "Footprint"
                    }
                    th id="sort-value" onclick="sortBy('value')" style="cursor: pointer; font-weight: normal" scope="col" {
                        "Value"
                    }
                    th id="sort-quantity" onclick="sortBy('quantity')" style="cursor: pointer; font-weight: normal" scope="col" {
                        "Qty."
                    }
                    th {
                        "Action"
                    }
                }
            }
            @for result in &results {
                tr {
                    th scope="row" {
                        @if let Some(mpn) = &result.mpn {
                            (mpn)
                        } @else {
                            "—"
                        }
                    }
                    td {
                        (result.category)
                    }
                    td {
                        @if let Some(footprint) = &result.footprint {
                            (footprint)
                        } @else {
                            "—"
                        }
                    }
                    td style="text-align: right; font-family: monospace; font-size: 1.3em; white-space: pre; width: 1%" {
                        @if let Some(value) = result.value {
                            (format_value(&result.category, value))
                        } @else {
                            "—"
                        }
                    }
                    td {
                        (result.quantity)
                    }
                    td {
                        "TODO"
                    }
                }
            }
        }
    }
    .into_string();

    Html(response)
}
