//! AutoMod rule model and engine (LNG-SSET-059).
//!
//! Rules are validated with [`AutoModRule::validate`], compiled once into a
//! [`CompiledRule`] (regexes, keyword matchers) and evaluated against a message or
//! a member profile. Evaluation is pure; executing the returned actions (delete the
//! message, post the alert, time the member out, audit entry, gateway events) is the
//! caller job.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use moka::sync::Cache;
use regex::{Regex, RegexBuilder, RegexSet, RegexSetBuilder};
use serde::{Deserialize, Serialize};
use sr_core::{ChannelId, GuildId, RoleId, UserId};
use unicode_normalization::UnicodeNormalization;

use super::error::{SettingsError, SettingsResult};

pub const MAX_KEYWORDS: usize = 1000;
pub const MAX_KEYWORD_CHARS: usize = 60;
pub const MAX_REGEX_PATTERNS: usize = 10;
pub const MAX_REGEX_CHARS: usize = 260;
pub const MAX_ALLOW_LIST: usize = 100;
pub const MAX_EXEMPT_ROLES: usize = 20;
pub const MAX_EXEMPT_CHANNELS: usize = 50;
pub const MAX_TIMEOUT_SECS: u32 = 2_419_200;
pub const MAX_CUSTOM_MESSAGE_CHARS: usize = 150;
pub const MAX_MENTION_LIMIT: u32 = 50;
/// Compiled-regex size cap, so a hostile pattern cannot exhaust memory.
const REGEX_SIZE_LIMIT: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    MessageSend,
    /// Member profile names (username, display name, nickname).
    MemberUpdate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeywordPreset {
    Profanity,
    SexualContent,
    Slurs,
}

/// What a rule looks for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trigger {
    /// Custom words (with `*` wildcards) and regexes, minus an allow list.
    Keyword {
        keywords: Vec<String>,
        regex_patterns: Vec<String>,
        allow_list: Vec<String>,
    },
    /// Stateless spam heuristics (character floods, repeated words, link floods).
    Spam,
    /// Operator-provided preset word lists.
    KeywordPreset {
        presets: Vec<KeywordPreset>,
        allow_list: Vec<String>,
    },
    /// More distinct user/role mentions than `total_limit` in one message.
    MentionSpam {
        total_limit: u32,
        raid_protection: bool,
    },
    /// Links: block every link except `allow_domains`, or only `block_domains`.
    Links {
        block_all: bool,
        block_domains: Vec<String>,
        allow_domains: Vec<String>,
    },
}

impl Trigger {
    pub fn kind(&self) -> TriggerKind {
        match self {
            Self::Keyword { .. } => TriggerKind::Keyword,
            Self::Spam => TriggerKind::Spam,
            Self::KeywordPreset { .. } => TriggerKind::KeywordPreset,
            Self::MentionSpam { .. } => TriggerKind::MentionSpam,
            Self::Links { .. } => TriggerKind::Links,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TriggerKind {
    Keyword,
    Spam,
    KeywordPreset,
    MentionSpam,
    Links,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// Delete the message and optionally explain why to the author.
    Block { custom_message: Option<String> },
    /// Post an alert into a channel (LNG-SSET-062).
    SendAlert { channel_id: ChannelId },
    /// Time the author out.
    Timeout { duration_secs: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoModRule {
    pub id: u64,
    pub guild_id: GuildId,
    pub name: String,
    pub creator_id: UserId,
    pub event_type: EventType,
    pub trigger: Trigger,
    pub actions: Vec<Action>,
    pub enabled: bool,
    pub exempt_role_ids: Vec<RoleId>,
    pub exempt_channel_ids: Vec<ChannelId>,
}

/// How many rules of each kind a guild may have (Discord parity).
pub fn max_rules(kind: TriggerKind, event: EventType) -> usize {
    match (kind, event) {
        (TriggerKind::Keyword, EventType::MessageSend) => 6,
        (TriggerKind::Keyword, EventType::MemberUpdate) => 1,
        (TriggerKind::Links, _) => 3,
        _ => 1,
    }
}

fn bad(field: &str, msg: impl Into<String>) -> SettingsError {
    SettingsError::invalid(field, "AUTO_MODERATION_INVALID", msg)
}

impl AutoModRule {
    /// Structural validation (limits, trigger/action combinations, regex syntax).
    pub fn validate(&self) -> SettingsResult<()> {
        let n = self.name.trim().chars().count();
        if !(1..=100).contains(&n) {
            return Err(bad("name", "must be between 1 and 100 in length"));
        }
        if self.exempt_role_ids.len() > MAX_EXEMPT_ROLES {
            return Err(bad("exempt_roles", "too many exempt roles"));
        }
        if self.exempt_channel_ids.len() > MAX_EXEMPT_CHANNELS {
            return Err(bad("exempt_channels", "too many exempt channels"));
        }
        let allowed_event = match (&self.trigger, self.event_type) {
            (Trigger::Keyword { .. }, _) => true,
            (_, EventType::MessageSend) => true,
            (_, EventType::MemberUpdate) => false,
        };
        if !allowed_event {
            return Err(bad(
                "trigger_type",
                "this trigger only applies to sent messages",
            ));
        }
        match &self.trigger {
            Trigger::Keyword {
                keywords,
                regex_patterns,
                allow_list,
            } => {
                if keywords.is_empty() && regex_patterns.is_empty() {
                    return Err(bad("trigger_metadata", "add at least one keyword or regex"));
                }
                check_words("keyword_filter", keywords, MAX_KEYWORDS)?;
                check_words("allow_list", allow_list, MAX_ALLOW_LIST)?;
                if regex_patterns.len() > MAX_REGEX_PATTERNS {
                    return Err(bad("regex_patterns", "too many regex patterns"));
                }
                for p in regex_patterns {
                    if p.is_empty() || p.chars().count() > MAX_REGEX_CHARS {
                        return Err(bad("regex_patterns", "pattern length must be 1-260"));
                    }
                    build_regex(p).map_err(|e| bad("regex_patterns", e))?;
                }
            }
            Trigger::KeywordPreset {
                presets,
                allow_list,
            } => {
                if presets.is_empty() {
                    return Err(bad("presets", "choose at least one preset"));
                }
                check_words("allow_list", allow_list, MAX_ALLOW_LIST)?;
            }
            Trigger::MentionSpam { total_limit, .. } => {
                if !(1..=MAX_MENTION_LIMIT).contains(total_limit) {
                    return Err(bad("mention_total_limit", "must be between 1 and 50"));
                }
            }
            Trigger::Links {
                block_all,
                block_domains,
                allow_domains,
            } => {
                if !block_all && block_domains.is_empty() {
                    return Err(bad("trigger_metadata", "block all links or list domains"));
                }
                check_words("block_domains", block_domains, MAX_KEYWORDS)?;
                check_words("allow_domains", allow_domains, MAX_ALLOW_LIST)?;
            }
            Trigger::Spam => {}
        }
        if self.actions.is_empty() {
            return Err(bad("actions", "at least one action is required"));
        }
        for a in &self.actions {
            match a {
                Action::Block { custom_message } => {
                    if self.event_type != EventType::MessageSend {
                        return Err(bad("actions", "blocking applies to sent messages"));
                    }
                    if custom_message
                        .as_ref()
                        .is_some_and(|m| m.chars().count() > MAX_CUSTOM_MESSAGE_CHARS)
                    {
                        return Err(bad("actions", "custom message is too long"));
                    }
                }
                Action::SendAlert { .. } => {}
                Action::Timeout { duration_secs } => {
                    if self.event_type != EventType::MessageSend
                        || !matches!(
                            self.trigger,
                            Trigger::Keyword { .. } | Trigger::MentionSpam { .. }
                        )
                    {
                        return Err(bad(
                            "actions",
                            "timeout is only available for keyword and mention spam rules",
                        ));
                    }
                    if *duration_secs == 0 || *duration_secs > MAX_TIMEOUT_SECS {
                        return Err(bad("actions", "timeout must be 1 second to 28 days"));
                    }
                }
            }
        }
        Ok(())
    }
}

fn check_words(field: &str, words: &[String], max: usize) -> SettingsResult<()> {
    if words.len() > max {
        return Err(bad(field, format!("at most {max} entries")));
    }
    for w in words {
        let n = w.chars().count();
        if n == 0 || n > MAX_KEYWORD_CHARS {
            return Err(bad(field, "entries must be 1-60 characters"));
        }
    }
    Ok(())
}

/// Reject creating more rules of a kind than the guild may have.
pub fn check_rule_quota(existing: &[AutoModRule], new: &AutoModRule) -> SettingsResult<()> {
    let kind = new.trigger.kind();
    let count = existing
        .iter()
        .filter(|r| r.id != new.id && r.trigger.kind() == kind && r.event_type == new.event_type)
        .count();
    if count >= max_rules(kind, new.event_type) {
        Err(SettingsError::LimitReached("AutoMod rules of this type"))
    } else {
        Ok(())
    }
}

fn build_regex(p: &str) -> Result<Regex, String> {
    RegexBuilder::new(p)
        .case_insensitive(true)
        .size_limit(REGEX_SIZE_LIMIT)
        .dfa_size_limit(REGEX_SIZE_LIMIT)
        .build()
        .map_err(|e| format!("invalid regex: {e}"))
}

/// Translate an AutoMod keyword (with `*` wildcards) into a regex source.
/// `cat` matches the whole word only, `cat*` a prefix, `*cat` a suffix, `*cat*`
/// any substring. Matching is case-insensitive.
fn keyword_pattern(keyword: &str) -> String {
    let k = keyword.trim();
    let starts = k.starts_with('*');
    let ends = k.ends_with('*') && k.len() > 1;
    let core = k.trim_matches('*');
    let mut p = String::new();
    if !starts {
        p.push_str(r"(?:^|[^\p{L}\p{N}_])");
    }
    p.push_str(&regex::escape(core));
    if !ends {
        p.push_str(r"(?:$|[^\p{L}\p{N}_])");
    }
    p
}

fn word_set(words: &[String]) -> Result<Option<RegexSet>, String> {
    if words.is_empty() {
        return Ok(None);
    }
    // Keywords are folded like the text they are matched against (open question 136).
    RegexSetBuilder::new(words.iter().map(|w| keyword_pattern(&fold(w))))
        .case_insensitive(true)
        .size_limit(REGEX_SIZE_LIMIT * 8)
        .build()
        .map(Some)
        .map_err(|e| format!("invalid keyword list: {e}"))
}

/// Operator-supplied preset word lists (the platform ships none; see open questions).
pub type PresetLists = HashMap<KeywordPreset, Vec<String>>;

enum Matcher {
    Keywords {
        words: Vec<String>,
        set: Option<RegexSet>,
        regexes: Vec<(String, Regex)>,
        allow: Option<RegexSet>,
    },
    Spam,
    Mentions {
        limit: u32,
    },
    Links {
        block_all: bool,
        block: Vec<String>,
        allow: Vec<String>,
    },
}

pub struct CompiledRule {
    pub rule: AutoModRule,
    matcher: Matcher,
}

impl CompiledRule {
    /// Validate and compile `rule`. Preset lists are only consulted for preset triggers.
    pub fn compile(rule: AutoModRule, presets: &PresetLists) -> SettingsResult<Self> {
        rule.validate()?;
        let matcher = match &rule.trigger {
            Trigger::Keyword {
                keywords,
                regex_patterns,
                allow_list,
            } => keyword_matcher(keywords, regex_patterns, allow_list)?,
            Trigger::KeywordPreset {
                presets: chosen,
                allow_list,
            } => {
                let mut words: Vec<String> = chosen
                    .iter()
                    .filter_map(|p| presets.get(p))
                    .flatten()
                    .cloned()
                    .collect();
                words.sort();
                words.dedup();
                keyword_matcher(&words, &[], allow_list)?
            }
            Trigger::Spam => Matcher::Spam,
            Trigger::MentionSpam { total_limit, .. } => Matcher::Mentions {
                limit: *total_limit,
            },
            Trigger::Links {
                block_all,
                block_domains,
                allow_domains,
            } => Matcher::Links {
                block_all: *block_all,
                block: block_domains.iter().map(|d| normalize_domain(d)).collect(),
                allow: allow_domains.iter().map(|d| normalize_domain(d)).collect(),
            },
        };
        Ok(Self { rule, matcher })
    }
}

fn keyword_matcher(
    keywords: &[String],
    regex_patterns: &[String],
    allow_list: &[String],
) -> SettingsResult<Matcher> {
    let set = word_set(keywords).map_err(|e| bad("keyword_filter", e))?;
    let allow = word_set(allow_list).map_err(|e| bad("allow_list", e))?;
    let mut regexes = Vec::new();
    for p in regex_patterns {
        regexes.push((
            p.clone(),
            build_regex(p).map_err(|e| bad("regex_patterns", e))?,
        ));
    }
    Ok(Matcher::Keywords {
        words: keywords.to_vec(),
        set,
        regexes,
        allow,
    })
}

fn normalize_domain(d: &str) -> String {
    d.trim()
        .trim_start_matches("*.")
        .trim_start_matches('.')
        .to_lowercase()
}

/// A message (or other event) as AutoMod sees it.
#[derive(Debug, Clone, Default)]
pub struct MessageContext {
    pub author_id: UserId,
    pub channel_id: ChannelId,
    /// Parent category or thread parent, for channel exemptions.
    pub parent_channel_id: Option<ChannelId>,
    pub author_role_ids: Vec<RoleId>,
    /// Members with ADMINISTRATOR or MANAGE_GUILD are never filtered (Discord).
    pub bypass: bool,
    pub content: String,
    pub mentioned_user_ids: Vec<UserId>,
    pub mentioned_role_ids: Vec<RoleId>,
}

/// Names AutoMod checks on member-profile rules.
#[derive(Debug, Clone, Default)]
pub struct ProfileContext {
    pub user_id: UserId,
    pub role_ids: Vec<RoleId>,
    pub bypass: bool,
    pub texts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleMatch {
    pub rule_id: u64,
    pub rule_name: String,
    pub trigger: TriggerKind,
    /// The keyword, regex, domain or reason that matched.
    pub matched: String,
    pub actions: Vec<Action>,
}

fn exempt(rule: &AutoModRule, roles: &[RoleId], channels: &[ChannelId], bypass: bool) -> bool {
    bypass
        || !rule.enabled
        || rule.exempt_role_ids.iter().any(|r| roles.contains(r))
        || rule.exempt_channel_ids.iter().any(|c| channels.contains(c))
}

/// Evaluate every compiled message rule; all matching rules are returned.
pub fn evaluate_message(rules: &[CompiledRule], ctx: &MessageContext) -> Vec<RuleMatch> {
    evaluate_message_with(rules, ctx, None)
}

/// [`evaluate_message`] plus the stateful verdict of a [`SpamWindow`]: when `burst`
/// carries a reason, every enabled, non-exempt spam rule matches with it.
pub fn evaluate_message_with(
    rules: &[CompiledRule],
    ctx: &MessageContext,
    burst: Option<&str>,
) -> Vec<RuleMatch> {
    let channels: Vec<ChannelId> = std::iter::once(ctx.channel_id)
        .chain(ctx.parent_channel_id)
        .collect();
    let text = Text::new(&ctx.content);
    rules
        .iter()
        .filter(|c| c.rule.event_type == EventType::MessageSend)
        .filter(|c| !exempt(&c.rule, &ctx.author_role_ids, &channels, ctx.bypass))
        .filter_map(|c| {
            let matched = match_text(&c.matcher, &text, Some(ctx), burst)?;
            Some(RuleMatch {
                rule_id: c.rule.id,
                rule_name: c.rule.name.clone(),
                trigger: c.rule.trigger.kind(),
                matched,
                actions: c.rule.actions.clone(),
            })
        })
        .collect()
}

/// Evaluate member-profile rules against names and bio text.
pub fn evaluate_profile(rules: &[CompiledRule], ctx: &ProfileContext) -> Vec<RuleMatch> {
    rules
        .iter()
        .filter(|c| c.rule.event_type == EventType::MemberUpdate)
        .filter(|c| !exempt(&c.rule, &ctx.role_ids, &[], ctx.bypass))
        .filter_map(|c| {
            let matched = ctx
                .texts
                .iter()
                .find_map(|t| match_text(&c.matcher, &Text::new(t), None, None))?;
            Some(RuleMatch {
                rule_id: c.rule.id,
                rule_name: c.rule.name.clone(),
                trigger: c.rule.trigger.kind(),
                matched,
                actions: c.rule.actions.clone(),
            })
        })
        .collect()
}

/// Text under evaluation: the original and its folded form (computed once per
/// message, not once per rule).
struct Text<'a> {
    raw: &'a str,
    folded: String,
}

impl<'a> Text<'a> {
    fn new(raw: &'a str) -> Self {
        Self {
            raw,
            folded: fold(raw),
        }
    }
}

fn burst_reason(burst: Option<&str>) -> Option<&'static str> {
    match burst? {
        BURST_DUPLICATE => Some(BURST_DUPLICATE),
        BURST_RATE => Some(BURST_RATE),
        _ => None,
    }
}

fn match_text(
    m: &Matcher,
    text: &Text<'_>,
    ctx: Option<&MessageContext>,
    burst: Option<&str>,
) -> Option<String> {
    match m {
        Matcher::Keywords {
            words,
            set,
            regexes,
            allow,
        } => {
            let folded = strip_allowed(&text.folded, allow.as_ref());
            if let Some(set) = set
                && let Some(i) = set.matches(&folded).iter().next()
            {
                return Some(words[i].clone());
            }
            // Regexes are the admin's own: try them on the text as written and folded.
            let raw = strip_allowed(text.raw, allow.as_ref());
            regexes
                .iter()
                .find(|(_, r)| r.is_match(&raw) || r.is_match(&folded))
                .map(|(p, _)| p.clone())
        }
        Matcher::Spam => spam_reason(text.raw)
            .or(burst_reason(burst))
            .map(str::to_owned),
        Matcher::Mentions { limit } => {
            let ctx = ctx?;
            let users: HashSet<_> = ctx.mentioned_user_ids.iter().collect();
            let roles: HashSet<_> = ctx.mentioned_role_ids.iter().collect();
            let total = users.len() + roles.len();
            (total as u64 > u64::from(*limit)).then(|| format!("{total} mentions"))
        }
        Matcher::Links {
            block_all,
            block,
            allow,
        } => extract_hosts(text.raw).into_iter().find_map(|host| {
            let allowed = allow.iter().any(|d| domain_matches(&host, d));
            let blocked = *block_all || block.iter().any(|d| domain_matches(&host, d));
            (blocked && !allowed).then_some(host)
        }),
    }
}

// ---- text folding (open question 136) -----------------------------------------

/// Characters that render as nothing and only exist to split a word.
fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}' | '\u{034F}' | '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{206F}'
            | '\u{FE00}'..='\u{FE0F}' | '\u{FEFF}'
    )
}

/// Combining diacritics (strike-through and accent obfuscation).
fn is_combining(c: char) -> bool {
    matches!(
        c,
        '\u{0300}'..='\u{036F}' | '\u{1AB0}'..='\u{1AFF}' | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
    )
}

/// Map a lowercase letter that looks like a Latin one (Cyrillic, Greek and a few
/// others NFKC does not unify) to its Latin twin. Deliberately small: only
/// glyphs that are visually identical in common fonts.
fn confusable(c: char) -> char {
    match c {
        'а' | 'α' => 'a',
        'в' | 'β' => 'b',
        'с' | 'ϲ' => 'c',
        'ԁ' => 'd',
        'е' | 'ε' | 'ё' => 'e',
        'ѕ' => 's',
        'і' | 'ι' | 'ı' | 'ї' => 'i',
        'ј' => 'j',
        'к' | 'κ' => 'k',
        'м' => 'm',
        'о' | 'ο' => 'o',
        'р' | 'ρ' => 'p',
        'ԛ' => 'q',
        'т' | 'τ' => 't',
        'ν' => 'v',
        'ԝ' => 'w',
        'х' | 'χ' => 'x',
        'у' | 'υ' => 'y',
        'һ' => 'h',
        other => other,
    }
}

/// Canonical form AutoMod matches against: NFKC, lowercase, invisible characters and
/// combining marks removed, look-alike letters mapped to Latin. Both keywords (at
/// compile time) and message text go through it, so `Fuck`, a fullwidth spelling,
/// a Cyrillic `с` and a zero-width space inside the word all meet. Open question 136.
pub fn fold(text: &str) -> String {
    text.nfkc()
        .filter(|c| !is_invisible(*c) && !is_combining(*c))
        .flat_map(char::to_lowercase)
        .map(confusable)
        .collect()
}

// ---- per-guild compiled-rule cache (open question 136) -------------------------

/// How long compiled rules live before they are rebuilt from the repository. Rule
/// writes invalidate sooner; the TTL only bounds staleness across nodes.
pub const RULE_CACHE_TTL: Duration = Duration::from_secs(300);

/// Compiled rules per guild. Compiling regex sets is the expensive part of
/// AutoMod, so it happens once per guild, not once per message.
#[derive(Clone)]
pub struct RuleCache {
    inner: Cache<GuildId, Arc<Vec<CompiledRule>>>,
}

impl Default for RuleCache {
    fn default() -> Self {
        Self::new(RULE_CACHE_TTL)
    }
}

impl RuleCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Cache::builder()
                .time_to_live(ttl)
                .max_capacity(10_000)
                .build(),
        }
    }

    pub fn get(&self, guild: GuildId) -> Option<Arc<Vec<CompiledRule>>> {
        self.inner.get(&guild)
    }

    pub fn insert(&self, guild: GuildId, rules: Vec<CompiledRule>) -> Arc<Vec<CompiledRule>> {
        let rules = Arc::new(rules);
        self.inner.insert(guild, Arc::clone(&rules));
        rules
    }

    /// Drop the guild entry after any rule change.
    pub fn invalidate(&self, guild: GuildId) {
        self.inner.invalidate(&guild);
    }
}

// ---- per-member sliding window (open question 136) -----------------------------

pub const BURST_DUPLICATE: &str = "duplicate messages";
pub const BURST_RATE: &str = "message rate";

/// Tunables of [`SpamWindow`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpamWindowConfig {
    pub window_ms: i64,
    /// This many messages inside the window is a flood.
    pub max_messages: usize,
    /// This many identical messages inside the window is a repeat.
    pub max_duplicates: usize,
}

impl Default for SpamWindowConfig {
    fn default() -> Self {
        Self {
            window_ms: 15_000,
            max_messages: 8,
            max_duplicates: 4,
        }
    }
}

type Samples = Arc<Mutex<VecDeque<(i64, Option<u64>)>>>;

/// Sliding window of each member's recent messages, for the rate and duplicate
/// checks that a stateless rule cannot make. Entries expire with the window.
#[derive(Clone)]
pub struct SpamWindow {
    cfg: SpamWindowConfig,
    inner: Cache<(GuildId, UserId), Samples>,
}

impl Default for SpamWindow {
    fn default() -> Self {
        Self::new(SpamWindowConfig::default())
    }
}

fn content_key(content: &str) -> Option<u64> {
    let folded = fold(content);
    let t = folded.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        return None;
    }
    let mut h = DefaultHasher::new();
    t.hash(&mut h);
    Some(h.finish())
}

impl SpamWindow {
    pub fn new(cfg: SpamWindowConfig) -> Self {
        let ttl = Duration::from_millis(u64::try_from(cfg.window_ms.max(1)).unwrap_or(1));
        Self {
            cfg,
            inner: Cache::builder()
                .time_to_idle(ttl)
                .max_capacity(200_000)
                .build(),
        }
    }

    /// Record one message at `now_ms` and report why the member now looks like a
    /// spammer, if they do. Duplicates are checked before the overall rate.
    pub fn observe(
        &self,
        guild: GuildId,
        user: UserId,
        content: &str,
        now_ms: i64,
    ) -> Option<&'static str> {
        let samples = self
            .inner
            .get_with((guild, user), || Arc::new(Mutex::new(VecDeque::new())));
        // The deque only holds plain numbers, so a poisoned lock is still usable.
        let mut q = samples
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while q
            .front()
            .is_some_and(|(t, _)| now_ms - *t >= self.cfg.window_ms)
        {
            q.pop_front();
        }
        let key = content_key(content);
        q.push_back((now_ms, key));
        if let Some(k) = key
            && q.iter().filter(|(_, c)| *c == Some(k)).count() >= self.cfg.max_duplicates
        {
            return Some(BURST_DUPLICATE);
        }
        (q.len() >= self.cfg.max_messages).then_some(BURST_RATE)
    }
}

/// Blank out allow-listed words so they cannot trigger the keyword filter.
fn strip_allowed(content: &str, allow: Option<&RegexSet>) -> String {
    let Some(allow) = allow else {
        return content.to_owned();
    };
    if !allow.is_match(content) {
        return content.to_owned();
    }
    // Rebuild per-entry regexes lazily from the set patterns.
    let mut out = content.to_owned();
    for p in allow.patterns() {
        if let Ok(re) = RegexBuilder::new(p).case_insensitive(true).build() {
            out = re.replace_all(&out, " ").into_owned();
        }
    }
    out
}

fn domain_matches(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

/// Hosts of `http(s)://` and `www.` links in `content`, lowercased.
pub fn extract_hosts(content: &str) -> Vec<String> {
    let mut hosts = Vec::new();
    for token in
        content.split(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '(' | ')'))
    {
        let t = token.trim_matches(|c: char| matches!(c, '.' | ',' | ';' | '!' | '?'));
        let lower = t.to_lowercase();
        let rest = if let Some(r) = lower.strip_prefix("https://") {
            r
        } else if let Some(r) = lower.strip_prefix("http://") {
            r
        } else if lower.starts_with("www.") {
            lower.as_str()
        } else {
            continue;
        };
        let host = rest
            .split(['/', '?', '#'])
            .next()
            .unwrap_or("")
            .rsplit('@')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("");
        if host.contains('.') {
            hosts.push(host.to_owned());
        }
    }
    hosts
}

/// Stateless spam heuristics. Returns the reason, if any.
pub fn spam_reason(content: &str) -> Option<&'static str> {
    // Same character repeated 25+ times.
    let mut run = 0usize;
    let mut prev = None;
    for c in content.chars() {
        if Some(c) == prev && !c.is_whitespace() {
            run += 1;
            if run >= 25 {
                return Some("character flood");
            }
        } else {
            run = 1;
            prev = Some(c);
        }
    }
    // One word repeated 8+ times.
    let mut counts: HashMap<String, usize> = HashMap::new();
    for w in content.split_whitespace() {
        let c = counts.entry(w.to_lowercase()).or_default();
        *c += 1;
        if *c >= 8 && w.chars().count() >= 2 {
            return Some("repeated word");
        }
    }
    // Many distinct links in one message.
    let hosts = extract_hosts(content);
    if hosts.len() >= 5 {
        return Some("link flood");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: u64, trigger: Trigger, actions: Vec<Action>) -> AutoModRule {
        AutoModRule {
            id,
            guild_id: GuildId::new(1),
            name: format!("rule {id}"),
            creator_id: UserId::new(2),
            event_type: EventType::MessageSend,
            trigger,
            actions,
            enabled: true,
            exempt_role_ids: vec![],
            exempt_channel_ids: vec![],
        }
    }

    fn block() -> Vec<Action> {
        vec![Action::Block {
            custom_message: None,
        }]
    }

    fn kw(words: &[&str], regexes: &[&str], allow: &[&str]) -> Trigger {
        let v = |s: &[&str]| s.iter().map(|x| (*x).to_owned()).collect();
        Trigger::Keyword {
            keywords: v(words),
            regex_patterns: v(regexes),
            allow_list: v(allow),
        }
    }

    fn compile(r: AutoModRule) -> CompiledRule {
        CompiledRule::compile(r, &PresetLists::new()).unwrap()
    }

    fn ctx(text: &str) -> MessageContext {
        MessageContext {
            author_id: UserId::new(5),
            channel_id: ChannelId::new(10),
            content: text.to_owned(),
            ..Default::default()
        }
    }

    fn hit(rules: &[CompiledRule], text: &str) -> Option<String> {
        evaluate_message(rules, &ctx(text))
            .first()
            .map(|m| m.matched.clone())
    }

    #[test]
    fn keyword_wildcards() {
        let rules = [compile(rule(
            1,
            kw(&["cat", "dog*", "*fish", "*bird*"], &[], &[]),
            block(),
        ))];
        assert_eq!(hit(&rules, "my CAT sat").as_deref(), Some("cat"));
        assert_eq!(hit(&rules, "the catalog"), None); // whole-word only
        assert_eq!(hit(&rules, "doggo time").as_deref(), Some("dog*"));
        assert_eq!(hit(&rules, "a goldfish").as_deref(), Some("*fish"));
        assert_eq!(hit(&rules, "fishing"), None);
        assert_eq!(hit(&rules, "hummingbirds").as_deref(), Some("*bird*"));
        assert_eq!(hit(&rules, "cat."), Some("cat".into()));
    }

    #[test]
    fn regex_and_allow_list() {
        let rules = [compile(rule(
            1,
            kw(
                &["scunthorpe*"],
                &[r"\b\d{3}-\d{4}\b"],
                &["scunthorpe united"],
            ),
            block(),
        ))];
        assert_eq!(
            hit(&rules, "call 555-1234 now").as_deref(),
            Some(r"\b\d{3}-\d{4}\b")
        );
        assert_eq!(hit(&rules, "go Scunthorpe United!"), None);
        assert_eq!(
            hit(&rules, "scunthorpe town").as_deref(),
            Some("scunthorpe*")
        );
    }

    #[test]
    fn invalid_regex_and_limits_rejected() {
        assert!(rule(1, kw(&[], &["("], &[]), block()).validate().is_err());
        assert!(rule(1, kw(&[], &[], &[]), block()).validate().is_err());
        let many: Vec<String> = (0..=MAX_KEYWORDS).map(|i| format!("w{i}")).collect();
        let t = Trigger::Keyword {
            keywords: many,
            regex_patterns: vec![],
            allow_list: vec![],
        };
        assert!(rule(1, t, block()).validate().is_err());
        assert!(rule(1, kw(&["a"], &[], &[]), vec![]).validate().is_err());
        let long = "x".repeat(MAX_KEYWORD_CHARS + 1);
        assert!(
            rule(1, kw(&[long.as_str()], &[], &[]), block())
                .validate()
                .is_err()
        );
    }

    #[test]
    fn action_validation() {
        let timeout = |s| vec![Action::Timeout { duration_secs: s }];
        assert!(
            rule(1, kw(&["a"], &[], &[]), timeout(60))
                .validate()
                .is_ok()
        );
        assert!(
            rule(1, kw(&["a"], &[], &[]), timeout(0))
                .validate()
                .is_err()
        );
        assert!(
            rule(1, kw(&["a"], &[], &[]), timeout(MAX_TIMEOUT_SECS + 1))
                .validate()
                .is_err()
        );
        assert!(rule(1, Trigger::Spam, timeout(60)).validate().is_err());
        let long_msg = vec![Action::Block {
            custom_message: Some("x".repeat(151)),
        }];
        assert!(rule(1, Trigger::Spam, long_msg).validate().is_err());
        // Profile rules cannot block.
        let mut p = rule(1, kw(&["a"], &[], &[]), block());
        p.event_type = EventType::MemberUpdate;
        assert!(p.validate().is_err());
        p.actions = vec![Action::SendAlert {
            channel_id: ChannelId::new(3),
        }];
        assert!(p.validate().is_ok());
    }

    #[test]
    fn mention_spam_counts_distinct_mentions() {
        let rules = [compile(rule(
            1,
            Trigger::MentionSpam {
                total_limit: 3,
                raid_protection: false,
            },
            block(),
        ))];
        let mut c = ctx("hey");
        c.mentioned_user_ids = vec![UserId::new(1), UserId::new(1), UserId::new(2)];
        c.mentioned_role_ids = vec![RoleId::new(9)];
        assert!(evaluate_message(&rules, &c).is_empty()); // 3 distinct, not over
        c.mentioned_user_ids.push(UserId::new(3));
        assert_eq!(evaluate_message(&rules, &c).len(), 1);
        assert!(
            rule(
                1,
                Trigger::MentionSpam {
                    total_limit: 0,
                    raid_protection: false
                },
                block()
            )
            .validate()
            .is_err()
        );
    }

    #[test]
    fn link_rules() {
        let rules = [compile(rule(
            1,
            Trigger::Links {
                block_all: true,
                block_domains: vec![],
                allow_domains: vec!["example.com".into()],
            },
            block(),
        ))];
        assert_eq!(
            hit(&rules, "see https://evil.io/x?y=1").as_deref(),
            Some("evil.io")
        );
        assert_eq!(hit(&rules, "see https://docs.example.com/a"), None);
        assert_eq!(hit(&rules, "no links, just example.com text"), None);
        assert_eq!(
            hit(&rules, "www.bad.net now").as_deref(),
            Some("www.bad.net")
        );
        let only = [compile(rule(
            2,
            Trigger::Links {
                block_all: false,
                block_domains: vec!["bad.net".into()],
                allow_domains: vec![],
            },
            block(),
        ))];
        assert_eq!(
            hit(&only, "http://sub.bad.net/").as_deref(),
            Some("sub.bad.net")
        );
        assert_eq!(hit(&only, "http://good.org/"), None);
        assert_eq!(hit(&only, "http://notbad.net/"), None);
    }

    #[test]
    fn spam_heuristics() {
        let rules = [compile(rule(1, Trigger::Spam, block()))];
        assert!(hit(&rules, &"a".repeat(30)).is_some());
        assert!(hit(&rules, "buy buy buy buy buy buy buy buy").is_some());
        assert!(hit(&rules, "a normal sentence with words").is_none());
        let links = "http://a.io http://b.io http://c.io http://d.io http://e.io";
        assert_eq!(hit(&rules, links).as_deref(), Some("link flood"));
    }

    #[test]
    fn exemptions_and_disabled() {
        let mut r = rule(1, kw(&["bad"], &[], &[]), block());
        r.exempt_role_ids = vec![RoleId::new(7)];
        r.exempt_channel_ids = vec![ChannelId::new(99)];
        let rules = [compile(r)];
        assert_eq!(evaluate_message(&rules, &ctx("bad")).len(), 1);
        let mut c = ctx("bad");
        c.author_role_ids = vec![RoleId::new(7)];
        assert!(evaluate_message(&rules, &c).is_empty());
        let mut c = ctx("bad");
        c.parent_channel_id = Some(ChannelId::new(99));
        assert!(evaluate_message(&rules, &c).is_empty());
        let mut c = ctx("bad");
        c.bypass = true;
        assert!(evaluate_message(&rules, &c).is_empty());
        let mut off = rule(2, kw(&["bad"], &[], &[]), block());
        off.enabled = false;
        assert!(evaluate_message(&[compile(off)], &ctx("bad")).is_empty());
    }

    #[test]
    fn presets_and_profile_rules() {
        let mut lists = PresetLists::new();
        lists.insert(KeywordPreset::Profanity, vec!["heck".into()]);
        let preset = CompiledRule::compile(
            rule(
                1,
                Trigger::KeywordPreset {
                    presets: vec![KeywordPreset::Profanity],
                    allow_list: vec![],
                },
                block(),
            ),
            &lists,
        )
        .unwrap();
        assert_eq!(evaluate_message(&[preset], &ctx("oh heck")).len(), 1);

        let mut p = rule(
            2,
            kw(&["badname"], &[], &[]),
            vec![Action::SendAlert {
                channel_id: ChannelId::new(3),
            }],
        );
        p.event_type = EventType::MemberUpdate;
        let compiled = [compile(p)];
        let pc = ProfileContext {
            texts: vec!["fine".into(), "BadName".into()],
            ..Default::default()
        };
        assert_eq!(evaluate_profile(&compiled, &pc).len(), 1);
        // Message evaluation ignores profile rules.
        assert!(evaluate_message(&compiled, &ctx("badname")).is_empty());
    }

    #[test]
    fn quota() {
        let existing: Vec<_> = (1..=6)
            .map(|i| rule(i, kw(&["a"], &[], &[]), block()))
            .collect();
        let new = rule(7, kw(&["b"], &[], &[]), block());
        assert!(check_rule_quota(&existing, &new).is_err());
        assert!(check_rule_quota(&existing[..5], &new).is_ok());
        // Editing an existing rule does not count against itself.
        assert!(check_rule_quota(&existing, &existing[0]).is_ok());
        let spam = rule(8, Trigger::Spam, block());
        assert!(check_rule_quota(&existing, &spam).is_ok());
    }

    #[test]
    fn folding_defeats_common_evasions() {
        assert_eq!(fold("ＦＵＣＫ"), "fuck"); // fullwidth, NFKC
        assert_eq!(fold("fuсk"), "fuck"); // Cyrillic es
        assert_eq!(fold("f\u{200B}u\u{200D}ck"), "fuck"); // zero-width splits
        assert_eq!(fold("f\u{0336}uck"), "fuck"); // combining strike
        assert_eq!(fold("ΡΑΧ"), "pax"); // Greek capitals lowercase then map
        assert_eq!(fold("plain"), "plain");

        let rules = [compile(rule(1, kw(&["fuck"], &[], &[]), block()))];
        for evasion in ["ＦＵＣＫ you", "fuсk you", "f\u{200B}uck you", "FUCK"] {
            assert_eq!(hit(&rules, evasion).as_deref(), Some("fuck"), "{evasion}");
        }
        // Keywords are folded too, so a look-alike keyword matches plain text.
        let rules = [compile(rule(2, kw(&["fuсk"], &[], &[]), block()))];
        assert!(hit(&rules, "fuck").is_some());
        // Whole-word semantics survive folding.
        assert_eq!(hit(&rules, "fucked"), None);
    }

    #[test]
    fn regexes_see_text_as_written_and_folded() {
        let rules = [compile(rule(1, kw(&[], &[r"\bfuck\b"], &[]), block()))];
        assert!(hit(&rules, "ＦＵＣＫ").is_some());
        let rules = [compile(rule(1, kw(&[], &[r"ＦＵＣＫ"], &[]), block()))];
        assert!(hit(&rules, "ＦＵＣＫ").is_some());
    }

    #[test]
    fn rule_cache_returns_the_same_compiled_set_until_invalidated() {
        let cache = RuleCache::new(Duration::from_secs(60));
        let g = GuildId::new(1);
        assert!(cache.get(g).is_none());
        let first = cache.insert(g, vec![compile(rule(1, Trigger::Spam, block()))]);
        let again = cache.get(g).unwrap();
        assert!(Arc::ptr_eq(&first, &again));
        cache.invalidate(g);
        assert!(cache.get(g).is_none());
    }

    #[test]
    fn spam_window_flags_duplicates_and_rate_per_member() {
        let w = SpamWindow::new(SpamWindowConfig {
            window_ms: 10_000,
            max_messages: 5,
            max_duplicates: 3,
        });
        let (g, a, b) = (GuildId::new(1), UserId::new(1), UserId::new(2));
        assert_eq!(w.observe(g, a, "hello", 0), None);
        assert_eq!(w.observe(g, a, "HELLO ", 100), None);
        // Third copy (case and spacing folded away) trips the duplicate check.
        assert_eq!(w.observe(g, a, "hello", 200), Some(BURST_DUPLICATE));
        // Another member and another guild keep separate windows.
        assert_eq!(w.observe(g, b, "hello", 300), None);
        assert_eq!(w.observe(GuildId::new(2), a, "hello", 300), None);
        // Distinct messages trip the rate check on the fifth within the window.
        let c = UserId::new(3);
        for (i, t) in ["a", "b", "c", "d"].iter().enumerate() {
            assert_eq!(w.observe(g, c, t, i as i64 * 10), None);
        }
        assert_eq!(w.observe(g, c, "e", 50), Some(BURST_RATE));
        // Old samples fall out of the window.
        assert_eq!(w.observe(g, c, "f", 60_000), None);
    }

    #[test]
    fn burst_makes_spam_rules_match_and_respects_exemptions() {
        let rules = [compile(rule(1, Trigger::Spam, block()))];
        let c = ctx("totally normal text");
        assert!(evaluate_message(&rules, &c).is_empty());
        let m = evaluate_message_with(&rules, &c, Some(BURST_RATE));
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].matched, BURST_RATE);
        let bypass = MessageContext { bypass: true, ..c };
        assert!(evaluate_message_with(&rules, &bypass, Some(BURST_RATE)).is_empty());
    }
}
