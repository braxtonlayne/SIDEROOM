//! Browser bindings for the permission computation in `sr-core` (built with
//! `default-features = false`, so no figment/config code). Permission values and ids are
//! decimal strings, like the API (bitfields use bit 56+, beyond `Number.MAX_SAFE_INTEGER`).

use serde::Deserialize;
use sr_core::permissions::{Overwrite, OverwriteKind, compute_base, compute_overwrites};
use sr_core::{Permissions, Snowflake};
use wasm_bindgen::prelude::*;

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
    fn high_bits_survive() {
        let big = (1u64 << 59).to_string();
        assert!(has(&big, &big).expect("ok"));
    }
}
