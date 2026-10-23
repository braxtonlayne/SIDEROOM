//! The result table.

use serde::Serialize;
use utoipa::ToSchema;

/// Outcome of one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
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

/// One row of the table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct Check {
    /// Stable id such as `database`, `storage` or `turn`.
    pub id: String,
    pub title: String,
    pub status: Status,
    /// One sentence: what was seen, or what to do about it. Never contains a secret.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
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
