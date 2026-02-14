use std::fmt::Display;

use axum::{
    Form,
    extract::State,
    response::{Html, IntoResponse},
};
use maud::{Markup, html};
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
    in_stock: Option<String>,
    search: String,
    sort: String,
    dir: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct InventoryItem {
    id: i64,
    mpn: Option<String>,
    category: String,
    footprint: Option<String>,
    value: Option<f32>,
    location: Option<String>,
    quantity: Option<i64>,
    comments: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FootprintAndCategoryForm {
    footprint: String,
    category: String,
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

    if search.in_stock.is_some() {
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

    match search.sort.as_str() {
        "mpn" => query.push(" ORDER BY mpn"),
        "category" => query.push(" ORDER BY category"),
        "footprint" => query.push(" ORDER BY footprint"),
        "value" => query.push(" ORDER BY value"),
        "quantity" => query.push(" ORDER BY quantity"),
        _ => query.push(" ORDER BY mpn"),
    };

    match search.dir.as_str() {
        "asc" => query.push(" ASC"),
        _ => query.push(" DESC"),
    };

    query.push(" LIMIT 100");

    let sql = query.sql();

    info!("Database query: {}", sql);

    query
        .build_query_as::<InventoryItem>()
        .fetch_all(db_conn.as_mut())
        .await
}

fn format_mult_value(value: f32) -> String {
    if value < 1e-21 {
        // (0 but with floating point precision!)
        format!("{:.2}  ", value)
    } else if value < 1e-9 {
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

pub async fn category_list_handler(
    State(state): State<AppState>,
    Form(fandc): Form<FootprintAndCategoryForm>,
) -> impl IntoResponse {
    info!("Performing category list query");

    let mut db_conn = match state.pool.acquire().await {
        Ok(conn) => conn,
        Err(e) => {
            return handle_generic_inventory_error(e);
        }
    };

    let mut query = QueryBuilder::new("SELECT DISTINCT category FROM inventory");
    if fandc.footprint != "All Footprints" {
        query.push(" WHERE footprint = ");
        query.push_bind(fandc.footprint);
    }

    let results = match query
        .build_query_scalar::<String>()
        .fetch_all(db_conn.as_mut())
        .await
    {
        Ok(results) => results,
        Err(e) => {
            return handle_generic_inventory_error(e);
        }
    };

    Html(
        html! {
            option {
                "All Categories"
            }
            @for cat in &results {
                option {
                    (cat)
                }
            }
        }
        .into_string(),
    )
}

pub async fn footprint_list_handler(
    State(state): State<AppState>,
    Form(fandc): Form<FootprintAndCategoryForm>,
) -> impl IntoResponse {
    info!("Performing footprint list query");

    let mut db_conn = match state.pool.acquire().await {
        Ok(conn) => conn,
        Err(e) => {
            return handle_generic_inventory_error(e);
        }
    };

    let mut query = QueryBuilder::new("SELECT DISTINCT footprint FROM inventory");
    if fandc.category != "All Categories" {
        query.push(" WHERE category = ");
        query.push_bind(fandc.category);
    }

    let results = match query
        .build_query_scalar::<Option<String>>()
        .fetch_all(db_conn.as_mut())
        .await
    {
        Ok(results) => results,
        Err(e) => {
            return handle_generic_inventory_error(e);
        }
    };

    Html(
        html! {
            option {
                "All Footprints"
            }
            @for maybe_fpt in &results {
                @if let Some(fpt) = maybe_fpt {
                    option {
                        (fpt)
                    }
                }
            }
        }
        .into_string(),
    )
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
            (html_table_header(&search.sort))
            @for result in &results {
                (html_table_row(result))
            }
        }
    }
    .into_string();

    Html(response)
}

pub fn html_table_header_row(id: &'static str, content: &'static str, sort: &String) -> Markup {
    let style_str = format!(
        "cursor: pointer; {}",
        if sort == id {
            ""
        } else {
            "font-weight: normal"
        }
    );

    html! {
        th
            id={"sort-" (id)}
            onclick={"sortBy('" (id) "')"}
            style=(style_str)
            scope="col"
            {
                (content)
            }
    }
}

pub fn html_table_header(sort: &String) -> Markup {
    html!(
    thead {
        tr {
            (html_table_header_row("mpn", "MPN", sort))
            (html_table_header_row("category", "Category", sort))
            (html_table_header_row("footprint", "Footprint", sort))
            (html_table_header_row("value", "Value", sort))
            (html_table_header_row("quantity", "Qty.", sort))
            th style="font-weight: normal" {
                "Action"
            }
        }
    })
}

pub fn html_table_row(result: &InventoryItem) -> Markup {
    html!(
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
                @if let Some(quantity) = result.quantity {
                    (quantity)
                } @else {
                    "—"
                }
            }
            td {
                "TODO"
            }
        }
    )
}
