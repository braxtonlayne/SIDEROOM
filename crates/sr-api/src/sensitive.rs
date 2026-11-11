//! The one list of sensitive actions that need a fresh proof of identity (RDY-SEC-010).
//!
//! * [`SENSITIVE_ACTIONS`] is the single source: the gate in [`reauth_gate`], the test that every listed route
//!   enforces it, and the table in doc 07 (section "Sensitive actions") all read it.
//! * A staff action is gated by a second factor checked in the last 15 minutes (`900004 SR_REAUTH_REQUIRED`,
//!   ADM-STF-009). The gate runs as a route layer, so a new row here protects its route without the handler
//!   remembering to ask; the handlers that already call `StaffCtx::require_reauth` keep doing so.
//! * `SR_ADMIN_REQUIRE_REAUTH=0` is the end-to-end escape hatch. [`require_reauth_from_env`] refuses it unless
//!   `server.public_url` is a development host (D-357).

use crate::{AppState, routes::admin::StaffCtx};
use axum::{
    extract::{FromRequestParts, MatchedPath, Request, State},
    http::Method,
    middleware::Next,
    response::{IntoResponse, Response},
};
use sr_core::config::is_development_host;

/// What proves the caller is who they say they are, right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// A staff session with a second factor proved in the last 15 minutes. Enforced by [`reauth_gate`].
    StaffSecondFactor,
    /// The account password in the request body (the developer portal's secret and token resets).
    AccountPassword,
    /// Not an HTTP route: the operator on the host, with the config and database in hand (`sr-cli`).
    HostOperator,
}

/// One row of the list.
#[derive(Debug, Clone, Copy)]
pub struct SensitiveAction {
    /// Stable id.
    pub id: &'static str,
    /// The kind of action in plain words (doc 07 groups by it).
    pub kind: &'static str,
    pub gate: Gate,
    /// `""` for a host command.
    pub method: &'static str,
    /// The route template as `route_matrix.tsv` spells it, or the command line for a host command.
    pub path: &'static str,
    /// What the row does, one sentence.
    pub what: &'static str,
}

const fn row(
    id: &'static str,
    kind: &'static str,
    gate: Gate,
    method: &'static str,
    path: &'static str,
    what: &'static str,
) -> SensitiveAction {
    SensitiveAction {
        id,
        kind,
        gate,
        method,
        path,
        what,
    }
}

use Gate::{AccountPassword, HostOperator, StaffSecondFactor as Staff};

/// Every action that needs a fresh second factor or password, grouped by `kind`; the doc 07 table is this list
/// in this order (`cargo test -p sr-api sensitive::tests::print_table -- --nocapture` prints it).
pub const SENSITIVE_ACTIONS: &[SensitiveAction] = &[
    row(
        "staff.appoint",
        "Staff appointment",
        Staff,
        "POST",
        "/admin/staff",
        "Appoint a staff member",
    ),
    row(
        "staff.roles",
        "Staff appointment",
        Staff,
        "PUT",
        "/admin/staff/{account_id}/roles",
        "Change a staff member's roles",
    ),
    row(
        "staff.revoke",
        "Staff appointment",
        Staff,
        "DELETE",
        "/admin/staff/{account_id}",
        "Remove a staff member",
    ),
    row(
        "staff.offer",
        "Staff appointment",
        Staff,
        "POST",
        "/admin/staff/offers",
        "Offer an appointment",
    ),
    row(
        "staff.delegate",
        "Staff appointment",
        Staff,
        "PUT",
        "/admin/staff/{account_id}/delegation",
        "Delegate a permission",
    ),
    row(
        "ban.account",
        "Platform ban",
        Staff,
        "POST",
        "/admin/accounts/{account_id}/ban",
        "Platform-ban the Person behind an account",
    ),
    row(
        "ban.person",
        "Platform ban",
        Staff,
        "POST",
        "/admin/persons/{person_id}/ban",
        "Platform-ban a Person",
    ),
    row(
        "ban.case",
        "Platform ban",
        Staff,
        "POST",
        "/moderation/cases/{case_id}/ban",
        "Platform-ban the Person a case is about",
    ),
    // SEC-AUD-022: the graduated-moderation bans reach the same platform ban.
    row(
        "ban.member",
        "Platform ban",
        Staff,
        "POST",
        "/admin/moderation/members/{account_id}/ban",
        "Platform-ban a member from the graduated moderation screen",
    ),
    row(
        "ban.node",
        "Platform ban",
        Staff,
        "POST",
        "/admin/moderation/subtree-cases/{case_id}/nodes/{account_id}/ban",
        "Platform-ban one account of an invite subtree case",
    ),
    // SEC-AUD-042 (D-574): every staff route that places, changes or ends a member restriction, suspension or
    // perk. An overturned appeal is an unban, a lift is an unfreeze, a revoke is a grant in reverse.
    row(
        "restrict.warn",
        "Member restriction",
        Staff,
        "POST",
        "/admin/moderation/members/{account_id}/warn",
        "Warn a member (a step on the graduated ladder)",
    ),
    row(
        "restrict.timeout",
        "Member restriction",
        Staff,
        "POST",
        "/admin/moderation/members/{account_id}/timeout",
        "Time a member out across the platform",
    ),
    row(
        "restrict.invite_freeze",
        "Member restriction",
        Staff,
        "POST",
        "/admin/moderation/members/{account_id}/invite-freeze",
        "Freeze a member's invites",
    ),
    row(
        "restrict.revoke_invites",
        "Member restriction",
        Staff,
        "POST",
        "/admin/moderation/members/{account_id}/revoke-invites",
        "Revoke every open invite a member made",
    ),
    row(
        "restrict.lift",
        "Member restriction",
        Staff,
        "POST",
        "/admin/moderation/restrictions/{restriction_id}/lift",
        "Lift a member restriction early",
    ),
    row(
        "restrict.node_freeze",
        "Member restriction",
        Staff,
        "POST",
        "/admin/moderation/subtree-cases/{case_id}/nodes/{account_id}/freeze",
        "Freeze one account of an invite subtree case",
    ),
    row(
        "restrict.node_clear",
        "Member restriction",
        Staff,
        "POST",
        "/admin/moderation/subtree-cases/{case_id}/nodes/{account_id}/clear",
        "Clear one account of an invite subtree case",
    ),
    row(
        "restrict.appeal",
        "Member restriction",
        Staff,
        "POST",
        "/admin/moderation/restrictions/{restriction_id}/appeal-decision",
        "Uphold or overturn a restriction appeal",
    ),
    row(
        "ban.appeal",
        "Member restriction",
        Staff,
        "POST",
        "/moderation/appeals/{appeal_id}/decision",
        "Uphold or overturn a ban appeal (an overturn unbans)",
    ),
    // SEC-AUD-051 (D-581): the case warning is the member warning from another screen, the data-request
    // restriction places and ends a member's hiding, an application's suspension stops its owner's bot everywhere.
    row(
        "restrict.case_warn",
        "Member restriction",
        Staff,
        "POST",
        "/moderation/cases/{case_id}/warn",
        "Warn the member a case is about",
    ),
    row(
        "restrict.data_hide",
        "Member restriction",
        Staff,
        "POST",
        "/admin/data-requests/{request_id}/restriction",
        "Hide or show a member's shared activity while their data request is reviewed",
    ),
    row(
        "app.suspend",
        "Application suspension",
        Staff,
        "POST",
        "/admin/developers/apps/{application_id}/suspend",
        "Switch an application off",
    ),
    row(
        "app.unsuspend",
        "Application suspension",
        Staff,
        "POST",
        "/admin/developers/apps/{application_id}/unsuspend",
        "Lift an application's suspension",
    ),
    row(
        "suspend.account",
        "Account suspension",
        Staff,
        "POST",
        "/admin/accounts/{account_id}/suspend",
        "Suspend an account",
    ),
    row(
        "suspend.lift",
        "Account suspension",
        Staff,
        "POST",
        "/admin/accounts/{account_id}/unsuspend",
        "Lift an account suspension",
    ),
    row(
        "age.underage",
        "Age tools",
        Staff,
        "POST",
        "/moderation/age/underage",
        "Lock an account's age confirmation (and suspend one under the minimum age)",
    ),
    row(
        "age.lift",
        "Age tools",
        Staff,
        "POST",
        "/moderation/age/underage/{account_id}/lift",
        "Lift an underage lock",
    ),
    row(
        "age.restrictions",
        "Age tools",
        Staff,
        "POST",
        "/moderation/age/restrictions",
        "Mark or unmark a channel, series or title as age-restricted",
    ),
    row(
        "data.run",
        "Data-request outcome",
        Staff,
        "POST",
        "/admin/data-requests/{request_id}/run",
        "Run an export, erasure or other data request",
    ),
    row(
        "data.decline",
        "Data-request outcome",
        Staff,
        "POST",
        "/admin/data-requests/{request_id}/decline",
        "Decline a data request",
    ),
    row(
        "takedown.uphold",
        "Takedown uphold",
        Staff,
        "POST",
        "/moderation/takedowns/{notice_id}/uphold",
        "Uphold a takedown notice",
    ),
    // SEC-AUD-022: delisting takes a member's bucket, or a file on it, offline for everyone.
    row(
        "storage.delist",
        "Takedown uphold",
        Staff,
        "POST",
        "/admin/storage/mounts/{mount_id}/delist",
        "Delist a member's whole storage connection",
    ),
    row(
        "storage.relist",
        "Takedown uphold",
        Staff,
        "POST",
        "/admin/storage/mounts/{mount_id}/relist",
        "Relist a delisted storage connection",
    ),
    row(
        "shelf.delist",
        "Takedown uphold",
        Staff,
        "POST",
        "/admin/shelves/delist",
        "Delist one file on a member's shelf",
    ),
    row(
        "shelf.relist",
        "Takedown uphold",
        Staff,
        "POST",
        "/admin/shelves/relist",
        "Relist a delisted shelf file",
    ),
    row(
        "secret.rotate",
        "Secret rotation",
        HostOperator,
        "",
        "sr-cli rotate-secret-key",
        "Re-seal every stored secret under a new server.secret_key",
    ),
    row(
        "owner.transfer",
        "Owner transfer",
        Staff,
        "POST",
        "/admin/guilds/{guild_id}/transfer-ownership",
        "Make another member the owner of a server",
    ),
    row(
        "app.secret",
        "Application secret reset",
        AccountPassword,
        "POST",
        "/applications/{application_id}/reset-secret",
        "Replace an application's client secret",
    ),
    row(
        "app.token",
        "Application secret reset",
        AccountPassword,
        "POST",
        "/applications/{application_id}/bot/reset-token",
        "Replace a bot token",
    ),
    row(
        "payout.wallet",
        "Payout descriptor change",
        Staff,
        "PUT",
        "/admin/donations/wallet",
        "Set the wallet that receives donations",
    ),
    // SEC-AUD-022: a staff grant of a perk, including to the staff member's own account.
    row(
        "entitlement.grant",
        "Perk grant",
        Staff,
        "POST",
        "/admin/donations/entitlements",
        "Grant a Patron or server perk by hand (never to the caller's own account, or a server they own, D-574 and D-581)",
    ),
    row(
        "entitlement.revoke",
        "Perk grant",
        Staff,
        "DELETE",
        "/admin/donations/entitlements/{grant_id}",
        "Revoke a perk grant",
    ),
    row(
        "account.reset2fa",
        "Account takeover surface",
        Staff,
        "POST",
        "/admin/accounts/{account_id}/reset-2fa",
        "Reset an account's second factor",
    ),
    row(
        "account.email",
        "Account takeover surface",
        Staff,
        "POST",
        "/admin/accounts/{account_id}/change-email",
        "Change an account's email",
    ),
    row(
        "account.password",
        "Account takeover surface",
        Staff,
        "POST",
        "/admin/accounts/{account_id}/force-password-reset",
        "Force a password reset",
    ),
    row(
        "person.move",
        "Account takeover surface",
        Staff,
        "POST",
        "/admin/persons/{person_id}/identities/{identity_id}/move",
        "Move an identity between Persons",
    ),
    // SEC-AUD-022: a username is how members recognise each other (impersonation), badges are public marks.
    row(
        "account.rename",
        "Account takeover surface",
        Staff,
        "POST",
        "/admin/accounts/{account_id}/rename",
        "Rename an account",
    ),
    row(
        "account.badges",
        "Account takeover surface",
        Staff,
        "POST",
        "/admin/accounts/{account_id}/badges",
        "Grant or revoke public badges",
    ),
    row(
        "announce.create",
        "Announcements and entrance",
        Staff,
        "POST",
        "/admin/announcements",
        "Broadcast an announcement",
    ),
    row(
        "announce.edit",
        "Announcements and entrance",
        Staff,
        "PATCH",
        "/admin/announcements/{announcement_id}",
        "Edit an announcement",
    ),
    row(
        "entrance.copy.put",
        "Announcements and entrance",
        Staff,
        "PUT",
        "/admin/entrance/copy/{key}",
        "Change the public entrance copy",
    ),
    row(
        "entrance.copy.delete",
        "Announcements and entrance",
        Staff,
        "DELETE",
        "/admin/entrance/copy/{key}",
        "Reset the public entrance copy",
    ),
    row(
        "entrance.hero.put",
        "Announcements and entrance",
        Staff,
        "PUT",
        "/admin/entrance/hero",
        "Change the entrance hero image",
    ),
    row(
        "entrance.hero.delete",
        "Announcements and entrance",
        Staff,
        "DELETE",
        "/admin/entrance/hero",
        "Remove the entrance hero image",
    ),
    row(
        "system.mail",
        "Announcements and entrance",
        Staff,
        "POST",
        "/admin/system/email-test",
        "Send the operator test mail",
    ),
];

/// The route layer: a listed staff route needs the second-factor window before its handler runs. Anything else
/// passes through untouched. A caller who is not staff gets the refusal the handler would give.
pub async fn reauth_gate(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let Some(path) = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| unversioned(m.as_str()).to_owned())
    else {
        return next.run(req).await;
    };
    if !gated(req.method(), &path) {
        return next.run(req).await;
    }
    let (mut parts, body) = req.into_parts();
    match StaffCtx::from_request_parts(&mut parts, &state).await {
        Ok(ctx) => {
            if let Err(e) = ctx.require_reauth().await {
                return e.into_response();
            }
        }
        Err(e) => return e.into_response(),
    }
    next.run(Request::from_parts(parts, body)).await
}

fn unversioned(path: &str) -> &str {
    path.strip_prefix("/api/v10")
        .or_else(|| path.strip_prefix("/api/v1"))
        .unwrap_or(path)
}

/// Is `method path` a listed route the layer gates with a second factor?
pub fn gated(method: &Method, path: &str) -> bool {
    SENSITIVE_ACTIONS
        .iter()
        .any(|a| a.gate == Gate::StaffSecondFactor && a.method == method.as_str() && a.path == path)
}

/// Read `SR_ADMIN_REQUIRE_REAUTH`: `0`, `false` or `off` turns the second-factor window off, and is an error
/// unless `public_url` is a development host. The server refuses to start on that error, so a variable left in
/// `server.env` cannot weaken a real host.
pub fn require_reauth_from_env(value: Option<&str>, public_url: &str) -> Result<bool, String> {
    let off = matches!(value.map(str::trim), Some("0" | "false" | "off"));
    if off && !is_development_host(public_url) {
        return Err(
            "SR_ADMIN_REQUIRE_REAUTH=0 is only honoured when server.public_url is a localhost address; unset it on this host".to_owned()
        );
    }
    Ok(!off)
}

/// The table doc 07 carries, as Markdown rows.
pub fn markdown_table() -> String {
    let mut out =
        String::from("| Action | Gate | Route or command | Effect |\n|---|---|---|---|\n");
    for a in SENSITIVE_ACTIONS {
        let gate = match a.gate {
            Gate::StaffSecondFactor => "second factor, last 15 minutes",
            Gate::AccountPassword => "account password",
            Gate::HostOperator => "host access",
        };
        let route = if a.method.is_empty() {
            format!("`{}`", a.path)
        } else {
            format!("`{} {}`", a.method, a.path)
        };
        out.push_str(&format!("| {} | {gate} | {route} | {} |\n", a.kind, a.what));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_hatch_is_refused_off_a_development_host() {
        assert!(require_reauth_from_env(None, "https://chat.example.org").unwrap());
        assert!(require_reauth_from_env(Some("1"), "https://chat.example.org").unwrap());
        assert!(!require_reauth_from_env(Some("0"), "http://localhost:5173").unwrap());
        assert!(!require_reauth_from_env(Some("off"), "https://smoke.localhost").unwrap());
        assert!(!require_reauth_from_env(Some("false"), "http://127.0.0.1:8080").unwrap());
        for v in ["0", "false", "off", " 0 "] {
            let e = require_reauth_from_env(Some(v), "https://chat.example.org").unwrap_err();
            assert!(e.contains("SR_ADMIN_REQUIRE_REAUTH"), "{e}");
        }
        assert!(
            require_reauth_from_env(Some("0"), "").is_err(),
            "an unset public_url is not a development host"
        );
    }

    #[test]
    fn rows_are_unique_and_every_route_is_in_the_matrix() {
        let matrix = include_str!("../tests/route_matrix.tsv");
        let mut seen = std::collections::HashSet::new();
        for a in SENSITIVE_ACTIONS {
            assert!(seen.insert(a.id), "duplicate id {}", a.id);
            if a.gate == Gate::HostOperator {
                assert!(a.method.is_empty() && a.path.starts_with("sr-cli "));
                continue;
            }
            let line = matrix
                .lines()
                .find(|l| {
                    let mut c = l.split('\t');
                    c.next() == Some(a.method) && c.next() == Some(a.path)
                })
                .unwrap_or_else(|| panic!("{} {} is not in route_matrix.tsv", a.method, a.path));
            let authz = line.split('\t').nth(3).unwrap_or("");
            assert!(
                matches!(authz, "staff" | "owner"),
                "{} {} is {authz}, not a staff or owner route",
                a.method,
                a.path
            );
        }
        assert!(SENSITIVE_ACTIONS.len() >= 20);
    }

    /// D-574: every staff route that places, changes or ends a member restriction, suspension or perk is gated.
    /// The rule is read off the route matrix by family, so a new write route in one of these families fails here
    /// until it is listed.
    #[test]
    fn every_restriction_suspension_and_perk_write_is_gated() {
        const FAMILIES: &[&str] = &[
            "/admin/moderation/members/{account_id}/",
            "/admin/moderation/restrictions/{restriction_id}/",
            "/admin/moderation/subtree-cases/{case_id}/nodes/{account_id}/",
            "/moderation/appeals/{appeal_id}/",
            "/moderation/age/",
            "/admin/donations/entitlements",
            "/admin/accounts/{account_id}/suspend",
            "/admin/accounts/{account_id}/unsuspend",
            // D-581
            "/moderation/cases/{case_id}/warn",
            "/admin/data-requests/{request_id}/restriction",
            "/admin/developers/apps/{application_id}/suspend",
            "/admin/developers/apps/{application_id}/unsuspend",
        ];
        let matrix = include_str!("../tests/route_matrix.tsv");
        let mut checked = 0;
        for line in matrix.lines().filter(|l| !l.starts_with('#')) {
            let mut c = line.split('\t');
            let (Some(method), Some(path)) = (c.next(), c.next()) else {
                continue;
            };
            if method == "GET" || !FAMILIES.iter().any(|f| path.starts_with(f)) {
                continue;
            }
            let m = Method::from_bytes(method.as_bytes()).unwrap();
            assert!(
                gated(&m, path),
                "{method} {path} needs the second factor (D-574)"
            );
            checked += 1;
        }
        assert!(
            checked >= 21,
            "only {checked} routes in the D-574 and D-581 families"
        );
    }

    #[test]
    fn print_table() {
        println!("{}", markdown_table());
    }
}
