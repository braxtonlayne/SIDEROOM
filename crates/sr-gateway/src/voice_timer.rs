//! A timer shared with a voice channel (UTL-DEV-001): a pomodoro, stopwatch or countdown one member runs and
//! everyone in the channel sees.
//!
//! * Who: the same people who may set the channel's status line (`SET_VOICE_CHANNEL_STATUS` while connected, or
//!   `MANAGE_CHANNELS`), because publishing a timer also sets the status line ("Focus 24:12"), so members who
//!   never open the tool still see it. The rules are [`crate::voice_status`]'s; this file adds none of its own.
//! * What travels: a small, validated description of the timer (`kind`, `phase`, when the running segment began,
//!   time already counted, length). The server stamps `server_ms` so receivers correct for clock skew; it keeps
//!   no copy. A member who joins later sees the status line, not the live timer, until the owner pauses or
//!   restarts it.
//! * Event: `VOICE_TIMER_UPDATE` `{ id, guild_id, user_id, server_ms, timer }`; `timer` is `null` when it was
//!   stopped. Dispatched to the members who can see the channel, under the `GUILD_VOICE_STATES` intent.
//! * The timer ends with the status: when the last member leaves, clients clear it on the
//!   `VOICE_CHANNEL_STATUS_UPDATE` that says the status is gone.

use serde_json::{Map, Value, json};
use sr_core::{ChannelId, UserId};

use crate::hub::Gateway;
use crate::voice::unix_ms;
use crate::voice_status::StatusError;

/// Longest countdown: 99 hours, 59 minutes, 59 seconds.
const MAX_DURATION_MS: u64 = 99 * 3_600_000 + 59 * 60_000 + 59_000;
/// Time counted so far may not exceed the longest segment (a stopwatch may run for a week).
const MAX_ACCUMULATED_MS: u64 = 7 * 86_400_000;
const MAX_COMPLETED: u64 = 10_000;

/// Check a client's timer and rebuild it with only the known fields, so nothing else rides along.
pub fn sanitize_timer(v: &Value) -> Option<Value> {
    let o = v.as_object()?;
    let kind = o.get("kind")?.as_str()?;
    if !matches!(kind, "pomodoro" | "stopwatch" | "countdown") {
        return None;
    }
    let phase = o.get("phase")?.as_str()?;
    if !matches!(phase, "running" | "paused") {
        return None;
    }
    let segment = match o.get("segment").and_then(Value::as_str) {
        None => "focus",
        Some(s @ ("focus" | "short" | "long")) => s,
        Some(_) => return None,
    };
    let uint = |k: &str, max: u64| -> Option<Option<u64>> {
        match o.get(k) {
            None | Some(Value::Null) => Some(None),
            Some(x) => x.as_u64().filter(|n| *n <= max).map(Some),
        }
    };
    let accumulated = uint("accumulatedMs", MAX_ACCUMULATED_MS)??;
    let duration = uint("durationMs", MAX_DURATION_MS)?;
    if kind != "stopwatch" && duration.is_none() {
        return None;
    }
    if kind == "stopwatch" && duration.is_some() {
        return None;
    }
    let started = uint("startedMs", u64::MAX >> 12)?;
    if phase == "running" && started.is_none() {
        return None;
    }
    let completed = uint("completed", MAX_COMPLETED)?.unwrap_or(0);
    let mut out = Map::new();
    out.insert("kind".into(), kind.into());
    out.insert("phase".into(), phase.into());
    out.insert("segment".into(), segment.into());
    out.insert(
        "startedMs".into(),
        if phase == "running" {
            started.into()
        } else {
            Value::Null
        },
    );
    out.insert("accumulatedMs".into(), accumulated.into());
    out.insert("durationMs".into(), duration.into());
    out.insert("completed".into(), completed.into());
    Some(Value::Object(out))
}

impl Gateway {
    /// Publish (`Some`) or stop (`None`) `actor`'s timer on a voice channel. `label` is the status line that goes
    /// with it ("Focus 24:12"); the status rules (permission, being in the channel, 500 characters) apply.
    pub async fn voice_timer_set(
        &self,
        actor: UserId,
        channel: ChannelId,
        timer: Option<&Value>,
        label: &str,
    ) -> Result<(), StatusError> {
        let dir = &self.inner.dir;
        let guild = dir
            .channel_guild(channel)
            .await
            .ok_or(StatusError::UnknownChannel)?;
        let timer = match timer {
            Some(t) => {
                let clean = sanitize_timer(t).ok_or(StatusError::InvalidStatus)?;
                if label.trim().is_empty() {
                    return Err(StatusError::InvalidStatus);
                }
                self.voice_status_set(actor, channel, Some(label)).await?;
                Some(clean)
            }
            None => {
                match self.voice_channel_status(channel) {
                    // Our own status line goes with the timer.
                    Some(s) if s.set_by == actor => {
                        self.voice_status_set(actor, channel, None).await?;
                    }
                    // Someone else's: the permission check, without changing it.
                    Some(s) => {
                        self.voice_status_set(actor, channel, Some(&s.text)).await?;
                    }
                    // Nothing shared, nothing to stop, but only for someone who may: the status line's checks
                    // run (clearing an empty status changes nothing and dispatches nothing), so a stranger is
                    // refused instead of told 204 (found by the authorization matrix, wave-32 integration).
                    None => {
                        self.voice_status_set(actor, channel, None).await?;
                        return Ok(());
                    }
                }
                None
            }
        };
        let body = json!({
            "id": channel,
            "guild_id": guild,
            "user_id": actor,
            "server_ms": unix_ms(),
            "timer": timer,
        });
        self.dispatch_guild(guild, "VOICE_TIMER_UPDATE", body, Some(channel), None)
            .await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok() -> Value {
        json!({
            "kind": "pomodoro", "phase": "running", "segment": "focus",
            "startedMs": 1_700_000_000_000u64, "accumulatedMs": 0, "durationMs": 1_500_000, "completed": 2,
            "extra": "dropped"
        })
    }

    #[test]
    fn a_good_timer_is_rebuilt_without_unknown_fields() {
        let t = sanitize_timer(&ok()).unwrap();
        assert!(t.get("extra").is_none());
        assert_eq!(t["kind"], "pomodoro");
        assert_eq!(t["durationMs"], 1_500_000);
        assert_eq!(t["completed"], 2);
    }

    #[test]
    fn paused_timers_carry_no_start_time() {
        let mut v = ok();
        v["phase"] = "paused".into();
        let t = sanitize_timer(&v).unwrap();
        assert_eq!(t["startedMs"], Value::Null);
        // A running timer without one is refused.
        let mut v = ok();
        v.as_object_mut().unwrap().remove("startedMs");
        assert!(sanitize_timer(&v).is_none());
    }

    #[test]
    fn stopwatches_have_no_length_and_the_rest_must() {
        let sw = json!({"kind": "stopwatch", "phase": "running", "startedMs": 5, "accumulatedMs": 0, "durationMs": null});
        assert!(sanitize_timer(&sw).is_some());
        let mut with_len = sw.clone();
        with_len["durationMs"] = 1000.into();
        assert!(sanitize_timer(&with_len).is_none());
        let mut cd = ok();
        cd["kind"] = "countdown".into();
        cd["durationMs"] = Value::Null;
        assert!(sanitize_timer(&cd).is_none());
    }

    #[test]
    fn nonsense_is_refused() {
        for bad in [
            json!(null),
            json!("timer"),
            json!([]),
            json!({}),
            json!({"kind": "alarm", "phase": "running"}),
            json!({"kind": "pomodoro", "phase": "idle"}),
        ] {
            assert!(sanitize_timer(&bad).is_none(), "{bad}");
        }
        for (k, v) in [
            ("durationMs", json!(MAX_DURATION_MS + 1)),
            ("durationMs", json!(-5)),
            ("durationMs", json!("x")),
            ("accumulatedMs", json!(MAX_ACCUMULATED_MS + 1)),
            ("accumulatedMs", json!(1.5)),
            ("segment", json!("nap")),
            ("completed", json!(MAX_COMPLETED + 1)),
        ] {
            let mut t = ok();
            t[k] = v.clone();
            assert!(sanitize_timer(&t).is_none(), "{k}={v}");
        }
    }
}
