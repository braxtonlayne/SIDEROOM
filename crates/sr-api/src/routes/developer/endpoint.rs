//! HTTP interactions endpoint (DEV-BOT-005): signed delivery of interactions to an
//! application's own server, and the verification ping run when the URL is saved.
//!
//! Requests carry `X-Signature-Ed25519` (hex) and `X-Signature-Timestamp` (unix seconds);
//! the signature covers `timestamp || body` and is made with the application's key
//! ([`sr_apps::signing`]). The endpoint answers a type 1 PING with `{"type":1}` and rejects
//! a request with a bad signature with 401; the answer to a real interaction is the same
//! JSON body the callback route takes (`{type, data}`).

use super::{CallbackBody, DevDeps, SystemCard, interactions::apply_callback};
use crate::routes::messages::MessagesDeps;
use async_trait::async_trait;
use serde_json::{Value, json};
use sr_apps::{model::InteractionRow, service::EndpointTarget, signing::SigningSeed};
use std::{sync::Arc, time::Duration};

/// Largest reply accepted from an endpoint (clients must enforce it).
pub const MAX_ENDPOINT_REPLY: usize = 256 * 1024;
/// Each verification request may take this long.
const PING_TIMEOUT: Duration = Duration::from_secs(5);

/// A reply from the developer's server.
#[derive(Debug, Clone)]
pub struct EndpointReply {
    /// HTTP status.
    pub status: u16,
    /// Body (capped).
    pub body: Vec<u8>,
}

/// Sends one signed POST to a developer-supplied URL. The production implementation sits on
/// the SSRF-safe client (`sr-net`); tests script it.
#[async_trait]
pub trait EndpointClient: Send + Sync {
    /// POST `body` (JSON) with `headers`. `Err` carries a short reason that is safe to show
    /// to the application's owner (no URL, no secrets).
    async fn post(
        &self,
        url: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        timeout: Duration,
    ) -> Result<EndpointReply, String>;
}

fn signed_headers(seed: &SigningSeed, now_secs: i64, body: &[u8]) -> Vec<(String, String)> {
    let ts = now_secs.to_string();
    vec![
        ("X-Signature-Ed25519".to_owned(), seed.sign_hex(&ts, body)),
        ("X-Signature-Timestamp".to_owned(), ts),
    ]
}

impl DevDeps {
    fn endpoint_client(&self) -> Option<&Arc<dyn EndpointClient>> {
        self.endpoint.as_ref()
    }

    /// Whether HTTP endpoints can be used at all (a client is wired).
    pub(crate) fn endpoints_available(&self) -> bool {
        self.endpoint.is_some()
    }

    /// Run the verification a new URL must pass: a signed PING answered with `{"type":1}`,
    /// and a PING with a wrong signature rejected with 401. `Err(reason)` is shown to the
    /// owner as a field error.
    pub(crate) async fn verify_endpoint(
        &self,
        app_id: i64,
        url: &str,
        seed: &SigningSeed,
    ) -> Result<(), String> {
        let client = self.endpoint_client().ok_or_else(|| {
            "HTTP interactions endpoints are not available on this server.".to_owned()
        })?;
        let body = serde_json::to_vec(&json!({
            "id": "0",
            "application_id": app_id.to_string(),
            "type": 1,
            "version": 1,
        }))
        .map_err(|_| "Could not build the verification request.".to_owned())?;
        let now = self.now_ms().div_euclid(1000);

        let ok = client
            .post(
                url,
                signed_headers(seed, now, &body),
                body.clone(),
                PING_TIMEOUT,
            )
            .await
            .map_err(|e| format!("The endpoint could not be reached: {e}"))?;
        let pong = serde_json::from_slice::<Value>(&ok.body)
            .ok()
            .and_then(|v| v.get("type").and_then(Value::as_i64));
        if ok.status != 200 || pong != Some(1) {
            return Err(format!(
                "The endpoint must answer a signed PING with 200 and {{\"type\":1}} (it answered {}).",
                ok.status
            ));
        }

        // A second request signed with a throwaway key must be refused.
        let wrong = SigningSeed::generate().map_err(|_| "No randomness available.".to_owned())?;
        let bad = client
            .post(url, signed_headers(&wrong, now, &body), body, PING_TIMEOUT)
            .await
            .map_err(|e| format!("The endpoint could not be reached: {e}"))?;
        if bad.status != 401 {
            return Err(
                "The endpoint must reject a request with an invalid signature with 401.".to_owned(),
            );
        }
        Ok(())
    }

    /// Count one delivery outcome; the 25th consecutive failure inside ten minutes disables
    /// the endpoint (dispatch then uses the gateway) and the owner gets a system DM (D-186).
    async fn report_delivery(&self, app: i64, ok: bool) {
        match self.apps.record_delivery(app, ok).await {
            Ok(true) => {
                let Ok(a) = self.apps.application(app).await else {
                    return;
                };
                self.dm_user(
                    a.owner_id,
                    SystemCard {
                        content: format!(
                            "The interactions endpoint of {} was switched off.",
                            a.name
                        ),
                        title: "Interactions endpoint disabled".to_owned(),
                        description: format!(
                            "{} failed to answer 25 interactions in a row within ten minutes, so SIDE ROOM switched the endpoint off and sends interactions through the gateway again. Fix your server, then open the developer portal and choose Verify and re-enable.",
                            a.name
                        ),
                        buttons: Vec::new(),
                    },
                )
                .await;
            }
            Ok(false) => {}
            Err(e) => tracing::warn!(app, error = ?e, "could not record an endpoint delivery"),
        }
    }

    /// Hand an interaction to the application's endpoint and apply its reply as the
    /// callback. Failures are logged without the URL or payload; the 3 second window then
    /// reports the failure to the invoker.
    pub(crate) fn deliver_to_endpoint(
        self: &Arc<Self>,
        messages: Arc<MessagesDeps>,
        target: EndpointTarget,
        row: InteractionRow,
        payload: Value,
    ) {
        let deps = self.clone();
        tokio::spawn(async move {
            let Some(client) = deps.endpoint_client().cloned() else {
                tracing::warn!(
                    app = row.application_id,
                    "interactions endpoint set but no client wired"
                );
                return;
            };
            let Ok(body) = serde_json::to_vec(&payload) else {
                return;
            };
            let headers = signed_headers(&target.seed, deps.now_ms().div_euclid(1000), &body);
            let reply = match client
                .post(&target.url, headers, body, deps.ack_window())
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(app = row.application_id, reason = %e, "interactions endpoint delivery failed");
                    deps.report_delivery(row.application_id, false).await;
                    return;
                }
            };
            if reply.status != 200 {
                tracing::warn!(
                    app = row.application_id,
                    status = reply.status,
                    "interactions endpoint refused an interaction"
                );
                deps.report_delivery(row.application_id, false).await;
                return;
            }
            let Ok(cb) = serde_json::from_slice::<CallbackBody>(&reply.body) else {
                tracing::warn!(
                    app = row.application_id,
                    "interactions endpoint sent an unreadable reply"
                );
                deps.report_delivery(row.application_id, false).await;
                return;
            };
            deps.report_delivery(row.application_id, true).await;
            if let Err(e) = apply_callback(&deps, &messages, row.clone(), cb).await {
                tracing::warn!(app = row.application_id, error = ?e, "interactions endpoint reply was not applied");
            }
        });
    }
}
