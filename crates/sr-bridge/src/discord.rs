//! The Discord REST calls the bridge makes, over `sr-sdk`'s HTTP transport.
//!
//! Only the routes the bridge needs: who the bot is, which servers and channels it sees, and the webhook life
//! cycle (create, execute, edit a message, delete a message, delete the webhook). Webhook calls carry their
//! credential in the URL (`/webhooks/{id}/{token}`), so nothing here ever puts a URL into an error or a log:
//! [`redact`] hides the token part of every string that leaves this module.
//!
//! Rate limits: Discord answers every call with `X-RateLimit-Remaining` and `X-RateLimit-Reset-After`. A
//! webhook whose bucket is empty is *blocked* for the time Discord named ([`DiscordApi::blocked_for`]), and a
//! 429 blocks it for `retry_after` (a global 429 blocks everything). The queue consults the block before a
//! send, so a busy channel is paced instead of hammering and being rejected. Tests run against the fake
//! Discord, which sends the same headers and a 429 when asked to.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Value, json};
use sr_sdk::http::{HttpRequest, HttpResponse, HttpTransport, Method, ReqwestTransport};

use crate::error::{BridgeError, BridgeResult};

/// The default API root.
pub const DEFAULT_API_BASE: &str = "https://discord.com/api/v10";
/// A wait Discord asks for beyond this is capped, so one bad header cannot park the queue for an hour.
const MAX_BLOCK: Duration = Duration::from_secs(120);

/// Why a Discord call did not succeed.
#[derive(Debug, Clone, PartialEq)]
pub enum CallError {
    /// 429: try again after this long.
    RateLimited(Duration),
    /// The webhook, message or channel is gone (404, or 401/403 for a deleted webhook).
    Gone(u16),
    /// Any other refusal.
    Status { status: u16, message: String },
    /// The request never got an answer.
    Transport(String),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimited(d) => write!(f, "rate limited for {} ms", d.as_millis()),
            Self::Gone(s) => write!(f, "gone ({s})"),
            Self::Status { status, message } => write!(f, "status {status}: {message}"),
            Self::Transport(m) => write!(f, "transport: {m}"),
        }
    }
}

impl From<CallError> for BridgeError {
    fn from(e: CallError) -> Self {
        BridgeError::Discord(e.to_string())
    }
}

/// Hide the token of every `/webhooks/{id}/{token}` in `s` (and any `Bot ...` credential).
pub fn redact(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("/webhooks/") {
        let (head, tail) = rest.split_at(i + "/webhooks/".len());
        out.push_str(head);
        let id_end = tail
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(tail.len());
        out.push_str(&tail[..id_end]);
        let after = &tail[id_end..];
        if let Some(t) = after.strip_prefix('/') {
            // The token runs to the next separator.
            let end = t
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'))
                .unwrap_or(t.len());
            if end == 0 {
                out.push('/');
            } else {
                out.push_str("/<redacted>");
            }
            rest = &t[end..];
        } else {
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// Who a bot token belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotUser {
    pub id: i64,
    pub name: String,
    pub bot: bool,
}

/// A Discord server the bot is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordGuild {
    pub id: i64,
    pub name: String,
}

/// A Discord channel the bot sees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordChannel {
    pub id: i64,
    pub name: String,
    /// Discord channel type (0 text, 5 announcement).
    pub kind: i64,
}

/// A webhook the bridge created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedWebhook {
    pub id: i64,
    pub token: String,
}

/// What a posted message needs on the Discord side.
#[derive(Debug, Clone, PartialEq)]
pub struct WebhookPost {
    pub content: String,
    pub username: String,
    pub avatar_url: Option<String>,
    /// Bridged Discord members the content mentions as `<@id>`: the only pings allowed.
    pub ping_users: Vec<String>,
}

/// `allowed_mentions` of a mirrored post: nobody (`@everyone`, roles, users) except the bridged members named.
fn allowed_mentions(ping_users: &[String]) -> Value {
    if ping_users.is_empty() {
        json!({ "parse": [] })
    } else {
        json!({ "parse": [], "users": ping_users })
    }
}

impl WebhookPost {
    fn body(&self) -> Value {
        let mut v = json!({
            "content": self.content,
            "username": self.username,
            "allowed_mentions": allowed_mentions(&self.ping_users),
        });
        if let Some(a) = &self.avatar_url {
            v["avatar_url"] = json!(a);
        }
        v
    }
}

/// The Discord client of the bridge. Cheap to clone; the rate-limit state is shared.
#[derive(Clone)]
pub struct DiscordApi {
    base: String,
    transport: Arc<dyn HttpTransport>,
    blocks: Arc<Mutex<HashMap<String, Instant>>>,
}

impl std::fmt::Debug for DiscordApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscordApi")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

fn header<'a>(r: &'a HttpResponse, name: &str) -> Option<&'a str> {
    r.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

fn secs(v: f64) -> Duration {
    Duration::from_secs_f64(v.clamp(0.0, MAX_BLOCK.as_secs_f64()))
}

#[derive(Deserialize)]
struct IdName {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    bot: bool,
    #[serde(default, rename = "type")]
    kind: i64,
    #[serde(default)]
    token: Option<String>,
}

fn snowflake(s: &str) -> BridgeResult<i64> {
    s.parse::<i64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| BridgeError::Discord("an id from Discord was not a snowflake".into()))
}

impl DiscordApi {
    /// The real client over rustls, talking to `base` (the API root).
    pub fn new(base: impl Into<String>) -> BridgeResult<Self> {
        let transport =
            ReqwestTransport::new().map_err(|e| BridgeError::Discord(redact(&e.to_string())))?;
        Ok(Self::with_transport(base, Arc::new(transport)))
    }

    pub fn with_transport(base: impl Into<String>, transport: Arc<dyn HttpTransport>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_owned(),
            transport,
            blocks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// How long the key (`wh:<webhook id>`) must still wait before the next call, if at all.
    pub fn blocked_for(&self, key: &str) -> Option<Duration> {
        let now = Instant::now();
        let blocks = self.blocks.lock().ok()?;
        [blocks.get(key), blocks.get("global")]
            .into_iter()
            .flatten()
            .filter_map(|until| until.checked_duration_since(now))
            .max()
    }

    fn block(&self, key: &str, wait: Duration) {
        if let Ok(mut b) = self.blocks.lock() {
            let until = Instant::now() + wait.min(MAX_BLOCK);
            let slot = b.entry(key.to_owned()).or_insert(until);
            if *slot < until {
                *slot = until;
            }
            // Keep the map from growing without bound: drop entries long past.
            if b.len() > 4096 {
                let now = Instant::now();
                b.retain(|_, u| *u > now);
            }
        }
    }

    /// The rate-limit key of a webhook.
    pub fn webhook_key(id: i64) -> String {
        format!("wh:{id}")
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        bot_token: Option<&str>,
        body: Option<Value>,
        key: Option<&str>,
    ) -> Result<Value, CallError> {
        let mut headers = vec![
            (
                "User-Agent".to_owned(),
                "DiscordBot (sideroom-bridge, 1)".to_owned(),
            ),
            ("Accept".to_owned(), "application/json".to_owned()),
        ];
        if let Some(t) = bot_token {
            headers.push(("Authorization".into(), format!("Bot {t}")));
        }
        let body_bytes = body.map(|b| b.to_string().into_bytes());
        if body_bytes.is_some() {
            headers.push(("Content-Type".into(), "application/json".into()));
        }
        let resp = self
            .transport
            .send(HttpRequest {
                method,
                url: format!("{}{}", self.base, path),
                headers,
                body: body_bytes,
            })
            .await
            .map_err(|e| CallError::Transport(redact(&e.to_string())))?;
        if let Some(key) = key {
            let remaining =
                header(&resp, "x-ratelimit-remaining").and_then(|v| v.trim().parse::<i64>().ok());
            let reset_after =
                header(&resp, "x-ratelimit-reset-after").and_then(|v| v.trim().parse::<f64>().ok());
            if remaining == Some(0)
                && let Some(r) = reset_after
            {
                self.block(key, secs(r));
            }
        }
        if resp.status == 429 {
            let v: Value = serde_json::from_slice(&resp.body).unwrap_or(Value::Null);
            let wait = v
                .get("retry_after")
                .and_then(Value::as_f64)
                .or_else(|| header(&resp, "retry-after").and_then(|h| h.trim().parse().ok()))
                .map_or(Duration::from_secs(1), secs);
            let global = v.get("global").and_then(Value::as_bool).unwrap_or(false)
                || header(&resp, "x-ratelimit-global").is_some();
            self.block(
                if global {
                    "global"
                } else {
                    key.unwrap_or("global")
                },
                wait,
            );
            return Err(CallError::RateLimited(wait));
        }
        if (200..300).contains(&resp.status) {
            if resp.body.is_empty() {
                return Ok(Value::Null);
            }
            return serde_json::from_slice(&resp.body)
                .map_err(|_| CallError::Transport("unreadable answer from Discord".into()));
        }
        let v: Value = serde_json::from_slice(&resp.body).unwrap_or(Value::Null);
        let message = redact(
            v.get("message")
                .and_then(Value::as_str)
                .unwrap_or("request failed"),
        );
        if matches!(resp.status, 404) {
            return Err(CallError::Gone(resp.status));
        }
        Err(CallError::Status {
            status: resp.status,
            message,
        })
    }

    // ---- who the bot is, and what it sees ----

    /// `GET /users/@me`.
    pub async fn bot_user(&self, token: &str) -> Result<BotUser, CallError> {
        let v = self
            .call(Method::Get, "/users/@me", Some(token), None, None)
            .await?;
        let u: IdName = serde_json::from_value(v)
            .map_err(|_| CallError::Transport("unreadable user from Discord".into()))?;
        Ok(BotUser {
            id: snowflake(&u.id).map_err(|e| CallError::Transport(e.to_string()))?,
            name: u.username,
            bot: u.bot,
        })
    }

    /// `GET /users/@me/guilds`.
    pub async fn bot_guilds(&self, token: &str) -> Result<Vec<DiscordGuild>, CallError> {
        let v = self
            .call(Method::Get, "/users/@me/guilds", Some(token), None, None)
            .await?;
        let list: Vec<IdName> = serde_json::from_value(v)
            .map_err(|_| CallError::Transport("unreadable guild list from Discord".into()))?;
        Ok(list
            .into_iter()
            .filter_map(|g| {
                Some(DiscordGuild {
                    id: snowflake(&g.id).ok()?,
                    name: g.name,
                })
            })
            .collect())
    }

    /// `GET /guilds/{id}/channels`, text and announcement channels only.
    pub async fn guild_channels(
        &self,
        token: &str,
        guild: i64,
    ) -> Result<Vec<DiscordChannel>, CallError> {
        let v = self
            .call(
                Method::Get,
                &format!("/guilds/{guild}/channels"),
                Some(token),
                None,
                None,
            )
            .await?;
        let list: Vec<IdName> = serde_json::from_value(v)
            .map_err(|_| CallError::Transport("unreadable channel list from Discord".into()))?;
        Ok(list
            .into_iter()
            .filter(|c| matches!(c.kind, 0 | 5))
            .filter_map(|c| {
                Some(DiscordChannel {
                    id: snowflake(&c.id).ok()?,
                    name: c.name,
                    kind: c.kind,
                })
            })
            .collect())
    }

    // ---- webhooks ----

    /// `POST /channels/{id}/webhooks`.
    pub async fn create_webhook(
        &self,
        token: &str,
        channel: i64,
        name: &str,
    ) -> Result<CreatedWebhook, CallError> {
        let v = self
            .call(
                Method::Post,
                &format!("/channels/{channel}/webhooks"),
                Some(token),
                Some(json!({ "name": name })),
                None,
            )
            .await?;
        let w: IdName = serde_json::from_value(v)
            .map_err(|_| CallError::Transport("unreadable webhook from Discord".into()))?;
        let token = w.token.filter(|t| !t.is_empty()).ok_or_else(|| {
            CallError::Transport("Discord returned a webhook without a token".into())
        })?;
        Ok(CreatedWebhook {
            id: snowflake(&w.id).map_err(|e| CallError::Transport(e.to_string()))?,
            token,
        })
    }

    /// `POST /webhooks/{id}/{token}?wait=true`: the id of the posted message.
    pub async fn execute_webhook(
        &self,
        id: i64,
        wtoken: &str,
        post: &WebhookPost,
    ) -> Result<i64, CallError> {
        let key = Self::webhook_key(id);
        let v = self
            .call(
                Method::Post,
                &format!("/webhooks/{id}/{wtoken}?wait=true"),
                None,
                Some(post.body()),
                Some(&key),
            )
            .await?;
        let mid = v
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| CallError::Transport("Discord returned no message id".into()))?;
        snowflake(mid).map_err(|e| CallError::Transport(e.to_string()))
    }

    /// `PATCH /webhooks/{id}/{token}/messages/{message}`.
    pub async fn edit_webhook_message(
        &self,
        id: i64,
        wtoken: &str,
        message: i64,
        content: &str,
        ping_users: &[String],
    ) -> Result<(), CallError> {
        let key = Self::webhook_key(id);
        self.call(
            Method::Patch,
            &format!("/webhooks/{id}/{wtoken}/messages/{message}"),
            None,
            Some(json!({ "content": content, "allowed_mentions": allowed_mentions(ping_users) })),
            Some(&key),
        )
        .await
        .map(|_| ())
    }

    /// `DELETE /webhooks/{id}/{token}/messages/{message}`.
    pub async fn delete_webhook_message(
        &self,
        id: i64,
        wtoken: &str,
        message: i64,
    ) -> Result<(), CallError> {
        let key = Self::webhook_key(id);
        self.call(
            Method::Delete,
            &format!("/webhooks/{id}/{wtoken}/messages/{message}"),
            None,
            None,
            Some(&key),
        )
        .await
        .map(|_| ())
    }

    /// `DELETE /webhooks/{id}/{token}`.
    pub async fn delete_webhook(&self, id: i64, wtoken: &str) -> Result<(), CallError> {
        let key = Self::webhook_key(id);
        self.call(
            Method::Delete,
            &format!("/webhooks/{id}/{wtoken}"),
            None,
            None,
            Some(&key),
        )
        .await
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_hides_webhook_tokens_only() {
        assert_eq!(
            redact(
                "error sending request for url (https://x.test/api/v10/webhooks/123/abcDEF_-9.x?wait=true)"
            ),
            "error sending request for url (https://x.test/api/v10/webhooks/123/<redacted>?wait=true)"
        );
        assert_eq!(redact("no secrets here"), "no secrets here");
        assert_eq!(
            redact("/webhooks/55/tok/messages/9"),
            "/webhooks/55/<redacted>/messages/9"
        );
        assert_eq!(redact("/webhooks/55"), "/webhooks/55");
    }

    #[test]
    fn a_post_never_pings() {
        let b = WebhookPost {
            content: "@everyone hi".into(),
            username: "ann".into(),
            avatar_url: None,
            ping_users: Vec::new(),
        }
        .body();
        assert_eq!(b["allowed_mentions"]["parse"], json!([]));
        assert!(b["allowed_mentions"].get("users").is_none());
        assert!(b.get("avatar_url").is_none());
        // A bridged member named in the content is the only ping.
        let b = WebhookPost {
            content: "hi <@77>".into(),
            username: "ann".into(),
            avatar_url: None,
            ping_users: vec!["77".into()],
        }
        .body();
        assert_eq!(b["allowed_mentions"], json!({"parse": [], "users": ["77"]}));
    }
}
