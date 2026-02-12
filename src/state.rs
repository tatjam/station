use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use tower_sessions::{ExpiredDeletion, Expiry, SessionManagerLayer, cookie::time::Duration};
use tower_sessions_sqlx_store::PostgresStore;
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    pub password_hash: String,
    pub pool: Pool<Postgres>,
}

impl AppState {
    pub async fn setup_session_store(&self) -> SessionManagerLayer<PostgresStore> {
        let allow_insecure = match dotenvy::var("ALLOW_UNSECURE_COOKIE")
            .unwrap_or(String::from("false"))
            .as_str()
        {
            "true" => true,
            _ => false,
        };

        let session_store = PostgresStore::new(self.pool.clone());

        info!("Migrating session store DB");

        session_store
            .migrate()
            .await
            .expect("Failed to migrate session store");

        tokio::task::spawn(
            session_store
                .clone()
                .continuously_delete_expired(tokio::time::Duration::from_secs(120)),
        );

        SessionManagerLayer::new(session_store)
            .with_secure(allow_insecure)
            .with_same_site(tower_sessions::cookie::SameSite::Lax)
            .with_expiry(Expiry::OnInactivity(Duration::seconds(60 * 60 * 24 * 7)))
            .with_name("station_session")
    }

    pub async fn new() -> Self {
        let login_str = format!(
            "postgres://{}:{}@{}/{}",
            dotenvy::var("DB_USER").unwrap(),
            dotenvy::var("DB_PASSWORD").unwrap(),
            dotenvy::var("DB_HOST").unwrap(),
            dotenvy::var("DB_NAME").unwrap()
        );

        info!(
            "Connecting to DB postgres://xxx:xxx@{}/{}",
            dotenvy::var("DB_HOST").unwrap(),
            dotenvy::var("DB_NAME").unwrap()
        );

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(login_str.as_str())
            .await
            .expect("Failed to connect to Postgres");

        let password_hash = dotenvy::var("LOGIN_PASSWORD").unwrap();

        AppState {
            pool,
            password_hash,
        }
    }
}
