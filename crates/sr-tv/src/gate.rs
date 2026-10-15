//! The watch party gate for Live TV (G-16): `tv:<channel>` media. [`TvService`] answers
//! [`PlayGate::resolve_live`]; Cinema titles are not its business, so `resolve` knows nothing.
//!
//! A live party's media names the channel in `title_id` and `asset_id` (the channel snowflake), is flagged
//! `live` and has no duration: nobody seeks, everybody follows the live edge, and each participant tunes the
//! channel themselves (`GET /tv/channels/{id}/tune`) like any viewer. What is on now only decorates the
//! card (`episode_label`); the age check is the same one the tune endpoint applies (D-184).

use async_trait::async_trait;
use sr_watchparty::{GateError, PlayGate, Resolved};

use crate::error::TvError;
use crate::service::{TuneState, TvService, Viewer};

#[async_trait]
impl PlayGate for TvService {
    async fn resolve(
        &self,
        _adult: bool,
        _title_id: i64,
        _episode_id: Option<i64>,
        _variant_id: Option<i64>,
    ) -> Result<Resolved, GateError> {
        Err(GateError::NotFound)
    }

    async fn resolve_live(&self, adult: bool, channel_id: i64) -> Result<Resolved, GateError> {
        let tune = self
            .tune(Viewer { user_id: 0, adult }, channel_id)
            .await
            .map_err(|e| match e {
                TvError::AgeConfirmationRequired => GateError::AgeConfirmationRequired,
                _ => GateError::NotFound,
            })?;
        if tune.state != TuneState::Live {
            return Err(GateError::NotFound);
        }
        let channel = self
            .channel(channel_id)
            .await
            .map_err(|_| GateError::NotFound)?;
        let on_now = tune.current.as_ref();
        Ok(Resolved {
            title_id: channel_id,
            episode_id: 0,
            variant_id: None,
            asset_id: channel_id,
            title: format!("{} {}", channel.number, channel.name),
            episode_label: on_now.map(|b| b.title.clone()),
            duration_ms: None,
            age_restricted: channel.age_restricted || on_now.is_some_and(|b| b.age_restricted),
            live: true,
        })
    }
}
