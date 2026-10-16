//! Community, onboarding and welcome-screen flags (LNG-SSET-063..067), plus the
//! guild-level security flags (invite pause, raid alerts) from Safety Setup.

use serde::{Deserialize, Serialize};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, GuildId, RoleId};

use super::error::{SettingsError, SettingsResult};

pub const MIN_DEFAULT_CHANNELS: usize = 7;
pub const MIN_PUBLIC_DEFAULT_CHANNELS: usize = 5;
pub const MAX_PROMPTS: usize = 15;
pub const MAX_OPTIONS_PER_PROMPT: usize = 50;
pub const MAX_WELCOME_CHANNELS: usize = 5;
pub const MAX_WELCOME_DESCRIPTION: usize = 140;
pub const MAX_SCREENING_RULES: usize = 16;
pub const MAX_SCREENING_RULE_CHARS: usize = 300;
pub const MAX_SCREENING_DESCRIPTION: usize = 300;
pub const MAX_NEW_ACCOUNT_DAYS: u32 = 365;
/// The longest an incident pause may run ahead of now (D-243): 24 hours.
pub const MAX_INCIDENT_PAUSE_MS: i64 = 24 * 60 * 60 * 1000;
/// A join-rate alert does not fire again within this window (D-243).
pub const RAID_ALERT_COOLDOWN_MS: i64 = 10 * 60 * 1000;

/// Per-guild community and safety flags.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CommunitySettings {
    pub guild_id: GuildId,
    pub community_enabled: bool,
    pub rules_channel_id: Option<ChannelId>,
    pub updates_channel_id: Option<ChannelId>,
    pub safety_alerts_channel_id: Option<ChannelId>,
    pub description: Option<String>,
    pub preferred_locale: String,
    pub onboarding_enabled: bool,
    pub welcome_screen_enabled: bool,
    pub guide_enabled: bool,
    /// Security Actions: while paused, no invite can be used (spec section 5).
    pub invites_paused: bool,
    /// Raid protection: alert when joins per minute exceed this (0 = off).
    pub raid_alert_joins_per_minute: u32,
    /// Incident actions (D-243): invites cannot be used before this time (ms since the epoch).
    pub invites_disabled_until_ms: Option<i64>,
    /// Incident actions (D-243): while every shared server has this in the future, a DM between
    /// two members who are not friends is refused.
    pub dms_disabled_until_ms: Option<i64>,
    /// When the join-rate alert last fired.
    pub raid_detected_at_ms: Option<i64>,
    /// The join-rate alert pauses invites for 24 hours by itself (default on).
    pub raid_auto_pause_invites: bool,
    /// DM and spam protection (LNG-SSET-052): flag direct messages that members of this
    /// server send to each other when they look like spam.
    pub dm_spam_filter: bool,
    /// Blur explicit media in direct messages between members (LNG-SSET-054); the scanner
    /// is operator configuration, so this only records the server's choice.
    pub explicit_media_filter_dms: bool,
    /// Hide the preview of links that look suspicious.
    pub hide_suspicious_links: bool,
    /// Warn moderators when an account younger than this many days joins (0 = off, at most
    /// [`MAX_NEW_ACCOUNT_DAYS`]).
    pub warn_new_account_days: u32,
    pub age_restricted: bool,
    /// Membership screening (LNG-GLD-008): new members stay `pending` until they agree to
    /// the rules below. Only takes effect while Community is on.
    pub screening_enabled: bool,
    /// Text shown above the rules.
    pub screening_description: Option<String>,
    /// The rules a new member agrees to (at most [`MAX_SCREENING_RULES`]).
    pub screening_rules: Vec<String>,
}

/// Validate and trim the membership screening form: 1 to 16 rules of at most 300
/// characters when screening is on, a description of at most 300.
pub fn validate_screening(
    enabled: bool,
    description: Option<&str>,
    rules: &[String],
) -> SettingsResult<(Option<String>, Vec<String>)> {
    let bad = |field: &str, msg: &str| SettingsError::invalid(field, "SCREENING_INVALID", msg);
    let description = description
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(str::to_owned);
    if description
        .as_ref()
        .is_some_and(|d| d.chars().count() > MAX_SCREENING_DESCRIPTION)
    {
        return Err(bad("description", "must be 300 or fewer in length"));
    }
    let rules: Vec<String> = rules
        .iter()
        .map(|r| r.trim().to_owned())
        .filter(|r| !r.is_empty())
        .collect();
    if rules.len() > MAX_SCREENING_RULES {
        return Err(bad("form_fields", "at most 16 rules"));
    }
    if rules
        .iter()
        .any(|r| r.chars().count() > MAX_SCREENING_RULE_CHARS)
    {
        return Err(bad("form_fields", "rules are at most 300 characters"));
    }
    if enabled && rules.is_empty() {
        return Err(bad("form_fields", "add at least one rule"));
    }
    Ok((description, rules))
}

/// What a fired join-rate alert did (D-243).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaidAlert {
    pub detected_at_ms: i64,
    /// Invites are paused until this time; `None` when `raid_auto_pause_invites` is off.
    pub paused_until_ms: Option<i64>,
}

/// A timed incident pause (D-243) is `null` or in the future and at most 24 hours ahead.
pub fn validate_incident_time(
    field: &str,
    until_ms: Option<i64>,
    now_ms: i64,
) -> SettingsResult<Option<i64>> {
    let Some(until) = until_ms else {
        return Ok(None);
    };
    if until <= now_ms {
        return Err(SettingsError::invalid(
            field,
            "INCIDENT_TIME_PAST",
            "must be in the future",
        ));
    }
    if until > now_ms.saturating_add(MAX_INCIDENT_PAUSE_MS) {
        return Err(SettingsError::invalid(
            field,
            "INCIDENT_TIME_TOO_FAR",
            "must be at most 24 hours ahead",
        ));
    }
    Ok(Some(until))
}

impl CommunitySettings {
    /// Invites are paused at `now_ms`, indefinitely or by a timed incident action.
    pub fn invites_blocked(&self, now_ms: i64) -> bool {
        self.invites_paused || self.invites_disabled_until_ms.is_some_and(|t| t > now_ms)
    }

    pub fn new(guild_id: GuildId) -> Self {
        Self {
            guild_id,
            preferred_locale: "en-US".into(),
            raid_auto_pause_invites: true,
            ..Self::default()
        }
    }
}

/// What a guild looks like to the "Enable Community" safety checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafetyInputs {
    pub verification_level: u8,
    /// 0 off, 1 members without roles, 2 all members.
    pub explicit_content_filter: u8,
    pub has_rules_channel: bool,
    pub has_updates_channel: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommunityBlocker {
    VerificationTooLow,
    ContentFilterTooLow,
    MissingRulesChannel,
    MissingUpdatesChannel,
}

/// Failed checks for enabling Community; empty means ready.
pub fn community_blockers(i: &SafetyInputs) -> Vec<CommunityBlocker> {
    let mut v = Vec::new();
    if i.verification_level < 1 {
        v.push(CommunityBlocker::VerificationTooLow);
    }
    if i.explicit_content_filter < 2 {
        v.push(CommunityBlocker::ContentFilterTooLow);
    }
    if !i.has_rules_channel {
        v.push(CommunityBlocker::MissingRulesChannel);
    }
    if !i.has_updates_channel {
        v.push(CommunityBlocker::MissingUpdatesChannel);
    }
    v
}

/// Apply the wizard result, refusing when a safety check fails.
pub fn enable_community(
    s: &mut CommunitySettings,
    inputs: &SafetyInputs,
    rules: ChannelId,
    updates: ChannelId,
) -> SettingsResult<()> {
    let mut checked = *inputs;
    checked.has_rules_channel = true;
    checked.has_updates_channel = true;
    if !community_blockers(&checked).is_empty() {
        return Err(SettingsError::invalid(
            "features",
            "COMMUNITY_REQUIREMENTS",
            "verification and explicit content filter requirements are not met",
        ));
    }
    s.community_enabled = true;
    s.rules_channel_id = Some(rules);
    s.updates_channel_id = Some(updates);
    Ok(())
}

/// Disabling Community also turns off the features that depend on it.
pub fn disable_community(s: &mut CommunitySettings) {
    s.community_enabled = false;
    s.onboarding_enabled = false;
    s.welcome_screen_enabled = false;
    s.guide_enabled = false;
    s.screening_enabled = false;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnboardingOption {
    pub id: u64,
    pub title: String,
    pub description: Option<String>,
    pub emoji: Option<String>,
    /// Channels shown to members who pick this option.
    pub channel_ids: Vec<ChannelId>,
    /// Roles granted when picked.
    pub role_ids: Vec<RoleId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnboardingPrompt {
    pub id: u64,
    pub title: String,
    pub single_select: bool,
    pub required: bool,
    /// Shown in the initial questions (true) or only in channels-and-roles (false).
    pub in_onboarding: bool,
    pub options: Vec<OnboardingOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Onboarding {
    pub guild_id: GuildId,
    pub enabled: bool,
    pub default_channel_ids: Vec<ChannelId>,
    pub prompts: Vec<OnboardingPrompt>,
}

/// Validate and normalise an onboarding config. `writable` says whether the
/// default (@everyone) role may send messages in that channel.
pub fn validate_onboarding(
    o: &Onboarding,
    writable: impl Fn(ChannelId) -> bool,
) -> SettingsResult<()> {
    let bad = |field: &str, msg: &str| SettingsError::invalid(field, "ONBOARDING_INVALID", msg);
    if o.prompts.len() > MAX_PROMPTS {
        return Err(bad("prompts", "at most 15 questions"));
    }
    for p in &o.prompts {
        let t = p.title.trim().chars().count();
        if !(1..=100).contains(&t) {
            return Err(bad("prompts", "question titles are 1-100 characters"));
        }
        if p.options.is_empty() || p.options.len() > MAX_OPTIONS_PER_PROMPT {
            return Err(bad("options", "each question needs 1-50 answers"));
        }
        for opt in &p.options {
            if !(1..=50).contains(&opt.title.trim().chars().count()) {
                return Err(bad("options", "answer titles are 1-50 characters"));
            }
        }
    }
    if o.enabled {
        let mut seen = std::collections::HashSet::new();
        let unique: Vec<_> = o
            .default_channel_ids
            .iter()
            .filter(|c| seen.insert(**c))
            .collect();
        if unique.len() < MIN_DEFAULT_CHANNELS {
            return Err(bad(
                "default_channel_ids",
                "at least 7 default channels are required",
            ));
        }
        if unique.iter().filter(|c| writable(***c)).count() < MIN_PUBLIC_DEFAULT_CHANNELS {
            return Err(bad(
                "default_channel_ids",
                "at least 5 default channels must allow @everyone to send messages",
            ));
        }
    }
    Ok(())
}

/// Channels and roles a member receives for the options they picked.
pub fn resolve_answers(o: &Onboarding, picked: &[u64]) -> (Vec<ChannelId>, Vec<RoleId>) {
    let mut channels: Vec<ChannelId> = o.default_channel_ids.clone();
    let mut roles = Vec::new();
    for opt in o.prompts.iter().flat_map(|p| &p.options) {
        if picked.contains(&opt.id) {
            for c in &opt.channel_ids {
                if !channels.contains(c) {
                    channels.push(*c);
                }
            }
            for r in &opt.role_ids {
                if !roles.contains(r) {
                    roles.push(*r);
                }
            }
        }
    }
    (channels, roles)
}

/// Check a member's answers against the form: every id must be an option of the form,
/// a single-select question takes at most one, and required in-onboarding questions
/// need an answer.
pub fn validate_answers(o: &Onboarding, picked: &[u64]) -> SettingsResult<()> {
    let bad = |msg: &str| SettingsError::invalid("option_ids", "ONBOARDING_ANSWER_INVALID", msg);
    let known: Vec<u64> = o
        .prompts
        .iter()
        .flat_map(|p| p.options.iter().map(|opt| opt.id))
        .collect();
    if picked.iter().any(|id| !known.contains(id)) {
        return Err(bad("unknown answer"));
    }
    for p in &o.prompts {
        let n = p
            .options
            .iter()
            .filter(|opt| picked.contains(&opt.id))
            .count();
        if p.single_select && n > 1 {
            return Err(bad("this question takes one answer"));
        }
    }
    if !missing_required(o, picked).is_empty() {
        return Err(bad("answer the required questions"));
    }
    Ok(())
}

/// Permission bits an onboarding answer may never grant through a role (Discord applies the
/// same rule when onboarding is configured).
pub const ONBOARDING_FORBIDDEN: Permissions = Permissions::ADMINISTRATOR
    .union(Permissions::MANAGE_GUILD)
    .union(Permissions::MANAGE_ROLES)
    .union(Permissions::MANAGE_CHANNELS)
    .union(Permissions::KICK_MEMBERS)
    .union(Permissions::BAN_MEMBERS)
    .union(Permissions::MANAGE_MESSAGES)
    .union(Permissions::MANAGE_WEBHOOKS)
    .union(Permissions::MANAGE_THREADS)
    .union(Permissions::MANAGE_NICKNAMES)
    .union(Permissions::MODERATE_MEMBERS)
    .union(Permissions::MENTION_EVERYONE)
    .union(Permissions::MANAGE_GUILD_EXPRESSIONS)
    .union(Permissions::MANAGE_EVENTS)
    .union(Permissions::MANAGE_EXTENSIONS);

/// Whether a role may be granted by an onboarding answer: not integration-managed and none
/// of [`ONBOARDING_FORBIDDEN`].
pub fn onboarding_role_is_safe(perms: Permissions, managed: bool) -> bool {
    !managed && !perms.intersects(ONBOARDING_FORBIDDEN)
}

/// Whether the member must answer before joining (any required in-onboarding prompt
/// left unanswered).
pub fn missing_required(o: &Onboarding, picked: &[u64]) -> Vec<u64> {
    o.prompts
        .iter()
        .filter(|p| p.required && p.in_onboarding)
        .filter(|p| !p.options.iter().any(|opt| picked.contains(&opt.id)))
        .map(|p| p.id)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WelcomeChannel {
    pub channel_id: ChannelId,
    pub description: String,
    pub emoji: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WelcomeScreen {
    pub guild_id: GuildId,
    pub enabled: bool,
    pub description: Option<String>,
    pub channels: Vec<WelcomeChannel>,
}

pub fn validate_welcome_screen(w: &WelcomeScreen) -> SettingsResult<()> {
    let bad = |field: &str, msg: &str| SettingsError::invalid(field, "WELCOME_SCREEN_INVALID", msg);
    if w.channels.len() > MAX_WELCOME_CHANNELS {
        return Err(bad("welcome_channels", "at most 5 channel cards"));
    }
    if w.description
        .as_ref()
        .is_some_and(|d| d.chars().count() > MAX_WELCOME_DESCRIPTION)
    {
        return Err(bad("description", "must be 140 or fewer in length"));
    }
    for c in &w.channels {
        if !(1..=50).contains(&c.description.trim().chars().count()) {
            return Err(bad(
                "welcome_channels",
                "card descriptions are 1-50 characters",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(n: u64) -> ChannelId {
        ChannelId::new(n)
    }

    fn option(id: u64, channels: &[u64], roles: &[u64]) -> OnboardingOption {
        OnboardingOption {
            id,
            title: format!("opt {id}"),
            description: None,
            emoji: None,
            channel_ids: channels.iter().map(|c| ch(*c)).collect(),
            role_ids: roles.iter().map(|r| RoleId::new(*r)).collect(),
        }
    }

    fn onboarding(enabled: bool, defaults: u64) -> Onboarding {
        Onboarding {
            guild_id: GuildId::new(1),
            enabled,
            default_channel_ids: (1..=defaults).map(ch).collect(),
            prompts: vec![OnboardingPrompt {
                id: 1,
                title: "What brings you here?".into(),
                single_select: false,
                required: true,
                in_onboarding: true,
                options: vec![option(10, &[100], &[7]), option(11, &[1, 101], &[])],
            }],
        }
    }

    #[test]
    fn community_checks() {
        let weak = SafetyInputs {
            verification_level: 0,
            explicit_content_filter: 0,
            has_rules_channel: false,
            has_updates_channel: true,
        };
        assert_eq!(community_blockers(&weak).len(), 3);
        let mut s = CommunitySettings::new(GuildId::new(1));
        assert!(enable_community(&mut s, &weak, ch(1), ch(2)).is_err());
        let ok = SafetyInputs {
            verification_level: 1,
            explicit_content_filter: 2,
            has_rules_channel: false,
            has_updates_channel: false,
        };
        enable_community(&mut s, &ok, ch(1), ch(2)).unwrap();
        assert!(s.community_enabled);
        s.onboarding_enabled = true;
        s.invites_paused = true;
        disable_community(&mut s);
        assert!(!s.community_enabled && !s.onboarding_enabled);
        assert!(
            s.invites_paused,
            "pausing invites is independent of community"
        );
    }

    #[test]
    fn onboarding_requirements() {
        assert!(validate_onboarding(&onboarding(false, 0), |_| true).is_ok());
        assert!(validate_onboarding(&onboarding(true, 6), |_| true).is_err());
        assert!(validate_onboarding(&onboarding(true, 7), |_| true).is_ok());
        assert!(validate_onboarding(&onboarding(true, 7), |c| c.get() <= 4).is_err());
        let mut o = onboarding(false, 0);
        o.prompts[0].options.clear();
        assert!(validate_onboarding(&o, |_| true).is_err());
        let mut o = onboarding(false, 0);
        o.prompts = (0..16).map(|_| o.prompts[0].clone()).collect();
        assert!(validate_onboarding(&o, |_| true).is_err());
    }

    #[test]
    fn answers_map_to_channels_and_roles() {
        let o = onboarding(true, 3);
        let (channels, roles) = resolve_answers(&o, &[10, 11]);
        assert_eq!(channels, vec![ch(1), ch(2), ch(3), ch(100), ch(101)]);
        assert_eq!(roles, vec![RoleId::new(7)]);
        assert_eq!(missing_required(&o, &[]), vec![1]);
        assert!(missing_required(&o, &[11]).is_empty());
    }

    #[test]
    fn answers_are_validated() {
        let mut o = onboarding(true, 3);
        assert!(validate_answers(&o, &[10]).is_ok());
        assert!(
            validate_answers(&o, &[10, 11]).is_ok(),
            "multi select by default"
        );
        assert!(validate_answers(&o, &[]).is_err(), "required question");
        assert!(validate_answers(&o, &[999]).is_err(), "unknown option");
        o.prompts[0].single_select = true;
        assert!(validate_answers(&o, &[10, 11]).is_err(), "single select");
        assert!(validate_answers(&o, &[11]).is_ok());
    }

    #[test]
    fn onboarding_roles_cannot_carry_power() {
        assert!(onboarding_role_is_safe(Permissions::empty(), false));
        assert!(onboarding_role_is_safe(Permissions::SEND_MESSAGES, false));
        assert!(
            !onboarding_role_is_safe(Permissions::empty(), true),
            "managed"
        );
        for p in [
            Permissions::ADMINISTRATOR,
            Permissions::MANAGE_ROLES,
            Permissions::KICK_MEMBERS,
            Permissions::MENTION_EVERYONE,
            Permissions::SEND_MESSAGES | Permissions::BAN_MEMBERS,
        ] {
            assert!(!onboarding_role_is_safe(p, false), "{p:?}");
        }
    }

    #[test]
    fn screening_form_is_validated() {
        let rules = vec!["  Be kind ".to_owned(), String::new(), "No spam".to_owned()];
        let (d, r) = validate_screening(true, Some("  hi "), &rules).unwrap();
        assert_eq!(
            (d.as_deref(), r.as_slice()),
            (
                Some("hi"),
                &["Be kind".to_owned(), "No spam".to_owned()][..]
            )
        );
        assert!(
            validate_screening(true, None, &[]).is_err(),
            "on needs a rule"
        );
        assert!(validate_screening(false, None, &[]).is_ok());
        assert!(validate_screening(false, None, &vec!["x".to_owned(); 17]).is_err());
        assert!(validate_screening(false, None, &["y".repeat(301)]).is_err());
        assert!(validate_screening(false, Some(&"z".repeat(301)), &[]).is_err());
    }

    #[test]
    fn welcome_screen_limits() {
        let card = |n| WelcomeChannel {
            channel_id: ch(n),
            description: "start here".into(),
            emoji: None,
        };
        let mut w = WelcomeScreen {
            guild_id: GuildId::new(1),
            enabled: true,
            description: Some("hi".into()),
            channels: (1..=5).map(card).collect(),
        };
        assert!(validate_welcome_screen(&w).is_ok());
        w.channels.push(card(6));
        assert!(validate_welcome_screen(&w).is_err());
        w.channels.truncate(1);
        w.description = Some("x".repeat(141));
        assert!(validate_welcome_screen(&w).is_err());
    }
}

#[cfg(test)]
mod incident_tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;
    const HOUR: i64 = 3_600_000;

    #[test]
    fn incident_times_are_bounded() {
        assert_eq!(validate_incident_time("f", None, NOW).unwrap(), None);
        assert_eq!(
            validate_incident_time("f", Some(NOW + HOUR), NOW).unwrap(),
            Some(NOW + HOUR)
        );
        assert!(validate_incident_time("f", Some(NOW + 24 * HOUR), NOW).is_ok());
        assert!(validate_incident_time("f", Some(NOW + 24 * HOUR + 1), NOW).is_err());
        assert!(validate_incident_time("f", Some(NOW), NOW).is_err());
        assert!(validate_incident_time("f", Some(NOW - 1), NOW).is_err());
    }

    #[test]
    fn timed_pause_blocks_invites_until_it_ends() {
        let mut s = CommunitySettings::new(GuildId::new(1));
        assert!(s.raid_auto_pause_invites, "auto pause is on by default");
        assert!(!s.invites_blocked(NOW));
        s.invites_disabled_until_ms = Some(NOW + HOUR);
        assert!(s.invites_blocked(NOW));
        assert!(!s.invites_blocked(NOW + HOUR));
        s.invites_disabled_until_ms = None;
        s.invites_paused = true;
        assert!(
            s.invites_blocked(NOW),
            "the indefinite switch stays its own"
        );
    }
}
