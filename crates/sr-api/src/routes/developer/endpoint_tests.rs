//! HTTP interactions endpoint (DEV-BOT-005): key, verification ping, signed delivery.

use super::tests::{BOB, Env, OWNER, Who, env_full, next_named, no_event};
use super::{EndpointClient, EndpointReply};
use async_trait::async_trait;
use axum::http::StatusCode;
use serde_json::{Value, json};
use sr_core::Permissions;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

/// A developer's server: checks signatures with the public key it was given, answers PINGs
/// and answers interactions with a canned callback body.
pub(super) struct Dev {
    pub(super) key: Mutex<Option<String>>,
    /// Reply to a verified, non-PING request.
    reply: Mutex<Value>,
    check_signature: bool,
    pong: bool,
    /// Answer every verified non-PING request with a 500.
    pub(super) fail: std::sync::atomic::AtomicBool,
    pub(super) seen: Mutex<Vec<Value>>,
}

impl Dev {
    fn with(check_signature: bool, pong: bool) -> Arc<Self> {
        Arc::new(Self {
            key: Mutex::new(None),
            reply: Mutex::new(json!({"type": 4, "data": {"content": "pong from http"}})),
            check_signature,
            pong,
            fail: std::sync::atomic::AtomicBool::new(false),
            seen: Mutex::default(),
        })
    }
    fn new() -> Arc<Self> {
        Self::with(true, true)
    }
}

#[async_trait]
impl EndpointClient for Dev {
    async fn post(
        &self,
        _url: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        _timeout: Duration,
    ) -> Result<EndpointReply, String> {
        let h = |n: &str| {
            headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(n))
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let key = self.key.lock().unwrap().clone().unwrap_or_default();
        let valid = sr_apps::signing::verify_hex(
            &key,
            &h("X-Signature-Ed25519"),
            &h("X-Signature-Timestamp"),
            &body,
        );
        if self.check_signature && !valid {
            return Ok(EndpointReply {
                status: 401,
                body: b"invalid request signature".to_vec(),
            });
        }
        let v: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        self.seen.lock().unwrap().push(v.clone());
        if v["type"] != 1 && self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(EndpointReply {
                status: 500,
                body: Vec::new(),
            });
        }
        let out = if v["type"] == 1 {
            if self.pong {
                json!({"type": 1})
            } else {
                json!({"nope": true})
            }
        } else {
            self.reply.lock().unwrap().clone()
        };
        Ok(EndpointReply {
            status: 200,
            body: out.to_string().into_bytes(),
        })
    }
}

pub(super) struct Setup {
    pub(super) e: Env,
    pub(super) app: String,
    pub(super) bot: String,
    pub(super) dev: Arc<Dev>,
    pub(super) cmd: String,
}

pub(super) async fn setup_default() -> Setup {
    setup(Dev::new()).await
}

async fn setup(dev: Arc<Dev>) -> Setup {
    let e = env_full(true, Some(dev.clone())).await;
    let (app, bot, token, _) = e.new_app().await;
    e.configure(&app).await;
    e.install(
        &app,
        &bot,
        Permissions::SEND_MESSAGES.bits() | Permissions::VIEW_CHANNEL.bits(),
    )
    .await;
    let (s, v) = e
        .post(
            Who::Bot(&token),
            &format!("/applications/{app}/commands"),
            json!({"name": "echo", "description": "Echo"}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let cmd = v["id"].as_str().unwrap().to_owned();
    Setup {
        e,
        app,
        bot,
        dev,
        cmd,
    }
}

pub(super) const URL: &str = "https://bot.example.com/interactions";

impl Setup {
    pub(super) async fn key(&self) -> String {
        let (s, v) = self
            .e
            .post(
                Who::Session(OWNER),
                &format!("/applications/{}/interactions-key", self.app),
                json!({}),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{v}");
        let k = v["verify_key"].as_str().unwrap().to_owned();
        assert_eq!(k.len(), 64);
        *self.dev.key.lock().unwrap() = Some(k.clone());
        k
    }

    pub(super) async fn set_url(&self, url: Value) -> (StatusCode, Value) {
        self.e
            .patch(
                Who::Session(OWNER),
                &format!("/applications/{}", self.app),
                json!({ "interactions_endpoint_url": url }),
            )
            .await
    }

    pub(super) async fn app_json(&self) -> Value {
        self.e
            .get(Who::Session(OWNER), &format!("/applications/{}", self.app))
            .await
            .1
    }

    pub(super) async fn invoke(&self) -> (StatusCode, Value) {
        self.e
            .post(
                Who::Session(BOB),
                "/interactions",
                json!({"type": 2, "application_id": self.app,
                    "channel_id": self.e.ch.to_string(), "nonce": "n-9",
                    "data": {"id": self.cmd}}),
            )
            .await
    }
}

#[tokio::test]
async fn the_key_is_created_once_and_only_for_the_owner() {
    let s = setup(Dev::new()).await;
    let v = s.app_json().await;
    assert!(v.get("verify_key").is_none(), "no key until asked: {v}");
    let k1 = s.key().await;
    assert_eq!(k1, s.key().await, "idempotent");
    let v = s.app_json().await;
    assert_eq!(v["verify_key"], json!(k1));
    assert!(!v.to_string().contains("seed"));
    // someone else sees a 404
    let st =
        s.e.post(
            Who::Session(BOB),
            &format!("/applications/{}/interactions-key", s.app),
            json!({}),
        )
        .await
        .0;
    assert_eq!(st, StatusCode::NOT_FOUND);
    // the public application object (bot view) never carries it
    let (st, v) =
        s.e.post(
            Who::Session(OWNER),
            &format!("/applications/{}/bot/reset-token", s.app),
            json!({"password": "correct"}),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let token = v["token"].as_str().unwrap().to_owned();
    let (_, v) = s.e.get(Who::Bot(&token), "/applications/@me").await;
    assert!(v.get("verify_key").is_none() && v.get("interactions_endpoint_url").is_none());
}

#[tokio::test]
async fn saving_a_url_runs_the_signed_ping_and_the_bad_signature_check() {
    let s = setup(Dev::new()).await;
    let _ = s.key().await;
    let (st, v) = s.set_url(json!(URL)).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["interactions_endpoint_url"], json!(URL));
    let seen = s.dev.seen.lock().unwrap().clone();
    assert_eq!(
        seen.len(),
        1,
        "the bad-signature probe is rejected before the handler"
    );
    assert_eq!(seen[0]["type"], 1);
    assert_eq!(seen[0]["application_id"], json!(s.app));
    let (st, v) = s.set_url(Value::Null).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert!(v.get("interactions_endpoint_url").is_none());
}

#[tokio::test]
async fn saving_a_url_creates_the_key_when_there_is_none() {
    let s = setup(Dev::new()).await;
    // the stand-in server needs the key to verify; fetch it as the owner would after reading
    // it from the portal, then save
    let k = s.key().await;
    assert_eq!(s.set_url(json!(URL)).await.0, StatusCode::OK);
    assert_eq!(s.app_json().await["verify_key"], json!(k));
}

#[tokio::test]
async fn a_url_that_fails_verification_is_refused_and_not_saved() {
    // accepts everything, so it cannot be checking signatures
    let s = setup(Dev::with(false, true)).await;
    let _ = s.key().await;
    let (st, v) = s.set_url(json!(URL)).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(
        v["errors"]["interactions_endpoint_url"]["_errors"][0]["code"],
        "INTERACTIONS_ENDPOINT_VERIFY"
    );
    assert!(
        s.app_json()
            .await
            .get("interactions_endpoint_url")
            .is_none()
    );
    // no PONG
    let s = setup(Dev::with(true, false)).await;
    let _ = s.key().await;
    assert_eq!(s.set_url(json!(URL)).await.0, StatusCode::BAD_REQUEST);
    // bad shapes never reach the network
    for bad in [
        "http://bot.example.com/i",
        "https://u:p@bot.example.com/i",
        "https://bot.example.com/i#x",
        "nonsense",
    ] {
        let (st, v) = s.set_url(json!(bad)).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{bad}: {v}");
    }
    assert_eq!(s.dev.seen.lock().unwrap().len(), 1, "only the no-PONG ping");
}

#[tokio::test]
async fn without_a_wired_client_urls_cannot_be_saved() {
    let e = env_full(true, None).await;
    let (app, _, _, _) = e.new_app().await;
    let (st, v) = e
        .patch(
            Who::Session(OWNER),
            &format!("/applications/{app}"),
            json!({"interactions_endpoint_url": URL}),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(
        v["errors"]["interactions_endpoint_url"]["_errors"][0]["code"],
        "NOT_AVAILABLE"
    );
}

#[tokio::test]
async fn interactions_go_to_the_endpoint_not_the_gateway_and_its_reply_is_applied() {
    let s = setup(Dev::new()).await;
    let _ = s.key().await;
    assert_eq!(s.set_url(json!(URL)).await.0, StatusCode::OK);
    s.dev.seen.lock().unwrap().clear();
    let mut bot_events = s.e.user_events(s.bot.parse().unwrap());
    let mut invoker = s.e.user_events(BOB);
    let (st, v) = s.invoke().await;
    assert_eq!(st, StatusCode::NO_CONTENT, "{v}");
    let ok = next_named(&mut invoker, "INTERACTION_SUCCESS").await;
    assert_eq!(ok["nonce"], "n-9");
    no_event(&mut bot_events, "INTERACTION_CREATE").await;
    let seen = s.dev.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0]["type"], 2);
    assert_eq!(seen[0]["data"]["name"], "echo");
    assert_eq!(seen[0]["token"].as_str().unwrap().len(), 43);
    let (st, msgs) =
        s.e.get(
            Who::Session(OWNER),
            &format!("/channels/{}/messages", s.e.ch),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{msgs}");
    assert_eq!(msgs[0]["content"], "pong from http");
    assert_eq!(msgs[0]["author"]["id"], json!(s.bot));
}

#[tokio::test]
async fn a_wrong_reply_fails_the_interaction_for_the_invoker() {
    let s = setup(Dev::new()).await;
    let _ = s.key().await;
    assert_eq!(s.set_url(json!(URL)).await.0, StatusCode::OK);
    *s.dev.reply.lock().unwrap() = json!({"type": 99});
    let mut invoker = s.e.user_events(BOB);
    assert_eq!(s.invoke().await.0, StatusCode::NO_CONTENT);
    let fail = next_named(&mut invoker, "INTERACTION_FAILURE").await;
    assert_eq!(fail["reason_code"], 2);
}

#[tokio::test]
async fn clearing_the_url_returns_delivery_to_the_gateway() {
    let s = setup(Dev::new()).await;
    let _ = s.key().await;
    assert_eq!(s.set_url(json!(URL)).await.0, StatusCode::OK);
    assert_eq!(s.set_url(Value::Null).await.0, StatusCode::OK);
    let mut bot_events = s.e.user_events(s.bot.parse().unwrap());
    assert_eq!(s.invoke().await.0, StatusCode::NO_CONTENT);
    let ev = next_named(&mut bot_events, "INTERACTION_CREATE").await;
    assert_eq!(ev["type"], 2);
}
