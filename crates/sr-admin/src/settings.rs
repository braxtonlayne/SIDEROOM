//! The catalog of platform settings and limits that Admin -> Limits edits
//! (doc 07 `MANAGE_FEATURES`, spec admin section 5). Values live in the
//! `platform_settings` table; a key with no row has its default here.

use serde::Serialize;
use serde_json::{Value, json};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SettingKind {
    Integer,
    Number,
    Boolean,
    Text,
}

/// One editable setting.
#[derive(Debug, Clone, Copy)]
pub struct SettingDef {
    pub key: &'static str,
    pub group: &'static str,
    pub kind: SettingKind,
    pub min: f64,
    pub max: f64,
    pub default_int: i64,
    pub description: &'static str,
}

impl SettingDef {
    pub fn default_value(&self) -> Value {
        match self.kind {
            SettingKind::Integer => json!(self.default_int),
            SettingKind::Number => json!(self.default_int as f64),
            SettingKind::Boolean => json!(self.default_int != 0),
            SettingKind::Text => json!(""),
        }
    }

    /// Check `v` against the kind and range; returns the value to store.
    pub fn validate(&self, v: &Value) -> Result<Value, String> {
        match self.kind {
            SettingKind::Integer => {
                let n = v.as_i64().ok_or("must be a whole number")?;
                if (n as f64) < self.min || (n as f64) > self.max {
                    return Err(format!("must be between {} and {}", self.min, self.max));
                }
                Ok(json!(n))
            }
            SettingKind::Number => {
                let n = v.as_f64().ok_or("must be a number")?;
                if !n.is_finite() || n < self.min || n > self.max {
                    return Err(format!("must be between {} and {}", self.min, self.max));
                }
                Ok(json!(n))
            }
            SettingKind::Boolean => v
                .as_bool()
                .map(|b| json!(b))
                .ok_or_else(|| "must be true or false".to_owned()),
            SettingKind::Text => {
                let s = v.as_str().ok_or("must be text")?;
                if self.key == TV_SLATE_ASSET && !s.chars().all(|c| c.is_ascii_digit()) {
                    return Err("must be an asset id (digits) or empty".to_owned());
                }
                if self.key == BYOS_WHO_MAY_MOUNT
                    && !s.is_empty()
                    && !WHO_MAY_MOUNT_VALUES.contains(&s)
                {
                    return Err("must be all, patrons or staff".to_owned());
                }
                if self.key == BYOS_ENDPOINT_ALLOW {
                    endpoint_allow_valid(s)?;
                }
                if s.chars().count() as f64 > self.max {
                    return Err(format!("must be at most {} characters", self.max));
                }
                Ok(json!(s))
            }
        }
    }
}

/// Each item of the endpoint allow-list is a host or a `*.suffix` (the parser in `sr-storage` is the
/// authority; this refuses obvious junk at the door so the Admin form can say so).
fn endpoint_allow_valid(text: &str) -> Result<(), String> {
    let items: Vec<&str> = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|i| !i.is_empty())
        .collect();
    if items.len() > ENDPOINT_ALLOW_MAX_PATTERNS {
        return Err(format!("at most {ENDPOINT_ALLOW_MAX_PATTERNS} patterns"));
    }
    for i in items {
        let host = i
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        let chars_ok = host.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '*' | '_' | ':' | '/' | '[' | ']')
        });
        let star_ok = !host.contains('*') || (host.starts_with("*.") && !host[2..].contains('*'));
        if host.is_empty() || !chars_ok || !star_ok {
            return Err(format!(
                "`{i}` is not a host or a *.suffix pattern (for example s3.example.com or *.example.com)"
            ));
        }
    }
    Ok(())
}

const fn int(
    key: &'static str,
    group: &'static str,
    min: i64,
    max: i64,
    default_int: i64,
    description: &'static str,
) -> SettingDef {
    SettingDef {
        key,
        group,
        kind: SettingKind::Integer,
        min: min as f64,
        max: max as f64,
        default_int,
        description,
    }
}

/// Account-cap setting key (read by registration).
pub const ACCOUNT_CAP: &str = "accounts.cap";
/// Inviter suspicion thresholds (doc 06 section 5.4).
pub const SUSPICION_FREEZE: &str = "invites.suspicion_freeze_threshold";
pub const SUSPICION_ESCALATE: &str = "invites.suspicion_escalate_threshold";

/// Live HTTP rate limits (H-39), per minute. The server applies a change on the next request.
pub const RATE_IP: &str = "limits.rate.ip";
pub const RATE_SESSION: &str = "limits.rate.session";
pub const RATE_BOT: &str = "limits.rate.bot";
pub const RATE_MESSAGES: &str = "limits.rate.messages";
pub const RATE_UPLOADS: &str = "limits.rate.uploads";

/// Abuse limits on member-writable surfaces (RDY-SEC-006). Each is read live by the limiter (HTTP
/// ones) or by the domain code (the last two) and listed in doc 05 "Abuse limits". The group is `abuse`.
pub const ABUSE_FRIEND_REQUESTS: &str = "abuse.friend_requests_per_hour";
pub const ABUSE_DM_OPENS: &str = "abuse.dm_opens_per_hour";
pub const ABUSE_WEBHOOK_EXECUTE: &str = "abuse.webhook_execute_per_minute";
pub const ABUSE_OAUTH_AUTHORIZE: &str = "abuse.oauth_authorize_per_minute";
pub const ABUSE_INTERACTION_CALLBACKS: &str = "abuse.interaction_callbacks_per_minute";
pub const ABUSE_WATCH_PARTY_JOINS: &str = "abuse.watch_party_joins_per_minute";
pub const ABUSE_EMOJI_UPLOADS: &str = "abuse.emoji_uploads_per_hour";
pub const ABUSE_STICKER_UPLOADS: &str = "abuse.sticker_uploads_per_hour";
pub const ABUSE_APPEAL_ATTEMPTS: &str = "abuse.appeal_attempts_per_hour";
pub const ABUSE_STORAGE_TESTS: &str = "abuse.storage_tests_per_hour";
pub const ABUSE_COUNTER_NOTICES: &str = "abuse.counter_notices_per_day";
pub const ABUSE_DATA_EXPORT_COOLDOWN: &str = "abuse.data_export_cooldown_hours";

/// Presign lifetime of bucket mounts (BYOS-SCP-002).
pub const BYOS_PRESIGN_TTL: &str = "byos.presign_ttl_secs";
pub const BYOS_PRESIGN_TTL_CAP: &str = "byos.presign_ttl_cap_secs";
/// Host patterns a bucket endpoint must match (BYOS-FLG-001). Empty: any endpoint that passes the SSRF policy.
pub const BYOS_ENDPOINT_ALLOW: &str = "byos.endpoint_allow";
/// Whether the client must remux before upload (read by the wave-29 media pipeline).
pub const BYOS_REQUIRE_CLIENT_REMUX: &str = "byos.require_client_remux";
/// Who may connect a bucket of their own: `all`, `patrons` or `staff` (empty means `all`).
pub const BYOS_WHO_MAY_MOUNT: &str = "byos.who_may_mount";
/// The accepted values of [`BYOS_WHO_MAY_MOUNT`].
pub const WHO_MAY_MOUNT_VALUES: &[&str] = &["all", "patrons", "staff"];
/// Most host patterns [`BYOS_ENDPOINT_ALLOW`] may list.
pub const ENDPOINT_ALLOW_MAX_PATTERNS: usize = 50;

/// Staff alert settings (ADM-ALR-001, ADM-ALR-003).
pub const ALERTS_EMAIL_OWNER: &str = "alerts.email_to_owner";
pub const ALERTS_DISK_PERCENT: &str = "alerts.disk_percent";
pub const ALERTS_BACKUP_MAX_AGE_HOURS: &str = "alerts.backup_max_age_hours";
pub const OPS_DISK_PATH: &str = "ops.disk_path";
pub const OPS_BACKUP_DIR: &str = "ops.backup_dir";
/// Most objects the daily `storage.measure` walk lists before it stops and flags its figures as a lower bound (RDY-OBS-002).
pub const OPS_STORAGE_MEASURE_MAX_OBJECTS: &str = "ops.storage_measure_max_objects";

/// Live TV: the asset id (as text, empty for none) of the instance slate loop played while a channel is off air
/// (D-266, D-269). `sr-cli tv slate` makes the asset and writes this; the server reads it before the
/// `SR_TV_SLATE_ASSET` environment fallback and follows a change without a restart.
pub const TV_SLATE_ASSET: &str = "tv.slate_asset";

pub const CATALOG: &[SettingDef] = &[
    int(
        ACCOUNT_CAP,
        "limits",
        1,
        1_000_000,
        20_000,
        "Maximum number of active accounts (D-013).",
    ),
    int(
        RATE_IP,
        "limits",
        60,
        60_000,
        3_000,
        "HTTP requests per minute from one address that is not signed in (applies on the next request).",
    ),
    int(
        RATE_SESSION,
        "limits",
        60,
        60_000,
        3_000,
        "HTTP requests per minute from one signed-in browser session (applies on the next request).",
    ),
    int(
        RATE_BOT,
        "limits",
        60,
        60_000,
        3_000,
        "HTTP requests per minute from one bot or API credential (applies on the next request).",
    ),
    int(
        RATE_MESSAGES,
        "limits",
        12,
        600,
        60,
        "Messages one member may send per minute in one channel (applies on the next request).",
    ),
    int(
        RATE_UPLOADS,
        "limits",
        6,
        6_000,
        240,
        "Upload slot requests one member may make per minute in one channel (applies on the next request).",
    ),
    int(
        ABUSE_FRIEND_REQUESTS,
        "abuse",
        1,
        1000,
        60,
        "Friend requests (and blocks) one member may send per hour (applies on the next request).",
    ),
    int(
        ABUSE_DM_OPENS,
        "abuse",
        1,
        1000,
        40,
        "Direct-message and group conversations one member may open per hour (applies on the next request).",
    ),
    int(
        ABUSE_WEBHOOK_EXECUTE,
        "abuse",
        12,
        6000,
        60,
        "Executions per minute of one webhook, counted per webhook token across all callers (applies on the next request).",
    ),
    int(
        ABUSE_OAUTH_AUTHORIZE,
        "abuse",
        1,
        600,
        20,
        "OAuth2 authorize approvals one member (or address) may make per minute (applies on the next request).",
    ),
    int(
        ABUSE_INTERACTION_CALLBACKS,
        "abuse",
        12,
        6000,
        60,
        "Interaction callbacks per minute from one address (applies on the next request).",
    ),
    int(
        ABUSE_WATCH_PARTY_JOINS,
        "abuse",
        1,
        120,
        20,
        "Watch-party joins one member may make per minute (applies on the next request).",
    ),
    int(
        ABUSE_EMOJI_UPLOADS,
        "abuse",
        1,
        1000,
        50,
        "Custom emoji uploads per hour by one member in one server, or by one application (applies on the next request).",
    ),
    int(
        ABUSE_STICKER_UPLOADS,
        "abuse",
        1,
        1000,
        20,
        "Sticker uploads per hour by one member in one server (applies on the next request).",
    ),
    int(
        ABUSE_APPEAL_ATTEMPTS,
        "abuse",
        1,
        1000,
        10,
        "Ban appeals per hour from one address, whatever the account (one appeal per ban besides, D-215).",
    ),
    int(
        ABUSE_STORAGE_TESTS,
        "abuse",
        1,
        1000,
        20,
        "Connection tests of storage mounts per hour by one member (applies on the next request).",
    ),
    int(
        ABUSE_COUNTER_NOTICES,
        "abuse",
        1,
        10000,
        50,
        "Counter-notices one staff member may record per day (each notice takes one besides).",
    ),
    int(
        ABUSE_DATA_EXPORT_COOLDOWN,
        "abuse",
        1,
        720,
        24,
        "Hours a member must wait between data-package requests (one open request at a time besides).",
    ),
    int(
        BYOS_PRESIGN_TTL,
        "storage",
        60,
        86400,
        900,
        "Seconds a presigned read URL of a member's or server's bucket stays valid (never more than the cap below, never longer than the caller's session has left).",
    ),
    int(
        BYOS_PRESIGN_TTL_CAP,
        "storage",
        60,
        86400,
        3600,
        "Longest lifetime in seconds a presigned read URL of a bucket mount may have. The hard ceiling is 86400.",
    ),
    SettingDef {
        key: BYOS_ENDPOINT_ALLOW,
        group: "storage",
        kind: SettingKind::Text,
        min: 0.0,
        max: 2000.0,
        default_int: 0,
        description: "Hosts a bucket endpoint may use, separated by commas or spaces (s3.example.com or *.example.com). Empty: any https endpoint that is reachable from the public internet; http only for localhost and the development Garage.",
    },
    SettingDef {
        key: BYOS_REQUIRE_CLIENT_REMUX,
        group: "storage",
        kind: SettingKind::Boolean,
        min: 0.0,
        max: 1.0,
        default_int: 0,
        description: "Ask the browser to remux a video into a web-playable container before it is uploaded to a bucket (read by the media pipeline of wave 29).",
    },
    SettingDef {
        key: BYOS_WHO_MAY_MOUNT,
        group: "storage",
        kind: SettingKind::Text,
        min: 0.0,
        max: 10.0,
        default_int: 0,
        description: "Who may connect a bucket of their own: all, patrons or staff (empty means all). A server's own buckets follow its Manage Storage grant instead.",
    },
    int(
        "invites.initial_credits",
        "invites",
        0,
        1_000,
        5,
        "Invite credits granted to a new account.",
    ),
    SettingDef {
        key: "invites.credit_price_usd",
        group: "invites",
        kind: SettingKind::Number,
        min: 0.0,
        max: 100_000.0,
        default_int: 20,
        description: "Price of one purchased invite credit, in USD.",
    },
    int(
        "invites.min_account_age_days",
        "invites",
        0,
        3_650,
        7,
        "Account age required before creating invites.",
    ),
    int(
        SUSPICION_FREEZE,
        "invites",
        1,
        100,
        2,
        "Suspicion level at which an inviter's invite creation is frozen.",
    ),
    int(
        SUSPICION_ESCALATE,
        "invites",
        1,
        100,
        3,
        "Suspicion level that escalates the alert to senior staff.",
    ),
    int(
        "uploads.standard_limit_bytes",
        "limits",
        1_024,
        107_374_182_400,
        31_457_280,
        "Standard attachment size limit in bytes.",
    ),
    int(
        "submissions.daily_upload_bytes",
        "limits",
        0,
        10_995_116_277_760,
        53_687_091_200,
        "Bytes a member may upload to submissions per UTC day (default 50 GiB, 0 = unlimited; staff are exempt).",
    ),
    int(
        "submissions.max_open_per_member",
        "limits",
        1,
        100,
        5,
        "Open submissions a member may have at once (staff are exempt, D-082).",
    ),
    int(
        "messages.max_length",
        "limits",
        1,
        100_000,
        4_000,
        "Maximum message length in characters.",
    ),
    int(
        "moderation.audit_retention_days",
        "retention",
        0,
        36_500,
        45,
        "Guild audit-log retention in days (0 = forever).",
    ),
    int(
        "moderation.reports_per_day",
        "limits",
        1,
        1_000,
        20,
        "Reports one account may file per day (spec 3.3; one open report per target besides).",
    ),
    int(
        "moderation.hold_review_days",
        "retention",
        1,
        3_650,
        90,
        "Days after which a moderation hold is flagged for review. Holds never expire by themselves (D-212).",
    ),
    int(
        "moderation.shared_ip_max_persons",
        "retention",
        2,
        50,
        4,
        "Most people who may share a sign-in address before it counts as a shared network and raises no evasion signal (D-346).",
    ),
    int(
        "moderation.new_account_after_ban_days",
        "retention",
        1,
        90,
        14,
        "Days after a ban within which a new account counts as \"soon after a ban\" for evasion signals (D-346).",
    ),
    int(
        "moderation.case_reopen_days",
        "retention",
        0,
        3_650,
        30,
        "Days a closed case can be reopened; later, a new case links to the old one (spec 4.2).",
    ),
    int(
        "retention.deleted_message_days",
        "retention",
        1,
        365,
        30,
        "Days a deleted message stays readable to high-level staff.",
    ),
    SettingDef {
        key: "library.ingest.rar_enabled",
        group: "library",
        kind: SettingKind::Boolean,
        min: 0.0,
        max: 1.0,
        default_int: 1,
        description: "Accept RAR/CBR uploads in Library ingest (decoded by the UnRAR library).",
    },
    int(
        "library.ingest.max_upload_mb",
        "library",
        1,
        4_096,
        512,
        "Largest Library chapter upload (archive, EPUB or PDF) in MiB.",
    ),
    SettingDef {
        key: "review.lookup.enabled",
        group: "library",
        kind: SettingKind::Boolean,
        min: 0.0,
        max: 1.0,
        default_int: 0,
        description: "Offer \"Fetch details\" in the submission editor (metadata lookup at the providers the operator configured in [lookup]).",
    },
    int(
        "cinema.upload.max_gb",
        "cinema",
        1,
        2_000,
        60,
        "Largest Cinema original a publisher may upload, in GiB.",
    ),
    int(
        "retention.large_file_days",
        "retention",
        1,
        365,
        7,
        "Days large attachments are kept.",
    ),
    SettingDef {
        key: ALERTS_EMAIL_OWNER,
        group: "alerts",
        kind: SettingKind::Boolean,
        min: 0.0,
        max: 1.0,
        default_int: 0,
        description: "Also email every staff alert to the Owner (at most one mail per kind every 30 minutes).",
    },
    int(
        ALERTS_DISK_PERCENT,
        "alerts",
        50,
        99,
        80,
        "Raise a staff alert when the disk holding the data is this percent full.",
    ),
    int(
        ALERTS_BACKUP_MAX_AGE_HOURS,
        "alerts",
        1,
        720,
        36,
        "Raise a staff alert when the newest backup is older than this many hours.",
    ),
    int(
        OPS_STORAGE_MEASURE_MAX_OBJECTS,
        "limits",
        1_000,
        5_000_000,
        200_000,
        "Most objects the daily storage measurement lists before it stops (the figures are then marked as a lower bound). The walk also stops after 60 seconds.",
    ),
    SettingDef {
        key: OPS_DISK_PATH,
        group: "alerts",
        kind: SettingKind::Text,
        min: 0.0,
        max: 500.0,
        default_int: 0,
        description: "Directory whose disk is watched for the disk alert (empty: the server's working directory).",
    },
    SettingDef {
        key: OPS_BACKUP_DIR,
        group: "alerts",
        kind: SettingKind::Text,
        min: 0.0,
        max: 500.0,
        default_int: 0,
        description: "Backup directory (SR_BACKUP_DIR of deploy/scripts/backup.sh); empty turns the backup alert off.",
    },
    SettingDef {
        key: TV_SLATE_ASSET,
        group: "tv",
        kind: SettingKind::Text,
        min: 0.0,
        max: 20.0,
        default_int: 0,
        description: "Asset id of the Live TV slate loop played while a channel is off air (made by sr-cli tv slate; empty: none). To remove it, prefer `sr-cli tv slate --remove`, which also retires the asset.",
    },
];

/// Definition of `key`, if it is an editable setting.
pub fn find(key: &str) -> Option<&'static SettingDef> {
    CATALOG.iter().find(|d| d.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_range_and_kind() {
        let cap = find(ACCOUNT_CAP).unwrap();
        assert_eq!(cap.validate(&json!(100)).unwrap(), json!(100));
        assert!(cap.validate(&json!(0)).is_err());
        assert!(cap.validate(&json!("100")).is_err());
        assert!(cap.validate(&json!(1.5)).is_err());
        let price = find("invites.credit_price_usd").unwrap();
        assert_eq!(price.validate(&json!(12.5)).unwrap(), json!(12.5));
        assert!(price.validate(&json!(-1)).is_err());
    }

    #[test]
    fn rate_limits_have_a_floor_and_a_ceiling() {
        for (key, floor, ceiling, default) in [
            (RATE_IP, 60, 60_000, 3_000),
            (RATE_SESSION, 60, 60_000, 3_000),
            (RATE_BOT, 60, 60_000, 3_000),
            (RATE_MESSAGES, 12, 600, 60),
            (RATE_UPLOADS, 6, 6_000, 240),
        ] {
            let d = find(key).unwrap();
            assert_eq!(d.default_value(), json!(default), "{key}");
            assert!(d.validate(&json!(floor)).is_ok(), "{key}");
            assert!(d.validate(&json!(ceiling)).is_ok(), "{key}");
            assert!(d.validate(&json!(floor - 1)).is_err(), "{key}");
            assert!(d.validate(&json!(ceiling + 1)).is_err(), "{key}");
            assert!(d.validate(&json!(0)).is_err(), "{key}");
        }
    }

    #[test]
    fn abuse_limits_are_in_their_own_group_with_a_floor_of_one() {
        let abuse: Vec<_> = CATALOG.iter().filter(|d| d.group == "abuse").collect();
        assert_eq!(abuse.len(), 12);
        for d in abuse {
            assert!(d.key.starts_with("abuse."), "{}", d.key);
            assert!(d.min >= 1.0 && d.max > d.min, "{}", d.key);
            assert!(d.validate(&json!(0)).is_err(), "{}", d.key);
            assert!(d.validate(&d.default_value()).is_ok(), "{}", d.key);
        }
    }

    #[test]
    fn the_tv_slate_setting_is_an_id_or_empty() {
        let d = find(TV_SLATE_ASSET).unwrap();
        assert_eq!(d.default_value(), json!(""));
        assert_eq!(
            d.validate(&json!("1234567890123")).unwrap(),
            json!("1234567890123")
        );
        assert!(d.validate(&json!("")).is_ok());
        assert!(d.validate(&json!("12ab")).is_err());
        assert!(d.validate(&json!(12)).is_err());
    }

    #[test]
    fn byos_settings_are_in_the_storage_group_with_safe_defaults() {
        for key in [
            BYOS_PRESIGN_TTL,
            BYOS_PRESIGN_TTL_CAP,
            BYOS_ENDPOINT_ALLOW,
            BYOS_REQUIRE_CLIENT_REMUX,
            BYOS_WHO_MAY_MOUNT,
        ] {
            let d = find(key).unwrap_or_else(|| panic!("{key} missing"));
            assert_eq!(d.group, "storage", "{key}");
            assert!(d.validate(&d.default_value()).is_ok(), "{key}");
        }
        // Defaults: everyone may mount, any safe endpoint, no remux demand.
        assert_eq!(find(BYOS_WHO_MAY_MOUNT).unwrap().default_value(), json!(""));
        assert_eq!(find(BYOS_ENDPOINT_ALLOW).unwrap().default_value(), json!(""));
        assert_eq!(
            find(BYOS_REQUIRE_CLIENT_REMUX).unwrap().default_value(),
            json!(false)
        );
    }

    #[test]
    fn who_may_mount_takes_only_the_three_words() {
        let d = find(BYOS_WHO_MAY_MOUNT).unwrap();
        for ok in ["", "all", "patrons", "staff"] {
            assert!(d.validate(&json!(ok)).is_ok(), "{ok}");
        }
        for bad in ["everyone", "Staff ", "admins", "1"] {
            assert!(d.validate(&json!(bad)).is_err(), "{bad}");
        }
        assert!(d.validate(&json!(true)).is_err());
    }

    #[test]
    fn endpoint_allow_takes_hosts_and_suffix_patterns() {
        let d = find(BYOS_ENDPOINT_ALLOW).unwrap();
        assert!(d.validate(&json!("")).is_ok());
        assert!(
            d.validate(&json!("s3.example.com, *.r2.cloudflarestorage.com\nlocalhost"))
                .is_ok()
        );
        for bad in ["a*.example.com", "*", "bad host!", "exa$mple.com", "*.a.*.b"] {
            assert!(d.validate(&json!(bad)).is_err(), "{bad}");
        }
        let many = (0..=ENDPOINT_ALLOW_MAX_PATTERNS)
            .map(|i| format!("h{i}.example.com"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(d.validate(&json!(many)).is_err());
    }

    #[test]
    fn keys_are_unique_and_defaults_valid() {
        let mut seen = std::collections::HashSet::new();
        for d in CATALOG {
            assert!(seen.insert(d.key), "duplicate {}", d.key);
            assert!(d.validate(&d.default_value()).is_ok(), "{}", d.key);
        }
    }
}
