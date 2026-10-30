//! The result table.

use serde::Serialize;

/// Outcome of one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Warn,
    Fail,
}

impl Status {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }
}

/// The ids of the rows [`crate::run`] returns, in table order (D-356). They are stable: clients, scripts
/// (`deploy/scripts/prod-smoke.sh`) and a future title catalog (`admin.preflight.row.<id>`) key on them, so a
/// rename is a deliberate, breaking change (the `check_ids_are_stable` test pins the set).
pub const CHECK_IDS: &[&str] = &[
    "database",
    "migrations",
    "clock",
    "storage",
    "smtp",
    "public-url",
    "turn",
    "vapid",
    "disk",
    "secrets",
    "esplora",
    "voice",
    "web",
];

/// The extra rows of `sr-cli preflight`: `--send-to` adds `mail-send`, `--alert-test` adds `alert-card` and
/// `alert-mail`. Stable in the same way as [`CHECK_IDS`].
pub const CLI_EXTRA_IDS: &[&str] = &["mail-send", "alert-card", "alert-mail"];

/// One row of the table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    /// Stable id, one of [`CHECK_IDS`] or [`CLI_EXTRA_IDS`] (D-356).
    pub id: String,
    /// English operator text, not localized (D-356).
    pub title: String,
    pub status: Status,
    /// English operator text, not localized (D-356): one sentence, what was seen or what to do about it. Never
    /// contains a secret.
    pub detail: String,
}

impl Check {
    pub fn new(id: &str, title: &str, status: Status, detail: impl Into<String>) -> Self {
        Self {
            id: id.to_owned(),
            title: title.to_owned(),
            status,
            detail: detail.into(),
        }
    }
}

/// All rows of one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub checks: Vec<Check>,
    /// Unix milliseconds when the run finished.
    pub ran_at_ms: i64,
    pub passed: u32,
    pub warned: u32,
    pub failed: u32,
    /// `true` when nothing failed (warnings do not fail a preflight).
    pub ok: bool,
}

impl Report {
    pub fn new(checks: Vec<Check>, ran_at_ms: i64) -> Self {
        let count = |s: Status| checks.iter().filter(|c| c.status == s).count() as u32;
        let (passed, warned, failed) = (
            count(Status::Pass),
            count(Status::Warn),
            count(Status::Fail),
        );
        Self {
            checks,
            ran_at_ms,
            passed,
            warned,
            failed,
            ok: failed == 0,
        }
    }

    /// The table `sr-cli preflight` prints (plain text, one row per check).
    pub fn table(&self) -> String {
        let width = self
            .checks
            .iter()
            .map(|c| c.title.chars().count())
            .max()
            .unwrap_or(0);
        let mut out = String::new();
        for c in &self.checks {
            out.push_str(&format!(
                "[{}]  {:<width$}  {}\n",
                c.status.label(),
                c.title,
                c.detail
            ));
        }
        out.push_str(&format!(
            "\n{} passed, {} warning(s), {} failed\n",
            self.passed, self.warned, self.failed
        ));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_exit_state() {
        let r = Report::new(
            vec![
                Check::new("a", "Alpha", Status::Pass, "fine"),
                Check::new("b", "Beta", Status::Warn, "meh"),
            ],
            0,
        );
        assert!(r.ok);
        assert_eq!((r.passed, r.warned, r.failed), (1, 1, 0));
        let t = r.table();
        assert!(t.contains("[PASS]  Alpha  fine"));
        assert!(t.contains("[WARN]  Beta   meh"));
        assert!(t.contains("1 passed, 1 warning(s), 0 failed"));
        let r = Report::new(vec![Check::new("c", "C", Status::Fail, "bad")], 0);
        assert!(!r.ok);
    }
}
