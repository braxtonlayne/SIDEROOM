//! The HTTP interactions endpoint and its signing key (DEV-BOT-005).

use super::AppsService;
use crate::{
    error::{AppsError, AppsResult, FieldProblem},
    model::{EndpointState, InteractionsConfig},
    signing::SigningSeed,
};
use serde_json::json;

/// What the owner sees of the endpoint settings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EndpointInfo {
    /// The configured URL.
    pub url: Option<String>,
    /// Public key (hex) once a key exists.
    pub verify_key: Option<String>,
    /// `disabled` after repeated delivery failures (D-186).
    pub state: EndpointState,
}

/// Consecutive failed deliveries that disable an endpoint (D-186).
pub const ENDPOINT_FAILURE_LIMIT: i32 = 25;
/// The failures must fall within this window (10 minutes).
pub const ENDPOINT_FAILURE_WINDOW_MS: i64 = 10 * 60 * 1000;

/// Fold one delivery outcome into the health counters of an endpoint. A success resets the
/// streak; a failure extends it, or starts a new one when the last streak began more than
/// ten minutes ago. Returns `true` when this failure was the 25th of a streak and flipped the
/// endpoint to `disabled`. A disabled endpoint is left alone.
pub fn endpoint_health_step(cfg: &mut InteractionsConfig, ok: bool, now_ms: i64) -> bool {
    if cfg.state == EndpointState::Disabled {
        return false;
    }
    if ok {
        cfg.failures = 0;
        cfg.failure_window_start_ms = None;
        return false;
    }
    match cfg.failure_window_start_ms {
        Some(start) if now_ms - start <= ENDPOINT_FAILURE_WINDOW_MS => cfg.failures += 1,
        _ => {
            cfg.failures = 1;
            cfg.failure_window_start_ms = Some(now_ms);
        }
    }
    if cfg.failures >= ENDPOINT_FAILURE_LIMIT {
        cfg.state = EndpointState::Disabled;
        return true;
    }
    false
}

/// What the delivery code needs: URL and key, only when both exist.
#[derive(Debug, Clone)]
pub struct EndpointTarget {
    /// Where to POST.
    pub url: String,
    /// The signing key.
    pub seed: SigningSeed,
}

/// Why a URL cannot be an interactions endpoint, or `None` when it can: `https`, a host, at
/// most 512 characters, no credentials, no fragment, no spaces or control characters.
pub fn endpoint_url_problem(u: &str) -> Option<&'static str> {
    let Some(rest) = u.strip_prefix("https://") else {
        return Some("Must be an https URL.");
    };
    if u.len() > 512 {
        return Some("Must be 512 characters or fewer.");
    }
    if u.chars().any(|c| c.is_control() || c == ' ' || c == '\\') || u.contains('#') {
        return Some("Must not contain spaces, control characters or a fragment.");
    }
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    if authority.is_empty() || authority.starts_with(':') {
        return Some("Must name a host.");
    }
    if authority.contains('@') {
        return Some("Must not contain credentials.");
    }
    None
}

impl AppsService {
    /// Endpoint URL and public key for the owner's view. Never creates a key.
    pub async fn endpoint_info(&self, app: i64) -> AppsResult<EndpointInfo> {
        let c = self.repo.interactions_config(app).await?;
        Ok(EndpointInfo {
            url: c.endpoint_url,
            verify_key: c.seed.as_ref().map(SigningSeed::verify_key_hex),
            state: c.state,
        })
    }

    /// Create the signing key when the application has none (idempotent). Owner only.
    pub async fn ensure_signing_key(&self, owner: i64, id: i64) -> AppsResult<EndpointInfo> {
        self.owned(owner, id).await?;
        let c = self.repo.interactions_config(id).await?;
        if c.seed.is_none() {
            let seed = SigningSeed::generate().map_err(|e| AppsError::Storage(e.to_string()))?;
            let audit = self.audit(id, owner, "app.signing_key_created", json!({}));
            self.repo
                .set_interactions_config(id, None, Some(seed), Some(audit))
                .await?;
        }
        self.endpoint_info(id).await
    }

    /// Validate a candidate endpoint URL (no network access).
    pub fn check_endpoint_url(url: &str) -> AppsResult<()> {
        match endpoint_url_problem(url) {
            None => Ok(()),
            Some(m) => Err(AppsError::Invalid(vec![FieldProblem::new(
                "interactions_endpoint_url",
                "URL_TYPE_INVALID_URL",
                m,
            )])),
        }
    }

    /// Store (or clear) the endpoint URL after the caller verified it. Makes sure a key
    /// exists. Owner only.
    pub async fn set_endpoint(
        &self,
        owner: i64,
        id: i64,
        url: Option<String>,
    ) -> AppsResult<EndpointInfo> {
        self.owned(owner, id).await?;
        if let Some(u) = &url {
            Self::check_endpoint_url(u)?;
        }
        let seed = match self.repo.interactions_config(id).await?.seed {
            Some(_) => None,
            None => Some(SigningSeed::generate().map_err(|e| AppsError::Storage(e.to_string()))?),
        };
        let audit = self.audit(
            id,
            owner,
            if url.is_some() {
                "app.interactions_endpoint_set"
            } else {
                "app.interactions_endpoint_cleared"
            },
            json!({"url": url}),
        );
        self.repo
            .set_interactions_config(id, Some(url), seed, Some(audit))
            .await?;
        self.endpoint_info(id).await
    }

    /// The signing seed of an application that has one (for the verification ping).
    pub async fn signing_seed(&self, app: i64) -> AppsResult<SigningSeed> {
        self.repo
            .interactions_config(app)
            .await?
            .seed
            .ok_or(AppsError::NotFound("signing key"))
    }

    /// Fold one delivery to the HTTP endpoint into its health counters (D-186). `true` when
    /// this failure disabled the endpoint: the caller then tells the owner.
    pub async fn record_delivery(&self, app: i64, ok: bool) -> AppsResult<bool> {
        let flipped = self
            .repo
            .record_endpoint_outcome(app, ok, self.now_ms())
            .await?;
        if flipped {
            // Same transaction would be nicer; the audit row is informational.
            let audit = self.audit(
                app,
                1,
                "app.interactions_endpoint_disabled",
                json!({"failures": ENDPOINT_FAILURE_LIMIT}),
            );
            self.repo.record_audit(audit).await?;
        }
        Ok(flipped)
    }

    /// Mark a disabled endpoint enabled again after the owner's PING check passed. Owner only.
    pub async fn reenable_endpoint(&self, owner: i64, id: i64) -> AppsResult<EndpointInfo> {
        self.owned(owner, id).await?;
        let audit = self.audit(id, owner, "app.interactions_endpoint_reenabled", json!({}));
        self.repo
            .set_endpoint_state(id, EndpointState::Enabled, audit)
            .await?;
        self.endpoint_info(id).await
    }

    /// URL and key for delivering an interaction, `None` when the application uses the
    /// gateway (no URL, no key, or the endpoint was disabled after repeated failures).
    pub async fn endpoint_target(&self, app: i64) -> AppsResult<Option<EndpointTarget>> {
        let c = self.repo.interactions_config(app).await?;
        if c.state == EndpointState::Disabled {
            return Ok(None);
        }
        Ok(match (c.endpoint_url, c.seed) {
            (Some(url), Some(seed)) => Some(EndpointTarget { url, seed }),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::endpoint_url_problem as p;

    #[test]
    fn urls() {
        assert_eq!(p("https://bot.example.com/interactions"), None);
        assert_eq!(p("https://bot.example.com:8443/x?y=1"), None);
        for bad in [
            "http://bot.example.com/",
            "ftp://x/",
            "https://",
            "https:///x",
            "https://:80/x",
            "https://user:pw@bot.example.com/",
            "https://bot.example.com/#frag",
            "https://bot.example.com/a b",
            "https://bot.example.com\\x",
        ] {
            assert!(p(bad).is_some(), "{bad}");
        }
        assert!(p(&format!("https://a.com/{}", "x".repeat(520))).is_some());
    }
}
