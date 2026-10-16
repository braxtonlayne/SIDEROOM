//! REST client: a small transport trait (so tests need no network), rate-limit aware retries
//! and typed helpers for the routes a bot uses.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::model::{CallbackType, Interaction, Message, User};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// How requests reach the server. [`ReqwestTransport`] is the real one.
#[async_trait]
pub trait HttpTransport: Send + Sync {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse>;
}

/// `reqwest` with rustls.
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| Error::Transport(e.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse> {
        let method = match req.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Patch => reqwest::Method::PATCH,
            Method::Delete => reqwest::Method::DELETE,
        };
        let mut b = self.client.request(method, &req.url);
        for (k, v) in &req.headers {
            b = b.header(k, v);
        }
        if let Some(body) = req.body {
            b = b.body(body);
        }
        let resp = b
            .send()
            .await
            .map_err(|e| Error::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
            .collect();
        let body = resp
            .bytes()
            .await
            .map_err(|e| Error::Transport(e.to_string()))?
            .to_vec();
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

/// Retries after a 429 before giving up.
const MAX_RATE_LIMIT_RETRIES: u32 = 3;
/// Longest single wait the client accepts on a 429.
const MAX_RETRY_WAIT: Duration = Duration::from_secs(60);

/// A REST client for one bot token.
#[derive(Clone)]
pub struct Rest {
    base: String,
    token: Arc<str>,
    transport: Arc<dyn HttpTransport>,
}

impl std::fmt::Debug for Rest {
    // The token is a credential and never printed.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rest")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

impl Rest {
    /// `base` is the API root such as `https://chat.example/api/v1`.
    pub fn new(base: impl Into<String>, token: impl Into<String>) -> Result<Self> {
        Ok(Self::with_transport(
            base,
            token,
            Arc::new(ReqwestTransport::new()?),
        ))
    }

    pub fn with_transport(
        base: impl Into<String>,
        token: impl Into<String>,
        transport: Arc<dyn HttpTransport>,
    ) -> Self {
        let token: String = token.into();
        let token = token.strip_prefix("Bot ").unwrap_or(&token).to_owned();
        Self {
            base: base.into().trim_end_matches('/').to_owned(),
            token: token.into(),
            transport,
        }
    }

    pub(crate) fn token(&self) -> &str {
        &self.token
    }

    /// One call with the bot credential; JSON in, JSON out (`Null` for 204).
    pub async fn request(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        self.send(method, path, body, true).await
    }

    /// Interaction-token routes carry their own credential, so no `Authorization` header.
    async fn request_anon(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        self.send(method, path, body, false).await
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        auth: bool,
    ) -> Result<Value> {
        let url = format!("{}{}", self.base, path);
        let mut attempt = 0;
        loop {
            let mut headers = vec![("User-Agent".to_owned(), "sr-sdk".to_owned())];
            if auth {
                headers.push(("Authorization".into(), format!("Bot {}", self.token)));
            }
            let body_bytes = body.as_ref().map(|b| b.to_string().into_bytes());
            if body_bytes.is_some() {
                headers.push(("Content-Type".into(), "application/json".into()));
            }
            let resp = self
                .transport
                .send(HttpRequest {
                    method,
                    url: url.clone(),
                    headers,
                    body: body_bytes,
                })
                .await?;
            if resp.status == 429 {
                let wait = retry_after(&resp);
                attempt += 1;
                if attempt > MAX_RATE_LIMIT_RETRIES || wait > MAX_RETRY_WAIT {
                    return Err(Error::RateLimited {
                        retry_after_ms: u64::try_from(wait.as_millis()).unwrap_or(u64::MAX),
                    });
                }
                tokio::time::sleep(wait).await;
                continue;
            }
            return decode(resp);
        }
    }

    /// Typed convenience over [`Rest::request`].
    pub async fn request_as<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T> {
        Ok(serde_json::from_value(
            self.request(method, path, body).await?,
        )?)
    }

    // ---- routes a bot uses ----

    pub async fn current_user(&self) -> Result<User> {
        self.request_as(Method::Get, "/users/@me", None).await
    }

    /// `GET /gateway/bot`: `(url, recommended shards)`.
    pub async fn gateway_bot(&self) -> Result<(String, u32)> {
        let v = self.request(Method::Get, "/gateway/bot", None).await?;
        let url = v
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Decode("gateway/bot without url".into()))?
            .to_owned();
        let shards = v.get("shards").and_then(Value::as_u64).unwrap_or(1).max(1);
        Ok((url, u32::try_from(shards).unwrap_or(1)))
    }

    pub async fn create_message(
        &self,
        channel_id: &str,
        message: impl Into<MessageBody>,
    ) -> Result<Message> {
        let body = message.into().into_json();
        self.request_as(
            Method::Post,
            &format!("/channels/{channel_id}/messages"),
            Some(body),
        )
        .await
    }

    pub async fn edit_message(
        &self,
        channel_id: &str,
        message_id: &str,
        message: impl Into<MessageBody>,
    ) -> Result<Message> {
        let body = message.into().into_json();
        self.request_as(
            Method::Patch,
            &format!("/channels/{channel_id}/messages/{message_id}"),
            Some(body),
        )
        .await
    }

    pub async fn delete_message(&self, channel_id: &str, message_id: &str) -> Result<()> {
        self.request(
            Method::Delete,
            &format!("/channels/{channel_id}/messages/{message_id}"),
            None,
        )
        .await
        .map(|_| ())
    }

    /// Replace the application's global commands (bulk overwrite).
    pub async fn set_global_commands(
        &self,
        application_id: &str,
        commands: &[Value],
    ) -> Result<Value> {
        self.request(
            Method::Put,
            &format!("/applications/{application_id}/commands"),
            Some(Value::Array(commands.to_vec())),
        )
        .await
    }

    /// Replace the application's commands in one guild.
    pub async fn set_guild_commands(
        &self,
        application_id: &str,
        guild_id: &str,
        commands: &[Value],
    ) -> Result<Value> {
        self.request(
            Method::Put,
            &format!("/applications/{application_id}/guilds/{guild_id}/commands"),
            Some(Value::Array(commands.to_vec())),
        )
        .await
    }

    /// First response to an interaction (`POST /interactions/{id}/{token}/callback`).
    pub async fn interaction_callback(
        &self,
        interaction: &Interaction,
        kind: CallbackType,
        data: Option<Value>,
    ) -> Result<()> {
        let mut body = json!({ "type": kind as u8 });
        if let Some(d) = data {
            body["data"] = d;
        }
        self.request_anon(
            Method::Post,
            &format!(
                "/interactions/{}/{}/callback",
                interaction.id, interaction.token
            ),
            Some(body),
        )
        .await
        .map(|_| ())
    }

    /// Follow-up message (`POST /webhooks/{application_id}/{token}?wait=true`).
    pub async fn interaction_followup(
        &self,
        interaction: &Interaction,
        message: impl Into<MessageBody>,
    ) -> Result<Message> {
        let v = self
            .request_anon(
                Method::Post,
                &format!(
                    "/webhooks/{}/{}?wait=true",
                    interaction.application_id, interaction.token
                ),
                Some(message.into().into_json()),
            )
            .await?;
        Ok(serde_json::from_value(v)?)
    }

    /// Edit the original response (the first edit after a deferral creates it).
    pub async fn interaction_edit_original(
        &self,
        interaction: &Interaction,
        message: impl Into<MessageBody>,
    ) -> Result<Value> {
        self.request_anon(
            Method::Patch,
            &format!(
                "/webhooks/{}/{}/messages/@original",
                interaction.application_id, interaction.token
            ),
            Some(message.into().into_json()),
        )
        .await
    }

    pub async fn interaction_delete_original(&self, interaction: &Interaction) -> Result<()> {
        self.request_anon(
            Method::Delete,
            &format!(
                "/webhooks/{}/{}/messages/@original",
                interaction.application_id, interaction.token
            ),
            None,
        )
        .await
        .map(|_| ())
    }
}

/// Body of a message the bot sends or edits.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MessageBody {
    pub content: Option<String>,
    pub embeds: Vec<Value>,
    pub components: Vec<Value>,
    pub flags: u64,
}

impl MessageBody {
    pub fn content(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            ..Self::default()
        }
    }

    /// Only the invoking user sees it (interaction responses only).
    pub fn ephemeral(mut self) -> Self {
        self.flags |= crate::model::FLAG_EPHEMERAL;
        self
    }

    pub fn embed(mut self, embed: Value) -> Self {
        self.embeds.push(embed);
        self
    }

    pub fn components(mut self, rows: Vec<Value>) -> Self {
        self.components = rows;
        self
    }

    pub fn into_json(self) -> Value {
        let mut o = serde_json::Map::new();
        if let Some(c) = self.content {
            o.insert("content".into(), Value::String(c));
        }
        if !self.embeds.is_empty() {
            o.insert("embeds".into(), Value::Array(self.embeds));
        }
        if !self.components.is_empty() {
            o.insert("components".into(), Value::Array(self.components));
        }
        if self.flags != 0 {
            o.insert("flags".into(), json!(self.flags));
        }
        Value::Object(o)
    }
}

impl From<&str> for MessageBody {
    fn from(s: &str) -> Self {
        Self::content(s)
    }
}

impl From<String> for MessageBody {
    fn from(s: String) -> Self {
        Self::content(s)
    }
}

fn retry_after(resp: &HttpResponse) -> Duration {
    let from_body = serde_json::from_slice::<Value>(&resp.body)
        .ok()
        .and_then(|v| v.get("retry_after").and_then(Value::as_f64));
    let secs = from_body
        .or_else(|| {
            resp.header("retry-after")
                .and_then(|h| h.trim().parse::<f64>().ok())
        })
        .unwrap_or(1.0);
    Duration::from_secs_f64(secs.clamp(0.0, 3600.0))
}

fn decode(resp: HttpResponse) -> Result<Value> {
    if (200..300).contains(&resp.status) {
        if resp.body.is_empty() {
            return Ok(Value::Null);
        }
        return serde_json::from_slice(&resp.body).map_err(Error::from);
    }
    let v: Value = serde_json::from_slice(&resp.body).unwrap_or(Value::Null);
    Err(Error::Api {
        status: resp.status,
        code: v
            .get("code")
            .and_then(Value::as_u64)
            .map_or(0, |c| u32::try_from(c).unwrap_or(0)),
        message: v
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("request failed")
            .to_owned(),
        errors: v.get("errors").cloned(),
    })
}

#[cfg(test)]
pub(crate) mod fake {
    //! A scripted transport for tests.
    use std::sync::Mutex;

    use super::*;

    pub struct FakeTransport {
        pub seen: Mutex<Vec<HttpRequest>>,
        replies: Mutex<Vec<HttpResponse>>,
    }

    impl FakeTransport {
        pub fn new(replies: Vec<HttpResponse>) -> Arc<Self> {
            let mut r = replies;
            r.reverse();
            Arc::new(Self {
                seen: Mutex::new(Vec::new()),
                replies: Mutex::new(r),
            })
        }

        pub fn json(status: u16, v: Value) -> HttpResponse {
            HttpResponse {
                status,
                headers: vec![],
                body: v.to_string().into_bytes(),
            }
        }

        pub fn no_content() -> HttpResponse {
            HttpResponse {
                status: 204,
                headers: vec![],
                body: vec![],
            }
        }
    }

    #[async_trait]
    impl HttpTransport for FakeTransport {
        async fn send(&self, req: HttpRequest) -> Result<HttpResponse> {
            self.seen.lock().expect("lock").push(req);
            self.replies
                .lock()
                .expect("lock")
                .pop()
                .ok_or_else(|| Error::Transport("no scripted reply".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeTransport;
    use super::*;

    fn rest(t: Arc<FakeTransport>) -> Rest {
        Rest::with_transport("https://x.test/api/v1/", "Bot tok123", t)
    }

    fn body_of(req: &HttpRequest) -> Value {
        serde_json::from_slice(req.body.as_ref().expect("body")).expect("json")
    }

    #[tokio::test]
    async fn sends_bot_auth_and_json_body_and_decodes_a_message() {
        let t = FakeTransport::new(vec![FakeTransport::json(
            200,
            json!({"id": "1", "channel_id": "2", "content": "hi", "author": {"id": "9", "bot": true}}),
        )]);
        let r = rest(t.clone());
        let m = r.create_message("2", "hi").await.expect("sent");
        assert_eq!(m.id, "1");
        assert!(m.author.bot);
        let seen = t.seen.lock().expect("lock");
        assert_eq!(seen[0].url, "https://x.test/api/v1/channels/2/messages");
        assert!(
            seen[0]
                .headers
                .contains(&("Authorization".into(), "Bot tok123".into()))
        );
        assert_eq!(body_of(&seen[0]), json!({"content": "hi"}));
    }

    #[tokio::test]
    async fn api_errors_carry_status_and_code() {
        let t = FakeTransport::new(vec![FakeTransport::json(
            403,
            json!({"code": 50013, "message": "Missing Permissions"}),
        )]);
        let err = rest(t).delete_message("1", "2").await.expect_err("denied");
        assert_eq!(err.api_code(), Some(50013));
        assert!(matches!(err, Error::Api { status: 403, .. }));
    }

    #[tokio::test(start_paused = true)]
    async fn a_429_is_retried_after_the_advertised_delay() {
        let t = FakeTransport::new(vec![
            FakeTransport::json(429, json!({"retry_after": 0.5, "global": false})),
            FakeTransport::json(200, json!({"id": "7", "username": "bot"})),
        ]);
        let u = rest(t.clone()).current_user().await.expect("retried");
        assert_eq!(u.id, "7");
        assert_eq!(t.seen.lock().expect("lock").len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limit_gives_up_after_the_retry_budget() {
        let replies = (0..5)
            .map(|_| FakeTransport::json(429, json!({"retry_after": 1})))
            .collect();
        let err = rest(FakeTransport::new(replies))
            .current_user()
            .await
            .expect_err("limited");
        assert!(matches!(err, Error::RateLimited { .. }));
    }

    #[tokio::test]
    async fn interaction_routes_omit_the_bot_credential() {
        let t = FakeTransport::new(vec![FakeTransport::no_content()]);
        let i = Interaction {
            id: "11".into(),
            application_id: "5".into(),
            kind: 2,
            token: "itok".into(),
            ..Interaction::default()
        };
        rest(t.clone())
            .interaction_callback(
                &i,
                CallbackType::ChannelMessage,
                Some(MessageBody::content("ok").ephemeral().into_json()),
            )
            .await
            .expect("callback");
        let seen = t.seen.lock().expect("lock");
        assert_eq!(
            seen[0].url,
            "https://x.test/api/v1/interactions/11/itok/callback"
        );
        assert!(!seen[0].headers.iter().any(|(k, _)| k == "Authorization"));
        assert_eq!(
            body_of(&seen[0]),
            json!({"type": 4, "data": {"content": "ok", "flags": 64}})
        );
    }

    #[tokio::test]
    async fn gateway_bot_returns_url_and_shards() {
        let t = FakeTransport::new(vec![FakeTransport::json(
            200,
            json!({"url": "wss://x.test/gateway", "shards": 3}),
        )]);
        assert_eq!(
            rest(t).gateway_bot().await.expect("ok"),
            ("wss://x.test/gateway".into(), 3)
        );
    }
}
