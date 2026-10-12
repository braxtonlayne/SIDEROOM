//! axum glue: `GET /gateway?v=1&encoding=json[&compress=zstd-stream]`.

use axum::Router;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::header::{COOKIE, ORIGIN};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::compress::Compression;
use crate::conn::{ConnectParams, Frame, run_connection};
use crate::hub::Gateway;

#[derive(Debug, Deserialize)]
pub struct GatewayQuery {
    #[serde(default)]
    pub v: Option<u32>,
    #[serde(default)]
    pub encoding: Option<String>,
    #[serde(default)]
    pub compress: Option<String>,
}

/// Handler for sr-api to mount: `.route("/gateway", get(gateway_handler))`
/// on a router whose state is (or converts via `FromRef` to) [`Gateway`].
pub async fn gateway_handler(
    State(gw): State<Gateway>,
    Query(q): Query<GatewayQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if !matches!(q.v, Some(1 | 10)) {
        return (StatusCode::BAD_REQUEST, "unsupported gateway version").into_response();
    }
    if q.encoding.as_deref().unwrap_or("json") != "json" {
        return (StatusCode::BAD_REQUEST, "unsupported encoding").into_response();
    }
    let Some(compression) = Compression::parse(q.compress.as_deref().unwrap_or("")) else {
        return (StatusCode::BAD_REQUEST, "unsupported compression").into_response();
    };
    let params = ConnectParams {
        compression,
        session_cookie: session_cookie(&headers),
        origin: headers
            .get(ORIGIN)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
    };
    ws.on_upgrade(move |socket| serve_socket(gw, params, socket))
}

/// Name of the browser session cookie (matches the REST API).
pub const SESSION_COOKIE: &str = "sr_session";

/// The `sr_session` value from any `Cookie` header, if present.
fn session_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(';'))
        .find_map(|kv| {
            let (name, value) = kv.trim().split_once('=')?;
            (name == SESSION_COOKIE && !value.is_empty()).then(|| value.to_owned())
        })
}

/// A router with just `/gateway`, for standalone use or `Router::merge`.
pub fn routes() -> Router<Gateway> {
    Router::new().route("/gateway", get(gateway_handler))
}

async fn serve_socket(gw: Gateway, params: ConnectParams, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let (in_tx, in_rx) = mpsc::channel::<Frame>(64);
    let (out_tx, mut out_rx) = mpsc::channel::<Frame>(64);

    let reader = tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            let f = match msg {
                Message::Text(t) => Frame::Text(t.as_str().to_owned()),
                Message::Binary(b) => Frame::Binary(b.to_vec()),
                Message::Close(_) => break,
                Message::Ping(_) | Message::Pong(_) => continue,
            };
            if in_tx.send(f).await.is_err() {
                break;
            }
        }
        // Dropping in_tx tells the connection the peer is gone.
    });
    let writer = tokio::spawn(async move {
        while let Some(f) = out_rx.recv().await {
            let (msg, last) = match f {
                Frame::Text(t) => (Message::Text(t.into()), false),
                Frame::Binary(b) => (Message::Binary(b.into()), false),
                Frame::Close { code, reason } => (
                    Message::Close(Some(CloseFrame {
                        code,
                        reason: reason.into(),
                    })),
                    true,
                ),
            };
            if sink.send(msg).await.is_err() || last {
                break;
            }
        }
        let _ = sink.close().await;
    });

    run_connection(gw, params, in_rx, out_tx).await;
    // out_tx dropped: writer drains remaining frames (including Close) then ends.
    let _ = writer.await;
    reader.abort();
}
