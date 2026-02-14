use std::fmt::Display;

use axum::{
    Form,
    extract::{Path, State},
    http::HeaderMap,
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
    in_stage: Option<String>,
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
    staged: Option<i64>,
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

fn parse_multiple_value(v: &String) -> Option<f32> {
    let number_end = v.rfind(|x: char| x.is_ascii_digit())?;
    if number_end + 1 >= v.len() {
        return v.parse::<f32>().ok();
    }

    assert!(number_end + 1 < v.len());

    let number_part = &v[0..number_end + 1];
    let qty_part = &v[number_end + 1..].trim();

    let number = number_part.parse::<f32>().ok()?;
    match *qty_part {
        "p" => Some(number * 1e-12),
        "n" => Some(number * 1e-9),
        "u" => Some(number * 1e-6),
        "m" => Some(number * 1e-3),
        "k" => Some(number * 1e3),
        "M" => Some(number * 1e6),
        "G" => Some(number * 1e9),
        _ => Some(number),
    }
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

    if search.in_stage.is_some() {
        query.push(" AND staged > 0");
    }

    if !search.min_val.is_empty() {
        if let Some(min) = parse_multiple_value(&search.min_val) {
            query.push(" AND value >= ");
            query.push_bind(min);
        }
    }

    if !search.max_val.is_empty() {
        if let Some(max) = parse_multiple_value(&search.max_val) {
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

fn response_filter_list(
    filter_results: Vec<String>,
    prev_value: &String,
    no_filter: &'static str,
) -> Markup {
    let mut filter_results = filter_results;
    // Remove the already chosen category, we insert it at the top
    let chosen_idx = filter_results.iter().position(|x| x == prev_value);
    let mut chosen_elem = None;
    if let Some(idx) = chosen_idx
        && prev_value != no_filter
    {
        chosen_elem = Some(filter_results.swap_remove(idx));
    }

    html! {
        @if let Some(chosen) = chosen_elem {
            option {
                (chosen)
            }
        }
        option {
            (no_filter)
        }
        @for cat in &filter_results {
            option {
                (cat)
            }
        }
    }
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
    if fandc.footprint == NO_FOOTPRINT_STR {
        query.push(" WHERE footprint IS NULL");
    } else if fandc.footprint != ALL_FOOTPRINTS_STR {
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

    Html(response_filter_list(results, &fandc.category, ALL_CATEGORIES_STR).into_string())
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

    let mut query = QueryBuilder::new("SELECT DISTINCT COALESCE(footprint, '");
    query.push(NO_FOOTPRINT_STR);
    query.push("') FROM inventory");

    if fandc.category != "All Categories" {
        query.push(" WHERE category = ");
        query.push_bind(fandc.category);
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

    Html(response_filter_list(results, &fandc.footprint, ALL_FOOTPRINTS_STR).into_string())
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

async fn update_stage(id: i64, number: i64, db_conn: &mut PoolConnection<Postgres>) -> Option<i64> {
    let mut query = QueryBuilder::new("UPDATE stock SET staged = LEAST(COALESCE(staged, 0) + ");
    query.push_bind(number);
    query.push(", quantity)");
    query.push(" WHERE part_id = ");
    query.push_bind(id);
    query.push(" AND quantity IS NOT NULL");
    query.push(" AND COALESCE(staged, 0) + ");
    query.push_bind(number);
    query.push(" >= 0");
    query.push(" RETURNING staged");
    match query
        .build_query_scalar::<i64>()
        .fetch_optional(db_conn.as_mut())
        .await
    {
        Ok(v) => v,
        Err(e) => {
            let _ = handle_generic_inventory_error(e);
            None
        }
    }
}

pub async fn confirm_stage_handler(State(state): State<AppState>) -> impl IntoResponse {
    info!("Confirming stage");

    let mut db_conn = match state.pool.acquire().await {
        Ok(conn) => conn,
        Err(e) => {
            return (HeaderMap::new(), handle_generic_inventory_error(e));
        }
    };

    let mut query = QueryBuilder::new("UPDATE stock SET");
    query.push(" quantity = quantity - COALESCE(staged, 0),");
    query.push(" staged = NULL");
    query.push(" WHERE staged <= quantity");

    match query.build().execute(db_conn.as_mut()).await {
        Ok(_) => {
            let mut headers = HeaderMap::new();
            headers.insert("HX-Trigger", "inventoryUpdated".parse().unwrap());
            (headers, Html(String::from("OK")))
        }
        Err(e) => (HeaderMap::new(), handle_generic_inventory_error(e)),
    }
}

pub async fn staging_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    info!("Staging component {}", id);

    let mut db_conn = match state.pool.acquire().await {
        Ok(conn) => conn,
        Err(e) => {
            return handle_generic_inventory_error(e);
        }
    };

    Html(html_stage(id, update_stage(id, 1, &mut db_conn).await).into_string())
}

pub async fn unstaging_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    info!("Unstaging component {}", id);

    let mut db_conn = match state.pool.acquire().await {
        Ok(conn) => conn,
        Err(e) => {
            return handle_generic_inventory_error(e);
        }
    };

    Html(html_stage(id, update_stage(id, -1, &mut db_conn).await).into_string())
}

fn html_stage(id: i64, number: Option<i64>) -> Markup {
    html!(
        span id={"staged-" (id)} style="color: red;" {
            @if let Some(staged) = number {
                @if staged > 0 {
                    "(" (staged) ")"
                } @else if staged < 0 {
                    "( ERROR )"
                }
            }
        }
    )
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
    const STAGING_BUTTON_STYLE: &'static str =
        "padding: 0rem; width: 1.5rem; height: 1.5rem; vertical-align: middle;";

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
                " "
                (html_stage(result.id, result.staged))
            }
            td {
                div style="display:inline-flex; gap: 0.5rem;" {
                    button
                    style=(STAGING_BUTTON_STYLE)
                    hx-post={"/api/inventory/stage/" (result.id)}
                    hx-target={"#staged-" (result.id)}
                    hx-swap="outerHTML" {
                        "+"
                    }
                    button style=(STAGING_BUTTON_STYLE)
                    hx-post={"/api/inventory/unstage/" (result.id)}
                    hx-target={"#staged-" (result.id)}
                    hx-swap="outerHTML" {
                        "-"
                    }
                }
            }
        }
    )
}
