use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, Request, State},
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::any,
};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::{TokioExecutor, TokioIo},
};
use tokio::{
    process::{Child, Command},
    sync::Mutex,
};
use tracing::warn;

use crate::state::AppState;

#[derive(Clone)]
pub struct ChiselConfig {
    url: String,
    client: Client<HttpConnector, Body>,
    proc: Arc<Mutex<Child>>,
}

impl ChiselConfig {
    pub async fn start_chisel() -> Result<Child, std::io::Error> {
        let cmd = dotenvy::var("CHISEL_CMD").unwrap();

        let key = dotenvy::var("CHISEL_KEY").unwrap();

        let port = dotenvy::var("CHISEL_PORT")
            .unwrap()
            .parse::<u16>()
            .expect("Invalid CHISEL_PORT");

        let auth_file = dotenvy::var("CHISEL_AUTHFILE").unwrap();

        let mut cmd = Command::new(cmd);
        cmd.args([
            "server",
            "--socks5",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--keyfile",
            &key,
            "--authfile",
            &auth_file,
        ]);

        cmd.spawn()
    }

    pub async fn new() -> Self {
        let client = Client::builder(TokioExecutor::new()).build_http();
        let url = dotenvy::var("CHISEL_URL").unwrap();
        url.parse::<Uri>().expect("Invalid CHISEL_URL");

        ChiselConfig {
            url,
            client,
            proc: Arc::new(Mutex::new(Self::start_chisel().await.unwrap())),
        }
    }

    pub async fn start_chisel_if_stopped(self: &ChiselConfig) {
        let mut handle = self.proc.clone().lock_owned().await;
        if handle.try_wait().is_ok_and(|f| f.is_none()) {
            return;
        }
        warn!("Chisel was killed, restarting");
        match Self::start_chisel().await {
            Ok(child) => *handle = child,
            Err(e) => warn!("Failed to restart chisel: {e}"),
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .fallback(any(proxy))
        .layer(DefaultBodyLimit::disable())
}

async fn proxy(State(state): State<AppState>, mut req: Request) -> Response {
    let upgrade = hyper::upgrade::on(&mut req);

    let path = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let uri = format!("{}{}", state.chisel.url.trim_end_matches('/'), path);
    *req.uri_mut() = uri.parse().expect("Invalid CHISEL_URL");
    req.headers_mut().remove(header::HOST);

    state.chisel.start_chisel_if_stopped().await;

    match state.chisel.client.request(req).await {
        Ok(mut res) => {
            if res.status() == StatusCode::SWITCHING_PROTOCOLS {
                let backend_upgrade = hyper::upgrade::on(&mut res);
                tokio::spawn(async move {
                    if let (Ok(client_io), Ok(backend_io)) = (upgrade.await, backend_upgrade.await)
                    {
                        let (mut client_io, mut backend_io) =
                            (TokioIo::new(client_io), TokioIo::new(backend_io));
                        let _ =
                            tokio::io::copy_bidirectional(&mut client_io, &mut backend_io).await;
                    }
                });
            }
            res.map(Body::new)
        }
        Err(err) => (StatusCode::BAD_GATEWAY, err.to_string()).into_response(),
    }
}
