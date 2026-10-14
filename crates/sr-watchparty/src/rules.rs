//! Pure rules of the Watch Party backend: who may control, buffering aggregation, host handover, queue
//! ordering and input validation. No database, no clock.

use crate::error::{PartyError, PartyResult};
use crate::model::ControlMode;

/// A stall shorter than this is ignored (spec section 4).
pub const BUFFER_THRESHOLD_MS: i64 = 1_500;
pub const MAX_QUEUE: usize = 100;
pub const MAX_REACTION_BYTES: usize = 32;
pub const MAX_MESSAGE_CHARS: usize = 500;
pub const DEFAULT_MAX_PARTICIPANTS: i32 = 50;
/// Parties without a persistent flag end this long after the last member left (spec section 4).
pub const IDLE_END_MS: i64 = 5 * 60 * 1000;
/// A member who sent no heartbeat for this long is dropped by the sweep.
pub const MEMBER_STALE_MS: i64 = 90 * 1000;

/// Facts about one account relative to a party, gathered by the service.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Standing {
    pub member: bool,
    pub host: bool,
    /// Named a controller by the host.
    pub controller: bool,
    /// Holds `CONTROL_WATCH_PARTY` in the bound channel (always false for private parties).
    pub channel_control: bool,
    /// May manage the channel (`MANAGE_CHANNELS`) or the server: may end the party.
    pub channel_manage: bool,
}

/// Whether the standing may satisfy a [`ControlMode`].
pub fn allowed(mode: ControlMode, s: Standing) -> bool {
    if !s.member {
        return false;
    }
    if s.host {
        return true;
    }
    match mode {
        ControlMode::Host => false,
        ControlMode::Controllers => s.controller || s.channel_control,
        ControlMode::Everyone => true,
    }
}

/// May end the party, kick, hand over the host role.
pub fn can_manage(s: Standing) -> bool {
    s.host || s.channel_manage
}

/// One member's buffering report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferReport {
    pub user: i64,
    /// Since when the player has been stalled, `None` when it plays fine.
    pub since_ms: Option<i64>,
}

/// What the buffering rule asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BufferDecision {
    Nothing,
    /// Pause everybody; give up waiting at `wait_until_ms`.
    Pause {
        wait_until_ms: i64,
        waiting: Vec<i64>,
    },
    /// Everybody is back: resume.
    Resume,
    /// The wait ran out: resume without the listed (still stalled) members.
    ContinueWithout {
        users: Vec<i64>,
    },
}

/// The buffering rule (WP-BE-005). `auto_paused` is the party's current flag and `wait_until_ms` its
/// deadline.
pub fn decide_buffering(
    enabled: bool,
    playing: bool,
    auto_paused: bool,
    wait_until_ms: Option<i64>,
    reports: &[BufferReport],
    now_ms: i64,
    max_wait_ms: i64,
) -> BufferDecision {
    let stalled: Vec<i64> = reports
        .iter()
        .filter(|r| {
            r.since_ms
                .is_some_and(|s| now_ms - s >= BUFFER_THRESHOLD_MS)
        })
        .map(|r| r.user)
        .collect();
    let any_reporting = reports.iter().any(|r| r.since_ms.is_some());
    if auto_paused {
        if !any_reporting {
            return BufferDecision::Resume;
        }
        if wait_until_ms.is_some_and(|w| now_ms >= w) {
            let users: Vec<i64> = reports
                .iter()
                .filter(|r| r.since_ms.is_some())
                .map(|r| r.user)
                .collect();
            return BufferDecision::ContinueWithout { users };
        }
        return BufferDecision::Nothing;
    }
    if enabled && playing && !stalled.is_empty() {
        return BufferDecision::Pause {
            wait_until_ms: now_ms + max_wait_ms,
            waiting: stalled,
        };
    }
    BufferDecision::Nothing
}

/// Who becomes host when `leaving` goes: an explicit controller first, then the longest-standing member.
/// `members` is `(user, joined_at_ms, is_controller)`.
pub fn next_host(members: &[(i64, i64, bool)], leaving: i64) -> Option<i64> {
    members
        .iter()
        .filter(|(u, _, _)| *u != leaving)
        .min_by_key(|(u, joined, controller)| (!*controller, *joined, *u))
        .map(|(u, _, _)| *u)
}

/// Move `id` to `to` (clamped) in an ordered id list. `false` when `id` is not in it.
pub fn move_item(ids: &mut Vec<i64>, id: i64, to: usize) -> bool {
    let Some(from) = ids.iter().position(|x| *x == id) else {
        return false;
    };
    let item = ids.remove(from);
    let to = to.min(ids.len());
    ids.insert(to, item);
    true
}

/// A reaction is one short token (an emoji or a custom `name:id`), nothing that looks like markup.
pub fn validate_reaction(emoji: &str) -> PartyResult<String> {
    let e = emoji.trim();
    let bad = e.is_empty()
        || e.len() > MAX_REACTION_BYTES
        || e.chars().any(char::is_control)
        || e.contains(['<', '>', '&', '"', '\'']);
    if bad {
        return Err(PartyError::Invalid("Not a valid reaction.".to_owned()));
    }
    Ok(e.to_owned())
}

pub fn validate_message(content: &str) -> PartyResult<String> {
    let c = content.trim();
    let bad = c.is_empty()
        || c.chars().count() > MAX_MESSAGE_CHARS
        || c.chars().any(|ch| ch.is_control() && ch != '\n');
    if bad {
        return Err(PartyError::Invalid(
            "A message is 1 to 500 characters.".to_owned(),
        ));
    }
    Ok(c.to_owned())
}

/// An invite code from 16 bytes of entropy.
pub fn invite_code(entropy: [u8; 16]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghjkmnpqrstuvwxyz234567890";
    entropy
        .iter()
        .map(|b| char::from(ALPHABET[usize::from(*b) % ALPHABET.len()]))
        .collect()
}

pub fn validate_settings(
    max_wait_ms: Option<i32>,
    max_participants: Option<i32>,
    voice_limit: Option<u32>,
) -> PartyResult<()> {
    if let Some(w) = max_wait_ms
        && !(1_000..=60_000).contains(&w)
    {
        return Err(PartyError::Invalid(
            "max_wait_ms must be between 1000 and 60000.".to_owned(),
        ));
    }
    if let Some(n) = max_participants {
        if !(2..=100).contains(&n) {
            return Err(PartyError::Invalid(
                "max_participants must be between 2 and 100.".to_owned(),
            ));
        }
        if let Some(limit) = voice_limit
            && limit > 0
            && n > i32::try_from(limit).unwrap_or(i32::MAX)
        {
            return Err(PartyError::Invalid(
                "A voice-bound party cannot be larger than the voice channel limit.".to_owned(),
            ));
        }
    }
    Ok(())
}

/// A per-key sliding-window limiter (commands and reactions).
#[derive(Debug, Default)]
pub struct Window {
    hits: std::collections::VecDeque<i64>,
}

impl Window {
    /// Record a hit at `now_ms`; `false` when `max` hits already fell in the last `span_ms`.
    pub fn hit(&mut self, now_ms: i64, span_ms: i64, max: usize) -> bool {
        while self.hits.front().is_some_and(|t| now_ms - *t >= span_ms) {
            self.hits.pop_front();
        }
        if self.hits.len() >= max {
            return false;
        }
        self.hits.push_back(now_ms);
        true
    }

    pub fn is_idle(&self, now_ms: i64, span_ms: i64) -> bool {
        self.hits.back().is_none_or(|t| now_ms - *t >= span_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(member: bool, host: bool, controller: bool, channel_control: bool) -> Standing {
        Standing {
            member,
            host,
            controller,
            channel_control,
            channel_manage: false,
        }
    }

    #[test]
    fn host_always_controls_and_non_members_never_do() {
        for mode in [
            ControlMode::Host,
            ControlMode::Controllers,
            ControlMode::Everyone,
        ] {
            assert!(allowed(mode, s(true, true, false, false)));
            assert!(!allowed(mode, s(false, true, true, true)));
        }
    }

    #[test]
    fn shared_control_off_rejects_everybody_but_the_host() {
        assert!(!allowed(ControlMode::Host, s(true, false, true, true)));
    }

    #[test]
    fn controllers_mode_admits_named_controllers_and_the_channel_permission() {
        assert!(allowed(
            ControlMode::Controllers,
            s(true, false, true, false)
        ));
        assert!(allowed(
            ControlMode::Controllers,
            s(true, false, false, true)
        ));
        assert!(!allowed(
            ControlMode::Controllers,
            s(true, false, false, false)
        ));
    }

    #[test]
    fn everyone_mode_admits_members() {
        assert!(allowed(ControlMode::Everyone, s(true, false, false, false)));
    }

    #[test]
    fn channel_managers_may_manage_without_being_host() {
        let mut st = s(true, false, false, false);
        assert!(!can_manage(st));
        st.channel_manage = true;
        assert!(can_manage(st));
    }

    fn rep(user: i64, since: Option<i64>) -> BufferReport {
        BufferReport {
            user,
            since_ms: since,
        }
    }

    #[test]
    fn a_short_stall_does_not_pause() {
        let d = decide_buffering(
            true,
            true,
            false,
            None,
            &[rep(1, Some(9_000))],
            10_000,
            10_000,
        );
        assert_eq!(d, BufferDecision::Nothing);
    }

    #[test]
    fn a_long_stall_pauses_everybody_with_a_deadline() {
        let d = decide_buffering(
            true,
            true,
            false,
            None,
            &[rep(1, Some(8_000)), rep(2, None)],
            10_000,
            10_000,
        );
        assert_eq!(
            d,
            BufferDecision::Pause {
                wait_until_ms: 20_000,
                waiting: vec![1]
            }
        );
    }

    #[test]
    fn the_rule_can_be_switched_off_and_ignores_paused_parties() {
        let stalled = [rep(1, Some(0))];
        assert_eq!(
            decide_buffering(false, true, false, None, &stalled, 10_000, 10_000),
            BufferDecision::Nothing
        );
        assert_eq!(
            decide_buffering(true, false, false, None, &stalled, 10_000, 10_000),
            BufferDecision::Nothing
        );
    }

    #[test]
    fn resumes_when_everybody_recovered() {
        let d = decide_buffering(
            true,
            false,
            true,
            Some(20_000),
            &[rep(1, None), rep(2, None)],
            12_000,
            10_000,
        );
        assert_eq!(d, BufferDecision::Resume);
    }

    #[test]
    fn keeps_waiting_until_the_deadline_then_continues_without_the_stalled() {
        let reports = [rep(1, Some(0)), rep(2, None)];
        assert_eq!(
            decide_buffering(true, false, true, Some(20_000), &reports, 15_000, 10_000),
            BufferDecision::Nothing
        );
        assert_eq!(
            decide_buffering(true, false, true, Some(20_000), &reports, 20_000, 10_000),
            BufferDecision::ContinueWithout { users: vec![1] }
        );
    }

    #[test]
    fn handover_prefers_controllers_then_seniority() {
        let members = [
            (1, 100, false),
            (2, 300, true),
            (3, 200, false),
            (4, 150, true),
        ];
        assert_eq!(next_host(&members, 1), Some(4));
        let members = [(1, 100, false), (3, 200, false), (5, 150, false)];
        assert_eq!(next_host(&members, 1), Some(5));
        assert_eq!(next_host(&[(1, 1, true)], 1), None);
    }

    #[test]
    fn queue_moves_clamp_and_report_unknown_ids() {
        let mut ids = vec![1, 2, 3, 4];
        assert!(move_item(&mut ids, 4, 0));
        assert_eq!(ids, vec![4, 1, 2, 3]);
        assert!(move_item(&mut ids, 4, 99));
        assert_eq!(ids, vec![1, 2, 3, 4]);
        assert!(!move_item(&mut ids, 9, 0));
    }

    #[test]
    fn reactions_must_be_short_plain_tokens() {
        assert_eq!(validate_reaction(" 🍿 ").unwrap(), "🍿");
        assert!(validate_reaction("").is_err());
        assert!(validate_reaction("<script>").is_err());
        assert!(validate_reaction(&"x".repeat(33)).is_err());
        assert!(validate_reaction("a\nb").is_err());
    }

    #[test]
    fn messages_are_bounded() {
        assert!(validate_message("   ").is_err());
        assert!(validate_message(&"a".repeat(501)).is_err());
        assert_eq!(validate_message(" hi ").unwrap(), "hi");
    }

    #[test]
    fn settings_bounds() {
        assert!(validate_settings(Some(999), None, None).is_err());
        assert!(validate_settings(Some(10_000), Some(1), None).is_err());
        assert!(validate_settings(None, Some(30), Some(25)).is_err());
        assert!(validate_settings(None, Some(20), Some(25)).is_ok());
        assert!(validate_settings(None, Some(60), Some(0)).is_ok());
    }

    #[test]
    fn the_window_limits_bursts_and_recovers() {
        let mut w = Window::default();
        assert!(w.hit(0, 1000, 2));
        assert!(w.hit(10, 1000, 2));
        assert!(!w.hit(20, 1000, 2));
        assert!(w.hit(1100, 1000, 2));
        assert!(w.is_idle(5000, 1000));
    }

    #[test]
    fn invite_codes_use_the_alphabet() {
        let c = invite_code([7; 16]);
        assert_eq!(c.len(), 16);
        assert!(c.chars().all(|ch| ch.is_ascii_alphanumeric()));
    }
}
