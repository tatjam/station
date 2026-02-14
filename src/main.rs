// Copyright (C) 2026 tatjam
// SPDX-License-Identifier: GPL-3.0-or-later

mod auth;
mod inventory;
mod state;

use axum::{
    Router,
    http::header,
    middleware::{self},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use dotenvy;
use tower_sessions::Session;
use tracing::info;

use crate::state::AppState;

const LOGIN_HTML: &str = include_str!("../res/login.html");
const INVENTORY_HTML: &str = include_str!("../res/inventory.html");
const STYLE_CSS: &str = include_str!("../res/style.css");

#[tokio::main]
async fn main() {
    if dotenvy::var("LOG_PLAIN").is_ok() {
        tracing_subscriber::fmt().with_ansi(false).init();
    } else {
        tracing_subscriber::fmt().with_ansi(true).init();
    }

    dotenvy::dotenv().unwrap();

    let shared_state = AppState::new().await;

    let open_routes = Router::new()
        .route("/", get(home_page))
        .route(
            "/login",
            get(|| html_page(LOGIN_HTML)).post(auth::login_handler),
        )
        .route("/style.css", get(|| css_file(STYLE_CSS)));

    let auth_routes = Router::new()
        .route("/inventory", get(|| html_page(INVENTORY_HTML)))
        .route("/logout", post(auth::logout_handler))
        .route("/api/inventory/search", get(inventory::search_handler))
        .route(
            "/api/inventory/categories",
            get(inventory::category_list_handler),
        )
        .route(
            "/api/inventory/footprints",
            get(inventory::footprint_list_handler),
        )
        .route_layer(middleware::from_fn(auth::auth_guard));

    let session_layer = shared_state.setup_session_store().await;

    let app = Router::new()
        .merge(open_routes)
        .merge(auth_routes)
        .with_state(shared_state)
        .layer(session_layer);

    let host = dotenvy::var("HOST").unwrap();
    let listener = tokio::net::TcpListener::bind(host).await.unwrap();

    info!("Listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();
}

async fn home_page(session: Session) -> impl IntoResponse {
    if auth::is_auth(session).await {
        Redirect::to("/inventory").into_response()
    } else {
        Redirect::to("/login").into_response()
    }
}

async fn html_page(html: &'static str) -> impl IntoResponse {
    Html(html)
}

async fn css_file(css: &'static str) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css")], css)
}
