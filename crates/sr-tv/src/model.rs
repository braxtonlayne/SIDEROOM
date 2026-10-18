//! Wire and domain types of Live TV (docs/specs/cinema-live-tv.md section 8).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::tz::Zone;

/// Serde adapter: snowflakes travel as decimal strings.
pub mod id {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &i64, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(v)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            S(String),
            N(i64),
        }
        match Raw::deserialize(d)? {
            Raw::S(s) => s.parse().map_err(serde::de::Error::custom),
            Raw::N(n) => Ok(n),
        }
    }
}

/// Serde adapter for `Option<i64>` ids.
pub mod opt_id {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<i64>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(v) => s.collect_str(v),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            S(String),
            N(i64),
        }
        match Option::<Raw>::deserialize(d)? {
            None => Ok(None),
            Some(Raw::S(s)) if s.is_empty() => Ok(None),
            Some(Raw::S(s)) => s.parse().map(Some).map_err(serde::de::Error::custom),
            Some(Raw::N(n)) => Ok(Some(n)),
        }
    }
}

// ---- enums ------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TvChannelKind)]
pub enum ChannelKind {
    RuleBased,
    Manual,
    /// A weekly grid of slots imported from a historical guide (TV-ENG-003).
    ImportedGuide,
}

impl ChannelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RuleBased => "rule_based",
            Self::Manual => "manual",
            Self::ImportedGuide => "imported_guide",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "rule_based" => Some(Self::RuleBased),
            "manual" => Some(Self::Manual),
            "imported_guide" => Some(Self::ImportedGuide),
            _ => None,
        }
    }
}

/// What fills the time between programmes (spec section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TvFillerPolicy)]
pub enum FillerPolicy {
    Bumpers,
    NextProgramEarly,
    Slate,
    OffAir,
}

impl FillerPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bumpers => "bumpers",
            Self::NextProgramEarly => "next_program_early",
            Self::Slate => "slate",
            Self::OffAir => "off_air",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "bumpers" => Some(Self::Bumpers),
            "next_program_early" => Some(Self::NextProgramEarly),
            "slate" => Some(Self::Slate),
            "off_air" => Some(Self::OffAir),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TvBlockKind)]
pub enum BlockKind {
    Program,
    Filler,
    Slate,
    OffAir,
}

impl BlockKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Program => "program",
            Self::Filler => "filler",
            Self::Slate => "slate",
            Self::OffAir => "off_air",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "program" => Some(Self::Program),
            "filler" => Some(Self::Filler),
            "slate" => Some(Self::Slate),
            "off_air" => Some(Self::OffAir),
            _ => None,
        }
    }

    /// Whether a stream exists for the block.
    pub fn has_media(self) -> bool {
        matches!(self, Self::Program | Self::Filler)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TvBlockSource)]
pub enum BlockSource {
    Generated,
    Manual,
    Override,
}

impl BlockSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::Manual => "manual",
            Self::Override => "override",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "generated" => Some(Self::Generated),
            "manual" => Some(Self::Manual),
            "override" => Some(Self::Override),
            _ => None,
        }
    }
}

/// The catalog kind of a title as Live TV sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TvMediaKind)]
pub enum MediaKind {
    Movie,
    Series,
    Anime,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Series => "series",
            Self::Anime => "anime",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "movie" => Some(Self::Movie),
            "series" => Some(Self::Series),
            "anime" => Some(Self::Anime),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TvPickMode)]
pub enum PickMode {
    #[default]
    Shuffle,
    Sequential,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TvFit)]
pub enum Fit {
    /// The last programme may run past the end of the daypart.
    #[default]
    Overrun,
    /// Only items that end before the daypart does.
    Fit,
}

/// How a pool treats age-restricted titles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TvRestricted)]
pub enum Restricted {
    Exclude,
    Include,
    Only,
}

// ---- rules ------------------------------------------------------------------------------------

/// What may air in a daypart. Empty lists do not filter.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = TvPool)]
pub struct PoolFilter {
    #[serde(default)]
    pub kinds: Vec<MediaKind>,
    /// A title must have every genre (lower case).
    #[serde(default)]
    pub genres: Vec<String>,
    /// A title must have every tag (lower case).
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    #[schema(value_type = Vec<String>)]
    #[serde(with = "id_list")]
    pub title_ids: Vec<i64>,
    #[serde(default)]
    pub year_from: Option<i16>,
    #[serde(default)]
    pub year_to: Option<i16>,
    #[serde(default)]
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub collection_id: Option<i64>,
    #[serde(default)]
    pub min_runtime_min: Option<i32>,
    #[serde(default)]
    pub max_runtime_min: Option<i32>,
    /// Default: exclude, or include on an age-restricted channel.
    #[serde(default)]
    pub restricted: Option<Restricted>,
}

/// Serde adapter for id lists.
pub mod id_list {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(v: &[i64], s: S) -> Result<S::Ok, S::Error> {
        v.iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<i64>, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            S(String),
            N(i64),
        }
        Vec::<Raw>::deserialize(d)?
            .into_iter()
            .map(|r| match r {
                Raw::S(s) => s.parse().map_err(serde::de::Error::custom),
                Raw::N(n) => Ok(n),
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = TvDaypart)]
pub struct Daypart {
    pub name: String,
    /// `HH:MM` channel time.
    pub start: String,
    /// `HH:MM` channel time; not after `start` means past midnight.
    pub end: String,
    /// 0 = Monday. Empty: every day.
    #[serde(default)]
    pub days: Vec<u8>,
    #[serde(default)]
    pub mode: PickMode,
    #[serde(default)]
    pub fit: Fit,
    #[serde(default)]
    pub pool: PoolFilter,
}

fn default_grid() -> u32 {
    30
}

fn default_window() -> u32 {
    8
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = TvRules)]
pub struct ChannelRules {
    /// Programmes start on this grid in channel time (0: back to back).
    #[serde(default = "default_grid")]
    pub grid_minutes: u32,
    /// Shuffle never repeats one of the last N picks (0 to 64).
    #[serde(default = "default_window")]
    pub no_repeat_window: u32,
    #[serde(default)]
    pub dayparts: Vec<Daypart>,
}

impl Default for ChannelRules {
    fn default() -> Self {
        Self {
            grid_minutes: default_grid(),
            no_repeat_window: default_window(),
            dayparts: Vec::new(),
        }
    }
}

/// `HH:MM` to minutes after midnight (`24:00` is allowed).
pub fn parse_hm(s: &str) -> Option<i64> {
    let (h, m) = s.split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (i64, i64) = (h.parse().ok()?, m.parse().ok()?);
    ((h < 24 && m < 60) || (h == 24 && m == 0)).then_some(h * 60 + m)
}

impl ChannelRules {
    /// A message for the first problem found.
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.grid_minutes, 0 | 1 | 5 | 10 | 15 | 20 | 30 | 60) {
            return Err("grid_minutes must be 0, 1, 5, 10, 15, 20, 30 or 60".into());
        }
        if self.no_repeat_window > 64 {
            return Err("no_repeat_window is at most 64".into());
        }
        if self.dayparts.len() > 24 {
            return Err("At most 24 dayparts".into());
        }
        let mut names = std::collections::BTreeSet::new();
        for d in &self.dayparts {
            let name = d.name.trim();
            if name.is_empty() || name.chars().count() > 40 {
                return Err("A daypart needs a name of 1 to 40 characters".into());
            }
            if !names.insert(name.to_lowercase()) {
                return Err(format!("Daypart name {name:?} is used twice"));
            }
            let (Some(s), Some(e)) = (parse_hm(&d.start), parse_hm(&d.end)) else {
                return Err(format!("Daypart {name:?}: times are HH:MM"));
            };
            if s >= 24 * 60 {
                return Err(format!("Daypart {name:?}: start must be before 24:00"));
            }
            if s == e {
                return Err(format!("Daypart {name:?}: start and end are equal"));
            }
            if d.days.iter().any(|x| *x > 6) {
                return Err(format!("Daypart {name:?}: days are 0 (Monday) to 6"));
            }
            let p = &d.pool;
            if p.kinds.len() > 3
                || p.genres.len() > 20
                || p.tags.len() > 20
                || p.title_ids.len() > 200
            {
                return Err(format!("Daypart {name:?}: the pool lists are too long"));
            }
            if let (Some(a), Some(b)) = (p.year_from, p.year_to)
                && a > b
            {
                return Err(format!("Daypart {name:?}: year_from is after year_to"));
            }
            if let (Some(a), Some(b)) = (p.min_runtime_min, p.max_runtime_min)
                && a > b
            {
                return Err(format!(
                    "Daypart {name:?}: min runtime is above the maximum"
                ));
            }
        }
        Ok(())
    }

    /// Lower-case the genre and tag filters and drop duplicates.
    pub fn normalise(&mut self) {
        for d in &mut self.dayparts {
            d.name = d.name.trim().to_owned();
            d.days.sort_unstable();
            d.days.dedup();
            for list in [&mut d.pool.genres, &mut d.pool.tags] {
                for v in list.iter_mut() {
                    *v = v.trim().to_lowercase();
                }
                list.retain(|v| !v.is_empty());
                list.sort();
                list.dedup();
            }
            d.pool.title_ids.dedup();
        }
    }
}

// ---- channels ---------------------------------------------------------------------------------

/// Staff input to create or replace a channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = TvChannelInput)]
pub struct ChannelInput {
    pub number: i32,
    pub name: String,
    #[serde(default)]
    pub call_sign: Option<String>,
    #[serde(default)]
    pub era_label: Option<String>,
    #[serde(default)]
    pub theme: Option<String>,
    /// Storage key of the logo; must start with `tv/`.
    #[serde(default)]
    pub logo_key: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub kind: ChannelKind,
    #[serde(default = "utc")]
    pub timezone: String,
    #[serde(default)]
    pub age_restricted: bool,
    #[serde(default = "bumpers")]
    pub filler_policy: FillerPolicy,
    #[serde(default)]
    pub rules: ChannelRules,
}

fn yes() -> bool {
    true
}

fn utc() -> String {
    "UTC".into()
}

fn bumpers() -> FillerPolicy {
    FillerPolicy::Bumpers
}

fn trimmed(v: &Option<String>, max: usize, what: &str) -> Result<Option<String>, String> {
    match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) if s.chars().count() <= max => Ok(Some(s.to_owned())),
        Some(_) => Err(format!("{what} is at most {max} characters")),
    }
}

impl ChannelInput {
    /// Validate and normalise (trim text, lower-case filters).
    pub fn checked(mut self) -> Result<Self, String> {
        if !(1..=9999).contains(&self.number) {
            return Err("number must be 1 to 9999".into());
        }
        self.name = self.name.trim().to_owned();
        if self.name.is_empty() || self.name.chars().count() > 80 {
            return Err("name must be 1 to 80 characters".into());
        }
        self.call_sign = trimmed(&self.call_sign, 40, "call_sign")?;
        self.era_label = trimmed(&self.era_label, 60, "era_label")?;
        self.theme = trimmed(&self.theme, 40, "theme")?;
        self.logo_key = trimmed(&self.logo_key, 300, "logo_key")?;
        if let Some(k) = &self.logo_key
            && (!k.starts_with("tv/") || k.contains(".."))
        {
            return Err("logo_key must start with tv/".into());
        }
        if Zone::parse(&self.timezone).is_none() {
            return Err(format!("Unknown timezone {:?}", self.timezone));
        }
        self.rules.validate()?;
        self.rules.normalise();
        Ok(self)
    }
}

/// A stored channel.
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelRecord {
    pub id: i64,
    pub input: ChannelInput,
    pub seed: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl ChannelRecord {
    pub fn zone(&self) -> Zone {
        Zone::parse(&self.input.timezone).unwrap_or_else(Zone::utc)
    }
}

/// A channel as the API shows it.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvChannel)]
pub struct Channel {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub number: i32,
    pub name: String,
    pub call_sign: Option<String>,
    pub era_label: Option<String>,
    pub theme: Option<String>,
    /// Signed `/cdn/...` URL of the logo.
    pub logo_url: Option<String>,
    pub kind: ChannelKind,
    pub timezone: String,
    pub age_restricted: bool,
    pub enabled: bool,
    pub filler_policy: FillerPolicy,
    /// Distinct members who fetched a live playlist in the last 45 seconds.
    pub viewers: u32,
    pub now: Option<Block>,
    pub next: Option<Block>,
    /// Staff only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules: Option<ChannelRules>,
    /// Staff only: the shuffle seed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub seed: Option<String>,
}

// ---- blocks -----------------------------------------------------------------------------------

/// Generator state that makes a rolling schedule continue where it stopped.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    /// Where generation resumes (the end of the last block).
    pub at_ms: i64,
    /// Next index of each sequential daypart, by daypart name.
    #[serde(default)]
    pub seq: BTreeMap<String, u64>,
    /// Keys (episode or title id) of the latest picks, newest last.
    #[serde(default)]
    pub recent: Vec<i64>,
    /// Shuffle generator state.
    #[serde(default)]
    pub rng: u64,
    /// Next bumper.
    #[serde(default)]
    pub bumper: u64,
}

/// A stored schedule block.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockRecord {
    pub id: i64,
    pub channel_id: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub kind: BlockKind,
    pub title_id: Option<i64>,
    pub episode_id: Option<i64>,
    pub asset_id: Option<i64>,
    pub title: String,
    pub episode_title: Option<String>,
    pub media_kind: Option<MediaKind>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub year: Option<i16>,
    pub age_restricted: bool,
    /// Position inside the asset at `start_ms` (after a head trim or an "on now" start).
    pub offset_ms: i64,
    pub source: BlockSource,
    /// Generator state after this block (generated blocks only).
    pub cursor_after: Option<Cursor>,
    /// HLS media sequence of the asset's first segment, assigned lazily (see `live`).
    pub seq_base: Option<i64>,
    /// HLS discontinuity sequence of the block, assigned together with `seq_base`.
    pub disc_base: Option<i64>,
}

impl BlockRecord {
    pub fn duration_ms(&self) -> i64 {
        self.end_ms - self.start_ms
    }

    pub fn contains(&self, t: i64) -> bool {
        self.start_ms <= t && t < self.end_ms
    }

    pub fn wire(&self) -> Block {
        Block {
            id: self.id,
            channel_id: self.channel_id,
            start_ms: self.start_ms,
            end_ms: self.end_ms,
            kind: self.kind,
            title: self.title.clone(),
            episode_title: self.episode_title.clone(),
            title_id: self.title_id,
            episode_id: self.episode_id,
            media_kind: self.media_kind,
            season_number: self.season_number,
            episode_number: self.episode_number,
            year: self.year,
            age_restricted: self.age_restricted,
            playable: self.kind.has_media() && self.asset_id.is_some(),
            source: self.source,
        }
    }
}

/// A block as the API shows it.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvBlock)]
pub struct Block {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub channel_id: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub kind: BlockKind,
    pub title: String,
    pub episode_title: Option<String>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub title_id: Option<i64>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub episode_id: Option<i64>,
    pub media_kind: Option<MediaKind>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub year: Option<i16>,
    pub age_restricted: bool,
    pub playable: bool,
    pub source: BlockSource,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> ChannelInput {
        serde_json::from_value(serde_json::json!({
            "number": 101, "name": " Toonami ", "kind": "rule_based",
            "timezone": "America/New_York",
            "rules": {"dayparts": [{"name": "Prime", "start": "20:00", "end": "02:00",
                "days": [4, 4, 5], "pool": {"genres": [" Action "], "title_ids": ["12", 13]}}]}
        }))
        .unwrap()
    }

    #[test]
    fn input_defaults_and_normalisation() {
        let c = input().checked().unwrap();
        assert_eq!(c.name, "Toonami");
        assert!(c.enabled);
        assert_eq!(c.filler_policy, FillerPolicy::Bumpers);
        assert_eq!(c.rules.grid_minutes, 30);
        let d = &c.rules.dayparts[0];
        assert_eq!(d.days, vec![4, 5]);
        assert_eq!(d.pool.genres, vec!["action"]);
        assert_eq!(d.pool.title_ids, vec![12, 13]);
        assert_eq!(d.mode, PickMode::Shuffle);
    }

    #[test]
    fn invalid_input_is_explained() {
        let mut c = input();
        c.number = 0;
        assert!(c.checked().unwrap_err().contains("number"));
        let mut c = input();
        c.timezone = "Nowhere/Land".into();
        assert!(c.checked().unwrap_err().contains("timezone"));
        let mut c = input();
        c.logo_key = Some("cinema/x.png".into());
        assert!(c.checked().unwrap_err().contains("tv/"));
        let mut c = input();
        c.rules.dayparts[0].start = "25:00".into();
        assert!(c.checked().unwrap_err().contains("HH:MM"));
        let mut c = input();
        c.rules.dayparts[0].end = "20:00".into();
        assert!(c.checked().unwrap_err().contains("equal"));
        let mut c = input();
        let d = c.rules.dayparts[0].clone();
        c.rules.dayparts.push(d);
        assert!(c.checked().unwrap_err().contains("twice"));
        let mut c = input();
        c.rules.grid_minutes = 7;
        assert!(c.checked().unwrap_err().contains("grid"));
    }

    #[test]
    fn hm_parses_strictly() {
        assert_eq!(parse_hm("00:00"), Some(0));
        assert_eq!(parse_hm("24:00"), Some(1440));
        assert_eq!(parse_hm("9:30"), None);
        assert_eq!(parse_hm("12:60"), None);
    }
}

// ---- weekly slots and guide imports (TV-ENG-001, TV-ENG-003, TV-ADM-003, TV-ADM-004) -------------

/// One weekly slot of an `imported_guide` channel, as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotRecord {
    pub id: i64,
    pub channel_id: i64,
    /// 0 = Monday, channel time.
    pub weekday: u8,
    /// Minutes after local midnight, 0 to 1439.
    pub start_minute: u16,
    pub duration_min: u16,
    /// The show text as imported or typed.
    pub show_name: String,
    /// The matched catalog title; none while the slot matches nothing (skipped when resolving).
    pub title_id: Option<i64>,
    pub episode_id: Option<i64>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub note: Option<String>,
    pub import_id: Option<i64>,
}

/// A weekly slot as the API shows and accepts it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = TvSlot)]
pub struct Slot {
    /// Ignored on input.
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub id: Option<i64>,
    /// 0 = Monday, in the channel time zone.
    pub weekday: u8,
    /// `HH:MM` in the channel time zone.
    pub start: String,
    pub duration_min: u16,
    pub show_name: String,
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub title_id: Option<i64>,
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub episode_id: Option<i64>,
    #[serde(default)]
    pub season_number: Option<i32>,
    #[serde(default)]
    pub episode_number: Option<i32>,
    #[serde(default)]
    pub note: Option<String>,
}

impl Slot {
    pub fn of(r: &SlotRecord) -> Self {
        Self {
            id: Some(r.id),
            weekday: r.weekday,
            start: format!("{:02}:{:02}", r.start_minute / 60, r.start_minute % 60),
            duration_min: r.duration_min,
            show_name: r.show_name.clone(),
            title_id: r.title_id,
            episode_id: r.episode_id,
            season_number: r.season_number,
            episode_number: r.episode_number,
            note: r.note.clone(),
        }
    }

    /// Validate and normalise into a record (ids are assigned by the caller).
    pub fn checked(&self, channel_id: i64, id: i64) -> Result<SlotRecord, String> {
        if self.weekday > 6 {
            return Err("weekday must be 0 (Monday) to 6 (Sunday)".into());
        }
        let start = parse_hm(&self.start)
            .filter(|m| *m < 1440)
            .ok_or("start must be HH:MM")?;
        if !(1..=1440).contains(&self.duration_min) {
            return Err("duration_min must be 1 to 1440".into());
        }
        let show = self.show_name.trim();
        if show.is_empty() || show.chars().count() > 200 {
            return Err("show_name must be 1 to 200 characters".into());
        }
        let note = trimmed(&self.note, 200, "note")?;
        Ok(SlotRecord {
            id,
            channel_id,
            weekday: self.weekday,
            start_minute: u16::try_from(start).map_err(|_| "start must be HH:MM")?,
            duration_min: self.duration_min,
            show_name: show.to_owned(),
            title_id: self.title_id,
            episode_id: self.episode_id,
            season_number: self.season_number,
            episode_number: self.episode_number,
            note,
            import_id: None,
        })
    }
}

/// A stored guide import waiting for confirmation (or confirmed).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportRecord {
    pub id: i64,
    pub channel_id: i64,
    pub created_by: Option<i64>,
    pub format: String,
    pub confirmed: bool,
    pub rows: serde_json::Value,
    pub report: serde_json::Value,
    pub created_at_ms: i64,
}

/// A member's reminder for one upcoming block (TV-PLY-005). The title and start are a snapshot taken when
/// the reminder was set, so a notice can still name the programme after the schedule changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReminderRecord {
    pub account_id: i64,
    pub block_id: i64,
    pub channel_id: i64,
    pub title: String,
    pub start_ms: i64,
    pub remind_at_ms: i64,
}
