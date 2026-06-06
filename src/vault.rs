use std::{fmt::Display, str::from_utf8};

use axum::{
    Form,
    extract::{Path, State},
    http::{HeaderMap, header},
    response::{Html, IntoResponse},
};
use maud::{Markup, html};
use serde::Deserialize;
use sqlx::{Postgres, QueryBuilder, pool::PoolConnection};
use tracing::{error, info};

use crate::state::AppState;

pub fn handle_generic_vault_error<E: Display>(e: E) -> Html<String> {
    error!("Error while processing vault API call: {}", e);
    return Html(
        html! {
            article {
                "Error while processing, try again later."
            }
        }
        .into_string(),
    );
}

pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    info!("Performing vault query");

    let mut db_conn = match state.pool.acquire().await {
        Ok(conn) => conn,
        Err(e) => {
            return handle_generic_vault_error(e);
        }
    };

    let mut query = QueryBuilder::new("SELECT name FROM vault");
    let results = match query
        .build_query_scalar::<String>()
        .fetch_all(db_conn.as_mut())
        .await
    {
        Ok(results) => results,
        Err(e) => {
            return handle_generic_vault_error(e);
        }
    };

    let response = html! {
        table class="striped" {
            @for result in &results {
                (html_table_row(result))
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

pub fn html_table_row(result: &str) -> Markup {
    html! {
        tr {
            th scope="row" { (result) }
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
