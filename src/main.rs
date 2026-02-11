// Copyright (C) 2026 tatjam
// SPDX-License-Identifier: GPL-3.0-or-later

use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    Form, Router,
    extract::{Request, State},
    http::header,
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use dotenvy;
use maud::html;
use serde::Deserialize;
use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use tower_sessions::{
    ExpiredDeletion, Expiry, Session, SessionManagerLayer, cookie::time::Duration,
};
use tower_sessions_sqlx_store::PostgresStore;
use tracing::info;

const LOGIN_HTML: &str = include_str!("../res/login.html");
const STYLE_CSS: &str = include_str!("../res/style.css");

const AUTH_SESSION_NAME: &'static str = "auth";

#[derive(Clone)]
struct AppState {
    password_hash: String,
    pool: Pool<Postgres>,
}

#[derive(Deserialize)]
struct LoginCredentials {
    password: String,
}

async fn auth_guard(session: Session, request: Request, next: Next) -> impl IntoResponse {
    let auth = session
        .get::<bool>(AUTH_SESSION_NAME)
        .await
        .unwrap_or_default()
        .unwrap_or(false);

    if auth {
        next.run(request).await
    } else {
        Redirect::to("/login").into_response()
    }
}

#[tokio::main]
async fn main() {
    if dotenvy::var("LOG_PLAIN").is_ok() {
        tracing_subscriber::fmt().with_ansi(false).init();
    } else {
        tracing_subscriber::fmt().with_ansi(true).init();
    }

    dotenvy::dotenv().unwrap();

    let login_str = format!(
        "postgres://{}:{}@{}/{}",
        dotenvy::var("DB_USER").unwrap(),
        dotenvy::var("DB_PASSWORD").unwrap(),
        dotenvy::var("DB_HOST").unwrap(),
        dotenvy::var("DB_NAME").unwrap()
    );

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(login_str.as_str())
        .await
        .expect("Failed to connect to Postgres");

    let allow_insecure = match dotenvy::var("ALLOW_UNSECURE_COOKIE")
        .unwrap_or(String::from("false"))
        .as_str()
    {
        "true" => true,
        _ => false,
    };

    let session_store = PostgresStore::new(pool.clone());

    session_store
        .migrate()
        .await
        .expect("Failed to migrate session store");

    tokio::task::spawn(
        session_store
            .clone()
            .continuously_delete_expired(tokio::time::Duration::from_secs(120)),
    );

    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(allow_insecure)
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(Duration::seconds(60 * 60 * 24 * 7)))
        .with_name("station_session");

    let host = dotenvy::var("HOST").unwrap();
    let password_hash = dotenvy::var("LOGIN_PASSWORD").unwrap();

    let shared_state = AppState {
        pool,
        password_hash,
    };

    let open_routes = Router::new()
        .route("/", get(home_page))
        .route("/login", get(login_page).post(login_handler))
        .route("/style.css", get(style_css_handler));

    let auth_routes = Router::new()
        .route("/inventory", get(inventory_page))
        .route("/logout", post(logout_handler))
        .route_layer(middleware::from_fn(auth_guard));

    let app = Router::new()
        .merge(open_routes)
        .merge(auth_routes)
        .with_state(shared_state)
        .layer(session_layer);

    let listener = tokio::net::TcpListener::bind(host).await.unwrap();

    info!("Listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();
}

async fn home_page(session: Session) -> impl IntoResponse {
    let auth = session
        .get::<bool>(AUTH_SESSION_NAME)
        .await
        .unwrap_or_default()
        .unwrap_or(false);

    if auth {
        Redirect::to("/inventory").into_response()
    } else {
        Redirect::to("/login").into_response()
    }
}

async fn inventory_page(session: Session) -> impl IntoResponse {
    Html("Inventory")
}

async fn login_page() -> impl IntoResponse {
    Html(LOGIN_HTML)
}

async fn login_handler(
    State(state): State<AppState>,
    session: Session,
    Form(creds): Form<LoginCredentials>,
) -> impl IntoResponse {
    let true_pass = PasswordHash::new(state.password_hash.as_str()).unwrap();
    let pass_valid = Argon2::default()
        .verify_password(creds.password.as_bytes(), &true_pass)
        .is_ok();

    if pass_valid {
        session.insert(AUTH_SESSION_NAME, true).await.unwrap();
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("HX-Redirect", "/inventory".parse().unwrap());
        (headers, "").into_response()
    } else {
        return html!({
            div.alert.alert-danger role="alert" style="color: red; margin-top: 10px;" {
                strong { "You shall not pass!" }
            }
        })
        .into_string()
        .into_response();
    }
}

async fn logout_handler(session: Session) -> impl IntoResponse {
    session.delete().await.ok();
    Redirect::to("/login")
}

async fn style_css_handler() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css")], STYLE_CSS)
}
