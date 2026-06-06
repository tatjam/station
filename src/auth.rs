use crate::state::AppState;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    Form,
    extract::{ConnectInfo, Request, State},
    http::HeaderMap,
    middleware::Next,
    response::{IntoResponse, Redirect},
};
use chrono::Utc;
use maud::html;
use serde::Deserialize;
use std::net::SocketAddr;
use tower_sessions::Session;
use tracing::{info, warn};

const AUTH_SESSION_NAME: &str = "auth";
const FAILED_EMAIL_COOLDOWN_SECS: i64 = 86400;

#[derive(Deserialize)]
pub struct LoginCredentials {
    pub password: String,
}

#[derive(sqlx::FromRow)]
struct LoginStats {
    failed_attempts: i32,
    last_failed_email_sent_at: Option<chrono::DateTime<Utc>>,
    last_reset_at: Option<chrono::DateTime<Utc>>,
}

fn get_real_ip(headers: &HeaderMap, addr: SocketAddr) -> String {
    // X-Real-IP is used by alwaysdata, other reverse proxies may use other values!
    headers
        .get("X-Real-IP")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| addr.ip().to_string())
}

async fn get_stats(state: &AppState) -> Option<LoginStats> {
    sqlx::query_as::<_, LoginStats>(
        "SELECT failed_attempts, last_failed_email_sent_at, last_reset_at 
         FROM login_stats WHERE id = 1",
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|e| tracing::error!("Failed to get login stats: {}", e))
    .ok()
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    session: Session,
    Form(creds): Form<LoginCredentials>,
) -> impl IntoResponse {
    let ip = get_real_ip(&headers, addr);
    let true_pass = PasswordHash::new(state.password_hash.as_str()).unwrap();
    let pass_valid = Argon2::default()
        .verify_password(creds.password.as_bytes(), &true_pass)
        .is_ok();

    if pass_valid {
        session.cycle_id().await.unwrap();
        session.insert(AUTH_SESSION_NAME, true).await.unwrap();
        info!("Successful login from {}", ip);

        // Grab stats before reset for the email
        let stats = get_stats(&state).await;

        // Reset failed attempts
        sqlx::query(
            "UPDATE login_stats SET failed_attempts = 0, last_reset_at = NOW() WHERE id = 1",
        )
        .execute(&state.pool)
        .await
        .ok();

        // Send email in background
        let mail = state.mail.clone();
        tokio::spawn(async move {
            info!("Sending email notification of new login");
            let failed_since = stats.as_ref().map(|s| s.failed_attempts).unwrap_or(0);
            let last_login = stats
                .as_ref()
                .and_then(|s| s.last_reset_at)
                .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "never".to_string());

            mail.send(
                "New login to station detected",
                format!(
                    "New successful login at {} UTC from IP {}.\n\nFailed attempts since last login ({}): {}",
                    Utc::now().format("%Y-%m-%d %H:%M:%S"),
                    ip,
                    last_login,
                    failed_since
                ),
            );
        });

        let mut resp_headers = axum::http::HeaderMap::new();
        resp_headers.insert("HX-Redirect", "/inventory".parse().unwrap());
        (resp_headers, "").into_response()
    } else {
        warn!("Failed login attempt from {}", ip);

        let new_count = sqlx::query_scalar::<_, i32>(
            "UPDATE login_stats SET failed_attempts = failed_attempts + 1 WHERE id = 1 RETURNING failed_attempts"
        )
        .fetch_one(&state.pool)
        .await
        .unwrap_or(1);

        // This atomically updates the cooldown, so even if thousands of request arrive at once, they only send a single email
        let should_email = sqlx::query_scalar::<_, i32>(
            "UPDATE login_stats 
             SET last_failed_email_sent_at = NOW() 
             WHERE id = 1 AND (last_failed_email_sent_at IS NULL OR EXTRACT(EPOCH FROM (NOW() - last_failed_email_sent_at)) > $1)
             RETURNING id"
        )
        .bind(FAILED_EMAIL_COOLDOWN_SECS)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten()
        .is_some();

        if should_email {
            let mail = state.mail.clone();
            tokio::spawn(async move {
                warn!("Sending email notification of failed login");
                mail.send(
                    "Failed login attempt on station",
                    format!(
                        "Failed login attempt at {} UTC from IP {}.\n\nTotal failed attempts since last successful login: {}",
                        Utc::now().format("%Y-%m-%d %H:%M:%S"),
                        ip,
                        new_count
                    ),
                );
            });
        } else {
            warn!("Skipping sending email notification of failed login");
        }

        html!({
            div.alert.alert-danger role="alert" style="color: red; margin-top: 10px;" {
                strong { "You shall not pass!" }
            }
        })
        .into_string()
        .into_response()
    }
}

pub async fn logout_handler(session: Session) -> impl IntoResponse {
    session.delete().await.ok();
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("HX-Redirect", "/login".parse().unwrap());
    (headers, "").into_response()
}
