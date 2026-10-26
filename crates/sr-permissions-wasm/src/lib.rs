//! Browser bindings for the permission computation in `sr-core` (built with
//! `default-features = false`, so no figment/config code). Permission values and ids are
//! decimal strings, like the API (bitfields use bit 56+, beyond `Number.MAX_SAFE_INTEGER`).

use serde::Deserialize;
use sr_core::permissions::{
    ComputeInput, Overwrite, OverwriteKind, compute, compute_base, compute_overwrites, is_timed_out,
};
use sr_core::{Clock, Permissions, Snowflake};
use wasm_bindgen::prelude::*;

mod clock;
pub use clock::JsClock;

fn perms(s: &str) -> Result<Permissions, String> {
    s.trim()
        .parse::<u64>()
        .map(Permissions::from_bits_truncate)
        .map_err(|_| format!("invalid permissions value `{s}`"))
}

fn snow(s: &str) -> Result<Snowflake, String> {
    s.trim()
        .parse::<Snowflake>()
        .map_err(|_| format!("invalid id `{s}`"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OverwriteIn {
    id: String,
    /// `"role"` or `"member"`.
    kind: String,
    allow: String,
    deny: String,
}

/// Guild-level permissions (owner and `ADMINISTRATOR` bypass). Decimal strings in and out.
pub fn base(is_owner: bool, everyone: &str, roles: &[String]) -> Result<String, String> {
    let role_perms = roles
        .iter()
        .map(|r| perms(r))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(compute_base(is_owner, perms(everyone)?, role_perms)
        .bits()
        .to_string())
}

/// Channel-level permissions. `overwrites_json` is
/// `[{ "id", "kind": "role"|"member", "allow", "deny" }]`, all decimal strings.
pub fn overwrites(
    base_perms: &str,
    guild_id: &str,
    member_id: &str,
    member_role_ids: &[String],
    overwrites_json: &str,
) -> Result<String, String> {
    let raw: Vec<OverwriteIn> = serde_json::from_str(overwrites_json).map_err(|e| e.to_string())?;
    let ows = raw
        .iter()
        .map(|o| {
            Ok(Overwrite {
                id: snow(&o.id)?,
                kind: match o.kind.as_str() {
                    "role" => OverwriteKind::Role,
                    "member" => OverwriteKind::Member,
                    other => return Err(format!("invalid overwrite kind `{other}`")),
                },
                allow: perms(&o.allow)?,
                deny: perms(&o.deny)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let roles = member_role_ids
        .iter()
        .map(|r| snow(r))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(compute_overwrites(
        perms(base_perms)?,
        snow(guild_id)?,
        snow(member_id)?,
        &roles,
        &ows,
    )
    .bits()
    .to_string())
}

/// `ADMINISTRATOR`-aware check that `have` includes every bit of `want`.
pub fn has(have: &str, want: &str) -> Result<bool, String> {
    let (have, want) = (perms(have)?, perms(want)?);
    Ok(have.has(want))
}

/// Guild-level permissions. Throws on malformed input.
#[wasm_bindgen(js_name = computeBase)]
pub fn compute_base_js(
    is_owner: bool,
    everyone: &str,
    role_permissions: Vec<String>,
) -> Result<String, JsError> {
    base(is_owner, everyone, &role_permissions).map_err(|e| JsError::new(&e))
}

/// Channel-level permissions after overwrites. Throws on malformed input.
#[wasm_bindgen(js_name = computeOverwrites)]
pub fn compute_overwrites_js(
    base_permissions: &str,
    guild_id: &str,
    member_id: &str,
    member_role_ids: Vec<String>,
    overwrites_json: &str,
) -> Result<String, JsError> {
    overwrites(
        base_permissions,
        guild_id,
        member_id,
        &member_role_ids,
        overwrites_json,
    )
    .map_err(|e| JsError::new(&e))
}

/// Whether `have` grants all of `want` (administrators bypass).
#[wasm_bindgen(js_name = hasPermission)]
pub fn has_permission(have: &str, want: &str) -> Result<bool, JsError> {
    has(have, want).map_err(|e| JsError::new(&e))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ComputeIn {
    is_owner: bool,
    everyone: String,
    role_permissions: Vec<String>,
    guild_id: String,
    member_id: String,
    member_role_ids: Vec<String>,
    overwrites: Vec<OverwriteIn>,
    /// Unix milliseconds, or null when the member is not timed out.
    communication_disabled_until_ms: Option<i64>,
}

/// Full channel permissions (Discord's overwrite algorithm plus the implicit rules:
/// timeouts, VIEW_CHANNEL, SEND_MESSAGES, CONNECT). `input_json` is a JSON object with
/// `is_owner`, `everyone`, `role_permissions`, `guild_id`, `member_id`, `member_role_ids`,
/// `overwrites` (same shape as [`overwrites`]) and `communication_disabled_until_ms`.
/// `now_ms` is the current unix time in milliseconds.
pub fn compute_channel(input_json: &str, now_ms: i64) -> Result<String, String> {
    let raw: ComputeIn = serde_json::from_str(input_json).map_err(|e| e.to_string())?;
    let role_perms = raw
        .role_permissions
        .iter()
        .map(|r| perms(r))
        .collect::<Result<Vec<_>, _>>()?;
    let roles = raw
        .member_role_ids
        .iter()
        .map(|r| snow(r))
        .collect::<Result<Vec<_>, _>>()?;
    let ows = raw
        .overwrites
        .iter()
        .map(|o| {
            Ok(Overwrite {
                id: snow(&o.id)?,
                kind: match o.kind.as_str() {
                    "role" => OverwriteKind::Role,
                    "member" => OverwriteKind::Member,
                    other => return Err(format!("invalid overwrite kind `{other}`")),
                },
                allow: perms(&o.allow)?,
                deny: perms(&o.deny)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let input = ComputeInput {
        is_owner: raw.is_owner,
        everyone: perms(&raw.everyone)?,
        role_perms: &role_perms,
        guild_id: snow(&raw.guild_id)?,
        member_id: snow(&raw.member_id)?,
        member_role_ids: &roles,
        overwrites: &ows,
        timed_out: is_timed_out(raw.communication_disabled_until_ms, now_ms),
    };
    Ok(compute(&input).bits().to_string())
}

/// Channel-level permissions with timeouts and implicit rules, using the browser clock.
/// Throws on malformed input. Takes and returns decimal strings, like the API.
#[wasm_bindgen(js_name = compute)]
pub fn compute_js(input_json: &str) -> Result<String, JsError> {
    compute_channel(input_json, JsClock.now_unix_ms()).map_err(|e| JsError::new(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEND: u64 = 1 << 11;
    const VIEW: u64 = 1 << 10;
    const ADMIN: u64 = 1 << 3;

    #[test]
    fn base_and_admin() {
        let b = base(false, &VIEW.to_string(), &[SEND.to_string()]).expect("ok");
        assert_eq!(b, (VIEW | SEND).to_string());
        let a = base(false, "0", &[ADMIN.to_string()]).expect("ok");
        assert_eq!(a, Permissions::ALL.bits().to_string());
        assert!(has(&a, &SEND.to_string()).expect("ok"));
        assert!(base(false, "x", &[]).is_err());
    }

    #[test]
    fn everyone_overwrite_denies_send() {
        let json = format!(r#"[{{"id":"10","kind":"role","allow":"0","deny":"{SEND}"}}]"#);
        let out = overwrites(&(VIEW | SEND).to_string(), "10", "99", &[], &json).expect("ok");
        assert_eq!(out, VIEW.to_string());
    }

    #[test]
    fn bad_overwrite_kind_rejected() {
        let json = r#"[{"id":"1","kind":"x","allow":"0","deny":"0"}]"#;
        assert!(overwrites("0", "1", "2", &[], json).is_err());
    }

    #[test]
    fn compute_channel_applies_timeout_with_clock_value() {
        let json = |until: &str| {
            format!(
                r#"{{"is_owner":false,"everyone":"{}","role_permissions":[],
                "guild_id":"10","member_id":"99","member_role_ids":[],"overwrites":[],
                "communication_disabled_until_ms":{until}}}"#,
                VIEW | SEND | (1 << 16)
            )
        };
        // Timed out until 2000, now 1000: only VIEW_CHANNEL | READ_MESSAGE_HISTORY remain.
        let out = compute_channel(&json("2000"), 1_000).expect("ok");
        assert_eq!(out, (VIEW | (1 << 16)).to_string());
        // Timeout expired: full set survives.
        let out = compute_channel(&json("2000"), 3_000).expect("ok");
        assert_eq!(out, (VIEW | SEND | (1 << 16)).to_string());
        assert!(compute_channel(&json("null"), 1_000).is_ok());
        assert!(compute_channel("{}", 1_000).is_err());
    }

    #[test]
    fn high_bits_survive() {
        let big = (1u64 << 59).to_string();
        assert!(has(&big, &big).expect("ok"));
        // Bit 60 (USE_EXPRESSIONS_EXTERNALLY, D-376) is part of ALL and survives the round trip.
        let ext = (1u64 << 60).to_string();
        assert!(has(&ext, &ext).expect("ok"));
        let a = base(false, "0", &[ADMIN.to_string()]).expect("ok");
        assert!(has(&a, &ext).expect("ok"));
        assert_ne!(Permissions::ALL.bits() & (1 << 60), 0);
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    use super::*;
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    wasm_bindgen_test_configure!(run_in_browser);

    /// The `JsError` path is only constructible on the wasm target.
    #[wasm_bindgen_test]
    fn malformed_input_throws_js_error() {
        assert!(has_permission("not-a-number", "1").is_err());
        assert!(compute_base_js(false, "oops", vec![]).is_err());
        assert!(compute_js("{}").is_err());
    }

    #[wasm_bindgen_test]
    fn valid_input_round_trips_through_the_exports() {
        // ADMINISTRATOR (bit 3) bypasses every check.
        assert!(has_permission("8", "2048").unwrap());
        assert!(!has_permission("0", "2048").unwrap());
        assert_eq!(compute_base_js(false, "3", vec![]).unwrap(), "3");
    }
}
