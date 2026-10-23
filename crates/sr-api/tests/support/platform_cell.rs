//! The `PLATFORM` cell grammar of `route_matrix.tsv` (D-360, D-366), shared by
//! `crates/sr-api/tests/route_matrix.rs` and `crates/sr-server/tests/authz_matrix.rs` through a
//! `#[path]` module (no crate dependency between the two test suites).
//!
//! A cell is one name (`NAME`, `STAFF_BASIC`, `OWNER`), or two or more distinct
//! `PlatformPermissions` constant names joined by a single operator used throughout the cell:
//! `+` (every listed bit is required) or `|` (any one listed bit suffices). A cell never mixes
//! `+` and `|`, never repeats a name, has no empty part, and never places `OWNER` or
//! `STAFF_BASIC` inside a compound.

#![allow(dead_code)] // each including suite uses its own part of the API

/// A parsed `PLATFORM` cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Platform {
    /// One permission name: a bit, `STAFF_BASIC` (any appointment) or `OWNER`.
    One(String),
    /// `A+B+...`: every listed bit is required.
    All(Vec<String>),
    /// `A|B|...`: any one listed bit suffices.
    Any(Vec<String>),
}

impl Platform {
    /// The names the cell lists, in order.
    pub fn names(&self) -> Vec<&str> {
        match self {
            Platform::One(n) => vec![n.as_str()],
            Platform::All(v) | Platform::Any(v) => v.iter().map(String::as_str).collect(),
        }
    }

    /// Whether a holder of `held` passes this cell. `OWNER` passes every cell; `STAFF_BASIC` is
    /// held by every appointment.
    pub fn admits(&self, held: &[String]) -> bool {
        let has = |p: &str| held.iter().any(|h| h == p);
        has("OWNER")
            || match self {
                Platform::One(n) => has(n),
                Platform::All(v) => v.iter().all(|n| has(n)),
                Platform::Any(v) => v.iter().any(|n| has(n)),
            }
    }
}

/// Parses a `PLATFORM` cell. `known` says whether a name is a `PlatformPermissions` constant;
/// the error text names the fault so the caller can prefix it with the row.
pub fn parse_platform(cell: &str, known: impl Fn(&str) -> bool) -> Result<Platform, String> {
    let all = cell.contains('+');
    let any = cell.contains('|');
    if all && any {
        return Err(format!(
            "PLATFORM {cell:?} mixes `+` and `|` (one operator per cell, D-366)"
        ));
    }
    let parts: Vec<&str> = if all {
        cell.split('+').collect()
    } else if any {
        cell.split('|').collect()
    } else {
        vec![cell]
    };
    let mut seen: Vec<&str> = Vec::new();
    for p in &parts {
        if p.is_empty() {
            return Err(format!("PLATFORM {cell:?} has an empty part"));
        }
        if !known(p) {
            return Err(format!("unknown PLATFORM permission {p}"));
        }
        if parts.len() > 1 && (*p == "OWNER" || *p == "STAFF_BASIC") {
            return Err(format!(
                "PLATFORM {cell:?}: {p} stands alone, never inside a compound"
            ));
        }
        if seen.contains(p) {
            return Err(format!("PLATFORM {cell:?} repeats {p}"));
        }
        seen.push(p);
    }
    let names: Vec<String> = parts.iter().map(|p| (*p).to_owned()).collect();
    Ok(if all {
        Platform::All(names)
    } else if any {
        Platform::Any(names)
    } else {
        Platform::One(cell.to_owned())
    })
}

/// `known` for `parse_platform`: a `PlatformPermissions` constant name.
pub fn is_platform_name(name: &str) -> bool {
    sr_core::PlatformPermissions::from_name(name).is_some()
}
