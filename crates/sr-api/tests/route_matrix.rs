//! XC-SEC-005: the per-route authentication and permission matrix.
//!
//! `route_matrix.tsv` has one row for every operation in the generated OpenAPI document
//! (the full API surface): `METHOD  PATH  ACCESS  AUTHZ  BOT  BEARER  PLATFORM`.
//!
//! * `ACCESS` is `public` (no credentials needed) or `authenticated`. It must agree with the
//!   `security(...)` the handler declares in OpenAPI, and every `authenticated` route must
//!   answer **401** to a request that carries no credentials.
//! * `AUTHZ` says who may act once authenticated (see `AUTHZ_VALUES`): `none` (public),
//!   `user` (any signed-in account, no per-resource check: catalogs and community reads),
//!   `self` (the caller's own data), `author` (only the owner or author of the target
//!   resource), `author_or_staff` (the owner, or a holder of the section's platform
//!   permission), `visibility` (owner, or whoever the resource's visibility and blocks
//!   admit), `membership` (server/channel permission checks in the service layer), `staff`
//!   (platform staff), `owner` (application owner), `bot` (an application's bot token only)
//!   and the Watch Party roles `party_access` (channel view or invite code), `party_member`,
//!   `party_control` (member who satisfies the party's control or queue policy) and
//!   `party_manager` (host or channel manager), `storage_scope` (a bucket mount read decided by `sr_storage::scopes`: own, friend share, guild grant or logged staff read) and `mount_owner` (only the mount's owner). There is no catch-all value: a new route
//!   must say precisely who may call it. A trailing `?` means "seeded by the generator, not
//!   yet reviewed" and fails the test.
//! * `BOT` and `BEARER` pin what `auth::policy` says about bot and OAuth bearer tokens, so a
//!   policy change cannot slip through without touching this file.
//! * `PLATFORM` (D-360, D-366) names the platform permission a `staff` or `author_or_staff` row
//!   demands: one name (`NAME`, `STAFF_BASIC`, `OWNER`), `A+B` (every listed bit) or `A|B` (any
//!   one), never mixed; `-` on every other row. The grammar is `support/platform_cell.rs`.
//!
//! The test fails when a route is added or removed without a row, and when a router path
//! literal is neither documented in OpenAPI nor listed under `#undocumented`.
//! To add rows: `SR_UPDATE_ROUTE_MATRIX=1 cargo test -p sr-api --test route_matrix`, then
//! review every `?` entry by hand and delete the `?`.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use sr_api::{AppState, auth::policy, build_router, routes};
use std::collections::{BTreeMap, BTreeSet};
use tower::ServiceExt;

#[path = "support/platform_cell.rs"]
mod platform_cell;
use platform_cell::{Platform, is_platform_name, parse_platform};

const MATRIX: &str = include_str!("route_matrix.tsv");
const MATRIX_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/route_matrix.tsv");
const METHODS: [&str; 5] = ["get", "post", "put", "patch", "delete"];

/// The AUTHZ vocabulary; `other` is deliberately absent (G-19).
const AUTHZ_VALUES: [&str; 16] = [
    "none",
    "user",
    "self",
    "author",
    "author_or_staff",
    "visibility",
    "membership",
    "staff",
    "owner",
    "bot",
    "party_access",
    "party_member",
    "party_control",
    "party_manager",
    "storage_scope",
    "mount_owner",
];

/// Extractor types that only a platform-staff session satisfies: `StaffCtx` and every struct that
/// carries one (`PublisherCtx`, `MediaCtx`, `StaffTv`, ...), found by reading the sources.
fn staff_extractors(files: &[(std::path::PathBuf, String)]) -> Vec<String> {
    let mut out = vec!["StaffCtx".to_owned()];
    for (_, src) in files {
        let mut rest = src.as_str();
        while let Some(i) = rest.find("struct ") {
            rest = &rest[i + 7..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let Some(open) = rest.find('{') else { break };
            let Some(close) = rest[open..].find('}') else {
                break;
            };
            let body = &rest[open..open + close];
            // A unit or tuple struct has no braces before the next item; skip those.
            let header = &rest[..open];
            if !header.contains(';') && body.contains(": StaffCtx") && !name.is_empty() {
                out.push(name);
            }
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    access: String,
    authz: String,
    bot: String,
    bearer: String,
    /// The platform permission(s) a `staff` or `author_or_staff` row demands: `NAME`, `A+B`, `OWNER`,
    /// `STAFF_BASIC` (any appointment suffices); `-` on every other row (D-360).
    platform: String,
}

/// `(METHOD, path) -> declared access` for every operation in the OpenAPI document.
fn documented() -> BTreeMap<(String, String), String> {
    let doc = serde_json::to_value(routes::openapi()).unwrap();
    let mut out = BTreeMap::new();
    for (path, item) in doc["paths"].as_object().unwrap() {
        for m in METHODS {
            let Some(op) = item.get(m) else { continue };
            // `security: []` or an absent key means no credentials are required.
            let secured = op
                .get("security")
                .and_then(|s| s.as_array())
                .is_some_and(|a| a.iter().any(|req| !req.as_object().unwrap().is_empty()));
            let access = if secured { "authenticated" } else { "public" };
            out.insert((m.to_uppercase(), path.clone()), access.to_owned());
        }
    }
    out
}

fn parse() -> (BTreeMap<(String, String), Row>, BTreeSet<String>) {
    let mut rows = BTreeMap::new();
    let mut undocumented = BTreeSet::new();
    for line in MATRIX.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with("# ") || line == "#" {
            continue;
        }
        if let Some(p) = line.strip_prefix("#undocumented\t") {
            undocumented.insert(p.to_owned());
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 7, "malformed matrix row: {line:?}");
        rows.insert(
            (f[0].to_owned(), f[1].to_owned()),
            Row {
                access: f[2].to_owned(),
                authz: f[3].to_owned(),
                bot: f[4].to_owned(),
                bearer: f[5].to_owned(),
                platform: f[6].to_owned(),
            },
        );
    }
    (rows, undocumented)
}

fn bot_of(method: &str, path: &str) -> &'static str {
    let m = Method::from_bytes(method.as_bytes()).unwrap();
    if policy::bot_denied(&m, path) {
        "deny"
    } else {
        "allow"
    }
}

fn bearer_of(method: &str, path: &str) -> String {
    let m = Method::from_bytes(method.as_bytes()).unwrap();
    match policy::bearer_scope(&m, path) {
        None => "-".to_owned(),
        Some("") => "any".to_owned(),
        Some(s) => s.to_owned(),
    }
}

fn guess_authz(path: &str, access: &str) -> &'static str {
    let p = policy::normalise(path);
    if access == "public" {
        "none"
    } else if p.starts_with("/admin") {
        "staff"
    } else if p.starts_with("/developers") || p.starts_with("/applications") {
        "owner"
    } else if p.contains("{guild_id}")
        || p.contains("{channel_id}")
        || p.contains("{thread_id}")
        || p.contains("{message_id}")
        || p.starts_with("/guilds")
        || p.starts_with("/channels")
        || p.starts_with("/invites")
    {
        "membership"
    } else if p.starts_with("/users/@me") || p.starts_with("/sr/auth") {
        "self"
    } else {
        // Seeded as the weakest claim; the `?` forces a hand review of the handler.
        "user"
    }
}

/// `(METHOD, path) -> the handler's signature text`, read from the `#[utoipa::path]` blocks
/// in the router sources.
fn source_files() -> Vec<(std::path::PathBuf, String)> {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|e| e == "rs")
                && !p.file_name().unwrap().to_string_lossy().contains("test")
            {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    files
        .into_iter()
        .map(|p| {
            let src = std::fs::read_to_string(&p).unwrap();
            (p, src)
        })
        .collect()
}

fn handler_signatures(
    files: &[(std::path::PathBuf, String)],
) -> BTreeMap<(String, String), String> {
    let mut out = BTreeMap::new();
    for (_, src) in files {
        let mut from = 0;
        while let Some(i) = src[from..].find("#[utoipa::path(") {
            let start = from + i;
            from = start + 15;
            let head = &src[from..];
            let head = head.trim_start();
            let Some((method, rest)) = head.split_once(',') else {
                continue;
            };
            let method = method.trim();
            if !METHODS.contains(&method) {
                continue;
            }
            let Some(p) = rest.find("path") else { continue };
            let after = &rest[p + 4..];
            let Some(q1) = after.find('"') else { continue };
            let Some(q2) = after[q1 + 1..].find('"') else {
                continue;
            };
            let path = after[q1 + 1..q1 + 1 + q2].to_owned();
            let Some(fn_at) = src[start..].find("async fn ") else {
                continue;
            };
            let sig_start = start + fn_at;
            let Some(brace) = src[sig_start..].find('{') else {
                continue;
            };
            out.insert(
                (method.to_uppercase(), path),
                src[sig_start..sig_start + brace].to_owned(),
            );
        }
    }
    out
}

/// G-19: the AUTHZ column is checked against what each handler's own signature demands,
/// wherever that is cheap to see: a staff-only extractor means `staff`, a staff extractor on
/// a non-staff row (or the reverse) is a wrong row, and `bot` rows take `BotOnly`.
#[test]
fn authz_agrees_with_handler_extractors() {
    let (rows, _) = parse();
    let files = source_files();
    let sigs = handler_signatures(&files);
    let staff_types = staff_extractors(&files);
    let mut bad = Vec::new();
    let mut checked = 0;
    for ((m, p), row) in &rows {
        let Some(sig) = sigs.get(&(m.clone(), p.clone())) else {
            continue;
        };
        checked += 1;
        let staff_ext = staff_types.iter().any(|e| sig.contains(e.as_str()));
        match row.authz.as_str() {
            "staff" => {
                // `/admin` handlers always sit behind a staff extractor.
                if !staff_ext && p.starts_with("/admin") {
                    bad.push(format!("{m} {p}: staff row without a staff extractor"));
                }
            }
            "bot" => {
                if !sig.contains("BotOnly") {
                    bad.push(format!("{m} {p}: bot row without BotOnly"));
                }
            }
            "none" => {}
            other => {
                if staff_ext {
                    bad.push(format!("{m} {p}: {other} row, but the handler needs staff"));
                }
            }
        }
    }
    assert!(checked > 100, "only {checked} handlers were matched");
    assert!(
        bad.is_empty(),
        "AUTHZ disagrees with handlers:\n{}",
        bad.join("\n")
    );
}

/// The vocabulary has no catch-all: `other` is gone and stays gone.
#[test]
fn matrix_has_no_catch_all_authz() {
    let (rows, _) = parse();
    assert!(
        rows.values().all(|r| r.authz != "other"),
        "AUTHZ `other` is not allowed; name the real check"
    );
}

/// Every string literal passed to `.route(` in the router sources.
fn router_literals() -> BTreeSet<String> {
    fn walk(dir: &std::path::Path, out: &mut BTreeSet<String>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n == "tests") {
                    continue;
                }
                walk(&p, out);
            } else if p.extension().is_some_and(|e| e == "rs")
                && !p.file_name().unwrap().to_string_lossy().contains("test")
            {
                let src = std::fs::read_to_string(&p).unwrap();
                let mut rest = src.as_str();
                while let Some(i) = rest.find(".route(") {
                    rest = &rest[i + 7..];
                    let t = rest.trim_start();
                    if let Some(lit) = t.strip_prefix('"')
                        && let Some(end) = lit.find('"')
                    {
                        out.insert(lit[..end].to_owned());
                    }
                }
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut out,
    );
    out
}

fn render(
    docs: &BTreeMap<(String, String), String>,
    old: &BTreeMap<(String, String), Row>,
    undocumented: &BTreeSet<String>,
) -> String {
    let mut s = String::from(
        "# XC-SEC-005 route matrix. See crates/sr-api/tests/route_matrix.rs.\n\
         # METHOD\tPATH\tACCESS\tAUTHZ\tBOT\tBEARER\tPLATFORM (a trailing ? on AUTHZ means unreviewed; PLATFORM - means fill it for a staff row: NAME, A+B for all, A|B for any, never mixed)\n",
    );
    for ((m, p), access) in docs {
        let old_row = old.get(&(m.clone(), p.clone()));
        let authz = old_row.map_or_else(
            || format!("{}?", guess_authz(p, access)),
            |r| r.authz.clone(),
        );
        // D-360: a new row starts at `-`; the reviewer fills it for a staff row.
        let platform = old_row.map_or("-", |r| r.platform.as_str());
        s.push_str(&format!(
            "{m}\t{p}\t{access}\t{authz}\t{}\t{}\t{platform}\n",
            bot_of(m, p),
            bearer_of(m, p)
        ));
    }
    for u in undocumented {
        s.push_str(&format!("#undocumented\t{u}\n"));
    }
    s
}

#[test]
fn matrix_covers_every_route() {
    let docs = documented();
    let (rows, undocumented) = parse();

    if std::env::var_os("SR_UPDATE_ROUTE_MATRIX").is_some() {
        let doc_paths: BTreeSet<&str> = docs.keys().map(|(_, p)| p.as_str()).collect();
        let undoc: BTreeSet<String> = router_literals()
            .into_iter()
            .filter(|l| !doc_paths.contains(l.as_str()))
            .collect();
        std::fs::write(MATRIX_PATH, render(&docs, &rows, &undoc)).unwrap();
        return;
    }

    let missing: Vec<_> = docs.keys().filter(|k| !rows.contains_key(*k)).collect();
    assert!(
        missing.is_empty(),
        "routes without a matrix row (run with SR_UPDATE_ROUTE_MATRIX=1 and review): {missing:?}"
    );
    let stale: Vec<_> = rows.keys().filter(|k| !docs.contains_key(*k)).collect();
    assert!(
        stale.is_empty(),
        "matrix rows for removed routes: {stale:?}"
    );

    for ((m, p), row) in &rows {
        assert!(
            !row.authz.ends_with('?'),
            "{m} {p}: AUTHZ is unreviewed ({})",
            row.authz
        );
        assert!(
            AUTHZ_VALUES.contains(&row.authz.as_str()),
            "{m} {p}: unknown AUTHZ {}",
            row.authz
        );
        // D-360: the PLATFORM cell names the permission a staff row demands, spelled as in
        // `sr_core::PlatformPermissions`; every other row has `-`.
        let needs_bit = row.authz == "staff" || row.authz == "author_or_staff";
        if needs_bit {
            assert!(
                row.platform != "-",
                "{m} {p}: a {} row needs a PLATFORM permission (STAFF_BASIC when any appointment suffices)",
                row.authz
            );
            if let Err(e) = parse_platform(&row.platform, is_platform_name) {
                panic!("{m} {p}: {e}");
            }
        } else {
            assert_eq!(
                row.platform, "-",
                "{m} {p}: only staff rows carry a PLATFORM permission"
            );
        }
        assert_eq!(
            row.access,
            docs[&(m.clone(), p.clone())],
            "{m} {p}: ACCESS disagrees with the handler's OpenAPI security"
        );
        assert_eq!(
            row.access == "public",
            row.authz == "none",
            "{m} {p}: public routes have AUTHZ none, and only they do"
        );
        assert_eq!(
            row.bot,
            bot_of(m, p),
            "{m} {p}: BOT disagrees with auth::policy"
        );
        assert_eq!(
            row.bearer,
            bearer_of(m, p),
            "{m} {p}: BEARER disagrees with auth::policy"
        );
    }

    // A route registered in a router but missing from OpenAPI must be listed explicitly.
    let doc_paths: BTreeSet<&str> = docs.keys().map(|(_, p)| p.as_str()).collect();
    let unlisted: Vec<_> = router_literals()
        .into_iter()
        .filter(|l| !doc_paths.contains(l.as_str()) && !undocumented.contains(l))
        .collect();
    assert!(
        unlisted.is_empty(),
        "router paths neither in OpenAPI nor under #undocumented: {unlisted:?}"
    );
    let gone: Vec<_> = undocumented
        .iter()
        .filter(|u| doc_paths.contains(u.as_str()))
        .collect();
    assert!(
        gone.is_empty(),
        "now documented, drop from #undocumented: {gone:?}"
    );
}

fn concrete(path: &str) -> String {
    let mut out = String::new();
    let mut in_param = false;
    for c in path.chars() {
        match c {
            '{' => {
                in_param = true;
                out.push('1');
            }
            '}' => in_param = false,
            _ if in_param => {}
            c => out.push(c),
        }
    }
    out
}

/// Every route the matrix calls `authenticated` refuses a request with no credentials.
#[tokio::test]
async fn authenticated_routes_answer_401_without_credentials() {
    let (rows, _) = parse();
    let mut bad = Vec::new();
    for ((m, p), row) in &rows {
        if row.access != "authenticated" {
            continue;
        }
        let req = Request::builder()
            .method(Method::from_bytes(m.as_bytes()).unwrap())
            .uri(format!("/api/v1{}", concrete(p)))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let app = build_router(AppState::builder().build(), routes::all());
        let resp = app.oneshot(req).await.unwrap();
        if resp.status() != StatusCode::UNAUTHORIZED {
            bad.push(format!("{m} {p} -> {}", resp.status()));
        }
    }
    assert!(
        bad.is_empty(),
        "{} routes did not answer 401 to an anonymous request:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// Public routes must not leak a 401 either way; a sanity check that the harness is not
/// answering 401 to everything.
#[tokio::test]
async fn public_routes_do_not_demand_credentials() {
    let (rows, _) = parse();
    let mut checked = 0;
    for ((m, p), row) in &rows {
        if row.access != "public" || m != "GET" {
            continue;
        }
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/v1{}", concrete(p)))
            .body(Body::empty())
            .unwrap();
        let app = build_router(AppState::builder().build(), routes::all());
        let resp = app.oneshot(req).await.unwrap();
        assert_ne!(resp.status(), StatusCode::UNAUTHORIZED, "GET {p}");
        checked += 1;
    }
    assert!(checked > 0, "no public GET routes in the matrix");
}

/// D-366: the accepted and refused `PLATFORM` cell forms.
#[test]
fn platform_cell_grammar() {
    let parse = |c: &str| parse_platform(c, is_platform_name);
    assert_eq!(parse("OPERATOR"), Ok(Platform::One("OPERATOR".into())));
    assert_eq!(
        parse("STAFF_BASIC"),
        Ok(Platform::One("STAFF_BASIC".into()))
    );
    assert_eq!(parse("OWNER"), Ok(Platform::One("OWNER".into())));
    assert_eq!(
        parse("MANAGE_CASES+PLATFORM_BAN"),
        Ok(Platform::All(vec![
            "MANAGE_CASES".into(),
            "PLATFORM_BAN".into()
        ]))
    );
    assert_eq!(
        parse("PUBLISH_MEDIA|DELETE_CONTENT"),
        Ok(Platform::Any(vec![
            "PUBLISH_MEDIA".into(),
            "DELETE_CONTENT".into()
        ]))
    );
    assert!(matches!(
        parse("REVIEW_MEDIA|REVIEW_READING|REVIEW_APPS|REVIEW_PACKAGES"),
        Ok(Platform::Any(v)) if v.len() == 4
    ));
    for bad in [
        "MANAGE_CASES+PLATFORM_BAN|OPERATOR",
        "OPERATOR|OPERATOR",
        "OPERATOR+",
        "|OPERATOR",
        "OPERATOR||MANAGE_CASES",
        "OWNER|OPERATOR",
        "STAFF_BASIC+OPERATOR",
        "NOT_A_BIT",
        "",
    ] {
        assert!(parse(bad).is_err(), "{bad:?} must be refused");
    }
    assert!(
        parse("MANAGE_CASES+PLATFORM_BAN|OPERATOR")
            .unwrap_err()
            .contains("mixes")
    );
}
