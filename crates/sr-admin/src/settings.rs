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
                if s.chars().count() as f64 > self.max {
                    return Err(format!("must be at most {} characters", self.max));
                }
                Ok(json!(s))
            }
        }
    }
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

/// Staff alert settings (ADM-ALR-001, ADM-ALR-003).
pub const ALERTS_EMAIL_OWNER: &str = "alerts.email_to_owner";
pub const ALERTS_DISK_PERCENT: &str = "alerts.disk_percent";
pub const ALERTS_BACKUP_MAX_AGE_HOURS: &str = "alerts.backup_max_age_hours";
pub const OPS_DISK_PATH: &str = "ops.disk_path";
pub const OPS_BACKUP_DIR: &str = "ops.backup_dir";

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
    fn keys_are_unique_and_defaults_valid() {
        let mut seen = std::collections::HashSet::new();
        for d in CATALOG {
            assert!(seen.insert(d.key), "duplicate {}", d.key);
            assert!(d.validate(&d.default_value()).is_ok(), "{}", d.key);
        }
    }
}
