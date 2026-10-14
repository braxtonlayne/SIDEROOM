//! Operator-editable donation settings and the perk tables of doc 08. Stored as one JSON row
//! (`payment_config`) and edited in Admin > Donations without a restart.

use serde::{Deserialize, Serialize};

pub const MIB: u64 = 1024 * 1024;

/// One allowed stream/camera quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoProfile {
    pub height: u16,
    pub fps: u8,
}

pub const fn vp(height: u16, fps: u8) -> VideoProfile {
    VideoProfile { height, fps }
}

/// What one tier (free, Patron, or a server level) allows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PerkSet {
    /// Chat upload size per file, MiB.
    pub upload_mib: u64,
    /// Allowed camera/screen-share profiles (a request is allowed when it fits one of them).
    pub video_profiles: Vec<VideoProfile>,
    /// Video senders per voice channel (a server level perk).
    pub video_senders: u16,
    /// "Go Live" audience cap.
    pub stream_viewers: u16,
    pub audio_kbps: u32,
    pub watch_parties: u16,
    /// Cinema/Library download requests per day; 0 = none (D-075).
    pub downloads_per_day: u32,
    /// Patron: this member's video never counts against a server's sender allowance (D-052).
    pub exempt_from_sender_limit: bool,
}

impl Default for PerkSet {
    fn default() -> Self {
        Self::free()
    }
}

impl PerkSet {
    pub fn free() -> Self {
        Self {
            upload_mib: 30,
            video_profiles: vec![vp(720, 30)],
            video_senders: 5,
            stream_viewers: 25,
            audio_kbps: 96,
            watch_parties: 2,
            downloads_per_day: 0,
            exempt_from_sender_limit: false,
        }
    }

    pub fn patron() -> Self {
        Self {
            upload_mib: 500,
            video_profiles: vec![vp(1080, 60)],
            video_senders: 0,
            stream_viewers: 0,
            audio_kbps: 0,
            watch_parties: 0,
            downloads_per_day: 20,
            exempt_from_sender_limit: true,
        }
    }

    pub fn level(n: usize) -> Self {
        let (up, vids, senders, viewers, kbps, parties) = match n {
            1 => (50, vec![vp(720, 60), vp(1080, 30)], 8, 50, 128, 4),
            2 => (100, vec![vp(1080, 60)], 12, 50, 256, 8),
            _ => (250, vec![vp(1080, 60)], 25, 100, 384, 16),
        };
        Self {
            upload_mib: up,
            video_profiles: vids,
            video_senders: senders,
            stream_viewers: viewers,
            audio_kbps: kbps,
            watch_parties: parties,
            downloads_per_day: 0,
            exempt_from_sender_limit: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DonationConfig {
    /// Master switch for creating invoices (a wallet must exist too).
    pub enabled: bool,
    /// Confirmations before a payment is credited.
    pub confirmations: u32,
    /// Credited invoices stay watched until this many confirmations (reorg window).
    pub finality_confirmations: u32,
    pub invoice_expiry_minutes: u32,
    /// The quote lock shown to the payer (the amount in sats never changes after creation).
    pub rate_lock_minutes: u32,
    /// Accepted shortfall and excess, basis points of the invoiced sats.
    pub tolerance_bps: u32,
    /// A late payment to an expired invoice is still watched this long.
    pub late_watch_days: u32,
    pub min_invoice_sats: u64,
    pub min_donation_cents: u32,
    pub max_donation_cents: u32,
    pub max_open_invoices: u32,
    /// Seconds between polls of an open invoice.
    pub poll_secs: u32,
    pub patron_price_cents: u32,
    pub patron_max_months: u32,
    pub patron_days_per_month: u32,
    /// Grace after Patron or a server level lapses (D-073).
    pub grace_days: u32,
    /// Pool thresholds of levels 1..3, USD cents.
    pub level_thresholds_cents: [u32; 3],
    pub pool_window_days: u32,
    pub invite_credit_price_cents: u32,
    pub max_invite_credits: u32,
    /// For the "hosting cost vs this month's donations" bar.
    pub hosting_cost_cents: u32,
    pub free: PerkSet,
    pub patron: PerkSet,
    pub levels: [PerkSet; 3],
    /// Explorer link template, `{txid}` and `{address}` are replaced; empty hides the link.
    pub explorer_tx_url: String,
}

impl Default for DonationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            confirmations: 1,
            finality_confirmations: 6,
            invoice_expiry_minutes: 60,
            rate_lock_minutes: 30,
            tolerance_bps: 100,
            late_watch_days: 30,
            min_invoice_sats: 1_000,
            min_donation_cents: 100,
            max_donation_cents: 1_000_000,
            max_open_invoices: 10,
            poll_secs: 30,
            patron_price_cents: 500,
            patron_max_months: 12,
            patron_days_per_month: 30,
            grace_days: 3,
            level_thresholds_cents: [1_000, 2_500, 5_000],
            pool_window_days: 30,
            invite_credit_price_cents: 2_000,
            max_invite_credits: 10,
            hosting_cost_cents: 12_800,
            free: PerkSet::free(),
            patron: PerkSet::patron(),
            levels: [PerkSet::level(1), PerkSet::level(2), PerkSet::level(3)],
            explorer_tx_url: "https://mempool.space/tx/{txid}".to_owned(),
        }
    }
}

impl DonationConfig {
    /// Range checks for an operator edit; `Err` names the first problem.
    pub fn validate(&self) -> Result<(), String> {
        fn within(name: &str, v: u64, lo: u64, hi: u64) -> Result<(), String> {
            if (lo..=hi).contains(&v) {
                Ok(())
            } else {
                Err(format!("{name} must be between {lo} and {hi}"))
            }
        }
        within("confirmations", self.confirmations.into(), 0, 12)?;
        within(
            "finality_confirmations",
            self.finality_confirmations.into(),
            1,
            100,
        )?;
        if self.finality_confirmations < self.confirmations {
            return Err("finality_confirmations cannot be below confirmations".into());
        }
        within(
            "invoice_expiry_minutes",
            self.invoice_expiry_minutes.into(),
            10,
            1440,
        )?;
        within("rate_lock_minutes", self.rate_lock_minutes.into(), 5, 1440)?;
        within("tolerance_bps", self.tolerance_bps.into(), 0, 1000)?;
        within("late_watch_days", self.late_watch_days.into(), 1, 365)?;
        within("min_invoice_sats", self.min_invoice_sats, 546, 10_000_000)?;
        within(
            "min_donation_cents",
            self.min_donation_cents.into(),
            1,
            100_000,
        )?;
        within(
            "max_donation_cents",
            self.max_donation_cents.into(),
            100,
            100_000_000,
        )?;
        if self.max_donation_cents < self.min_donation_cents {
            return Err("max_donation_cents cannot be below min_donation_cents".into());
        }
        within("max_open_invoices", self.max_open_invoices.into(), 1, 100)?;
        within("poll_secs", self.poll_secs.into(), 5, 3600)?;
        within(
            "patron_price_cents",
            self.patron_price_cents.into(),
            1,
            1_000_000,
        )?;
        within("patron_max_months", self.patron_max_months.into(), 1, 36)?;
        within(
            "patron_days_per_month",
            self.patron_days_per_month.into(),
            1,
            62,
        )?;
        within("grace_days", self.grace_days.into(), 0, 60)?;
        within("pool_window_days", self.pool_window_days.into(), 1, 365)?;
        within(
            "invite_credit_price_cents",
            self.invite_credit_price_cents.into(),
            1,
            10_000_000,
        )?;
        within("max_invite_credits", self.max_invite_credits.into(), 1, 100)?;
        within(
            "hosting_cost_cents",
            self.hosting_cost_cents.into(),
            0,
            1_000_000_000,
        )?;
        let t = self.level_thresholds_cents;
        if !(t[0] > 0 && t[0] < t[1] && t[1] < t[2]) {
            return Err("level thresholds must be positive and increasing".into());
        }
        for (name, p) in [("free", &self.free), ("patron", &self.patron)]
            .into_iter()
            .chain(self.levels.iter().map(|l| ("level", l)))
        {
            within(&format!("{name}.upload_mib"), p.upload_mib, 1, 100_000)?;
            within(&format!("{name}.audio_kbps"), p.audio_kbps.into(), 0, 1000)?;
            if p.video_profiles.len() > 8 {
                return Err(format!("{name}.video_profiles: at most 8 entries"));
            }
        }
        if !self.explorer_tx_url.is_empty()
            && !(self.explorer_tx_url.starts_with("https://")
                || self.explorer_tx_url.starts_with("http://"))
        {
            return Err("explorer_tx_url must be an http(s) URL".into());
        }
        Ok(())
    }

    /// The explorer link for a transaction, if configured.
    pub fn explorer_url(&self, txid: &str) -> Option<String> {
        (!self.explorer_tx_url.is_empty()).then(|| self.explorer_tx_url.replace("{txid}", txid))
    }
}

/// Effective limits of one member in one context (doc 08 section 5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    pub upload_bytes: u64,
    /// Files above the free upload size expire after this many days (D-043); `None` = never.
    pub large_upload_expiry_days: Option<u32>,
    pub standard_upload_bytes: u64,
    pub video_profiles: Vec<VideoProfile>,
    pub video_senders: u16,
    pub stream_viewers: u16,
    pub audio_kbps: u32,
    pub watch_parties: u16,
    pub downloads_per_day: u32,
    pub exempt_from_sender_limit: bool,
    pub patron: bool,
    pub guild_level: u8,
}

impl Limits {
    /// Whether a camera/screen profile fits what is allowed.
    pub fn allows_video(&self, height: u16, fps: u8) -> bool {
        self.video_profiles
            .iter()
            .any(|p| height <= p.height && fps <= p.fps)
    }

    /// The best single profile (largest pixel rate), for display.
    pub fn best_video(&self) -> Option<VideoProfile> {
        self.video_profiles
            .iter()
            .copied()
            .max_by_key(|p| u32::from(p.height) * u32::from(p.fps))
    }
}

fn union(a: &[VideoProfile], b: &[VideoProfile]) -> Vec<VideoProfile> {
    let mut out: Vec<VideoProfile> = Vec::new();
    for p in a.iter().chain(b) {
        // Drop profiles that another one dominates.
        if out.iter().any(|o| p.height <= o.height && p.fps <= o.fps) {
            continue;
        }
        out.retain(|o| !(o.height <= p.height && o.fps <= p.fps));
        out.push(*p);
    }
    out.sort_by_key(|p| (p.height, p.fps));
    out
}

/// `max(free, guild level perks, Patron perks)`. `guild_level` 0 is free; `patron` is active
/// (including grace). Tier-specific fields that a tier does not set (0) never lower a limit.
pub fn effective_limits(cfg: &DonationConfig, patron: bool, guild_level: u8) -> Limits {
    let mut sets: Vec<&PerkSet> = vec![&cfg.free];
    let level = usize::from(guild_level.min(3));
    if level > 0 {
        sets.push(&cfg.levels[level - 1]);
    }
    if patron {
        sets.push(&cfg.patron);
    }
    let mut profiles: Vec<VideoProfile> = Vec::new();
    for s in &sets {
        profiles = union(&profiles, &s.video_profiles);
    }
    let max = |f: fn(&PerkSet) -> u64| sets.iter().map(|s| f(s)).max().unwrap_or(0);
    let upload_mib = max(|s| s.upload_mib);
    Limits {
        upload_bytes: upload_mib * MIB,
        large_upload_expiry_days: (upload_mib > cfg.free.upload_mib).then_some(7),
        standard_upload_bytes: cfg.free.upload_mib * MIB,
        video_profiles: profiles,
        video_senders: u16::try_from(max(|s| s.video_senders.into())).unwrap_or(u16::MAX),
        stream_viewers: u16::try_from(max(|s| s.stream_viewers.into())).unwrap_or(u16::MAX),
        audio_kbps: u32::try_from(max(|s| s.audio_kbps.into())).unwrap_or(u32::MAX),
        watch_parties: u16::try_from(max(|s| s.watch_parties.into())).unwrap_or(u16::MAX),
        downloads_per_day: u32::try_from(max(|s| s.downloads_per_day.into())).unwrap_or(u32::MAX),
        exempt_from_sender_limit: sets.iter().any(|s| s.exempt_from_sender_limit),
        patron,
        guild_level: level as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_validate_and_match_doc_08() {
        let c = DonationConfig::default();
        assert!(c.validate().is_ok());
        assert_eq!(c.patron_price_cents, 500);
        assert_eq!(c.level_thresholds_cents, [1000, 2500, 5000]);
        assert_eq!(c.invite_credit_price_cents, 2000);
        assert_eq!(c.grace_days, 3);
        assert_eq!(c.invoice_expiry_minutes, 60);
        // JSON round trip, and a partial document falls back to defaults.
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<DonationConfig>(&json).unwrap(), c);
        let partial: DonationConfig = serde_json::from_str(r#"{"grace_days": 5}"#).unwrap();
        assert_eq!(partial.grace_days, 5);
        assert_eq!(partial.patron_price_cents, 500);
    }

    #[test]
    fn validation_catches_nonsense() {
        let d = DonationConfig::default;
        let bad = [
            DonationConfig {
                level_thresholds_cents: [2000, 1000, 5000],
                ..d()
            },
            DonationConfig {
                finality_confirmations: 0,
                confirmations: 1,
                ..d()
            },
            DonationConfig {
                tolerance_bps: 5000,
                ..d()
            },
            DonationConfig {
                explorer_tx_url: "ftp://x".into(),
                ..d()
            },
        ];
        for c in bad {
            assert!(c.validate().is_err());
        }
    }

    #[test]
    fn free_limits() {
        let c = DonationConfig::default();
        let l = effective_limits(&c, false, 0);
        assert_eq!(l.upload_bytes, 30 * MIB);
        assert_eq!(l.large_upload_expiry_days, None);
        assert_eq!(l.video_senders, 5);
        assert!(l.allows_video(720, 30));
        assert!(!l.allows_video(720, 60));
        assert!(!l.allows_video(1080, 30));
        assert_eq!(l.downloads_per_day, 0);
        assert!(!l.exempt_from_sender_limit);
    }

    #[test]
    fn patron_overrides_follow_the_user() {
        let c = DonationConfig::default();
        let l = effective_limits(&c, true, 0);
        assert_eq!(l.upload_bytes, 500 * MIB);
        assert_eq!(l.large_upload_expiry_days, Some(7));
        assert!(l.allows_video(1080, 60));
        assert!(l.exempt_from_sender_limit);
        assert_eq!(l.downloads_per_day, 20);
        // A Patron in a free server still gets the server's free sender count.
        assert_eq!(l.video_senders, 5);
    }

    #[test]
    fn server_level_and_patron_take_the_maximum() {
        let c = DonationConfig::default();
        let l1 = effective_limits(&c, false, 1);
        assert_eq!(l1.upload_bytes, 50 * MIB);
        assert!(l1.allows_video(720, 60));
        assert!(l1.allows_video(1080, 30));
        assert!(!l1.allows_video(1080, 60));
        assert_eq!(l1.video_senders, 8);
        assert_eq!(l1.audio_kbps, 128);
        assert_eq!(
            l1.downloads_per_day, 0,
            "server levels never unlock downloads (D-075)"
        );
        let l3p = effective_limits(&c, true, 3);
        assert_eq!(l3p.upload_bytes, 500 * MIB);
        assert_eq!(l3p.video_senders, 25);
        assert_eq!(l3p.stream_viewers, 100);
        assert_eq!(l3p.audio_kbps, 384);
        assert_eq!(l3p.watch_parties, 16);
        assert_eq!(l3p.best_video(), Some(vp(1080, 60)));
        assert_eq!(l3p.guild_level, 3);
        // Out-of-range levels clamp.
        assert_eq!(effective_limits(&c, false, 9).guild_level, 3);
    }

    #[test]
    fn explorer_link() {
        let c = DonationConfig::default();
        assert_eq!(
            c.explorer_url("abc").unwrap(),
            "https://mempool.space/tx/abc"
        );
        let mut c = c;
        c.explorer_tx_url.clear();
        assert!(c.explorer_url("abc").is_none());
    }
}
