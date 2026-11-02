use std::sync::OnceLock;

use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, Request},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::any,
};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::{TokioExecutor, TokioIo},
};

use crate::state::AppState;

static CLIENT: OnceLock<Client<HttpConnector, Body>> = OnceLock::new();
static BACKEND: OnceLock<String> = OnceLock::new();

pub fn router() -> Router<AppState> {
    Router::new()
        .fallback(any(proxy))
        .layer(DefaultBodyLimit::disable())
}

fn client() -> &'static Client<HttpConnector, Body> {
    CLIENT.get_or_init(|| Client::builder(TokioExecutor::new()).build_http())
}

fn backend() -> &'static str {
    BACKEND.get_or_init(|| dotenvy::var("CHISEL_URL").unwrap())
}

async fn proxy(mut req: Request) -> Response {
    let upgrade = hyper::upgrade::on(&mut req);

    let path = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let uri = format!("{}{}", backend().trim_end_matches('/'), path);
    *req.uri_mut() = uri.parse().expect("Invalid CHISEL_URL");
    req.headers_mut().remove(header::HOST);

    match client().request(req).await {
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
