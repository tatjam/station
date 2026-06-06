use std::fmt::Display;

use axum::{
    extract::{Multipart, Path, State},
    http::header,
    response::{Html, IntoResponse},
};
use maud::{Markup, html};
use sqlx::QueryBuilder;
use tracing::{error, info};

use crate::state::AppState;

#[derive(sqlx::FromRow)]
struct VaultEntry {
    name: String,
    modified_at: chrono::NaiveDateTime,
}

pub fn handle_generic_vault_error<E: Display>(e: E) -> Html<String> {
    error!("Error while processing vault API call: {}", e);
    Html(
        html! {
            article {
                "Error while processing, try again later."
            }
        }
        .into_string(),
    )
}

pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    info!("Performing vault query");

    let mut db_conn = match state.pool.acquire().await {
        Ok(conn) => conn,
        Err(e) => {
            return handle_generic_vault_error(e);
        }
    };

    let results = match sqlx::query_as::<_, VaultEntry>("SELECT name, modified_at FROM vault")
        .fetch_all(db_conn.as_mut())
        .await
    {
        Ok(rows) => rows,
        Err(e) => return handle_generic_vault_error(e),
    };

    let response = html! {
        table class="striped" {
            @for result in &results {
                (html_table_row(&result.name, result.modified_at))
            }
        }
    }
    .into_string();

    Html(response)
}

pub async fn download(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    info!("Performing vault download for {}", id);

    let mut db_conn = match state.pool.acquire().await {
        Ok(conn) => conn,
        Err(e) => {
            return handle_generic_vault_error(e).into_response();
        }
    };

    let mut query = QueryBuilder::new("SELECT data FROM vault WHERE name = ");
    query.push_bind(&id);

    let result = match query
        .build_query_scalar::<Vec<u8>>()
        .fetch_one(db_conn.as_mut())
        .await
    {
        Ok(results) => results,
        Err(e) => {
            return handle_generic_vault_error(e).into_response();
        }
    };

    let disposition = format!("attachment; filename=\"{}\"", id);

    (
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CONTENT_DISPOSITION, disposition.as_str()),
        ],
        result,
    )
        .into_response()
}

pub async fn upload(State(state): State<AppState>, mut multipart: Multipart) -> impl IntoResponse {
    let field_maybe = match multipart.next_field().await {
        Ok(field_maybe) => field_maybe,
        Err(e) => {
            return handle_generic_vault_error(e).into_response();
        }
    };

    let field = match field_maybe {
        Some(field) => field,
        None => {
            return handle_generic_vault_error("Field not received").into_response();
        }
    };

    if field.name().unwrap_or_default() != "file" {
        return handle_generic_vault_error("Invalid field received").into_response();
    }

    let id = match field.file_name() {
        Some(file_name) => file_name.to_string(),
        None => {
            return handle_generic_vault_error("Missing file name field").into_response();
        }
    };

    let file_bytes = match field.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            return handle_generic_vault_error(e).into_response();
        }
    };

    info!("Performing vault upload for {}", id);

    let mut db_conn = match state.pool.acquire().await {
        Ok(conn) => conn,
        Err(e) => {
            return handle_generic_vault_error(e).into_response();
        }
    };

    let mut query = QueryBuilder::new("INSERT INTO vault (name, data) VALUES (");
    query.push_bind(&id);
    query.push(", ");
    query.push_bind(file_bytes.as_ref());
    query.push(") ON CONFLICT (name) DO UPDATE SET data = EXCLUDED.data, modified_at = NOW()");

    if let Err(e) = query.build().execute(db_conn.as_mut()).await {
        return handle_generic_vault_error(e).into_response();
    }

    let response = html! {
        "Successfully uploaded"
    }
    .into_string();

    Html(response).into_response()
}

pub fn html_table_row(result: &str, modified_at: chrono::NaiveDateTime) -> Markup {
    html! {
        tr {
            th scope="row" { (result) }
            td { (modified_at.format("%Y-%m-%d %H:%M:%S").to_string()) }
            td {
                a href={"/api/vault/download/" (result)} download style="display: contents;"
                {
                    button {
                        "Download"
                    }
                }
            }
        }
    }
}
