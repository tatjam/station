use crate::state::AppState;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    Form,
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Redirect},
};
use maud::html;
use serde::Deserialize;
use tower_sessions::Session;

const AUTH_SESSION_NAME: &'static str = "auth";

#[derive(Deserialize)]
pub struct LoginCredentials {
    pub password: String,
}

pub async fn auth_guard(session: Session, request: Request, next: Next) -> impl IntoResponse {
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

pub async fn is_auth(session: Session) -> bool {
    return session
        .get::<bool>(AUTH_SESSION_NAME)
        .await
        .unwrap_or_default()
        .unwrap_or(false);
}

pub async fn login_handler(
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

pub async fn logout_handler(session: Session) -> impl IntoResponse {
    session.delete().await.ok();
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("HX-Redirect", "/login".parse().unwrap());
    (headers, "").into_response()
}
