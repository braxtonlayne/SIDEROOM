//! The hardened sandbox of an Arcade game (ARC-SBX-001): what the server says about a bundle's executable
//! code, and the Content-Security-Policy its origin sends.
//!
//! * [`code_sha256`]: the hash of a bundle's executable code only (`.js`, `.mjs`, `.wasm`). It is what the
//!   operator signs and what a member's vouch is pinned to, so art, audio and other assets can change without
//!   invalidating either, and no signing step ever needs the developer's external asset bucket.
//! * [`audit`]: the automated static audit that must pass before the operator signs. It reads the code files
//!   and nothing else. It is a tripwire for obvious attempts (network APIs, `eval`, reaching the parent page,
//!   absolute URLs), not a proof of safety: the walls are the iframe `sandbox`, the cookieless origin and the
//!   CSP below, and a signature says "this exact code passed the audit", never "this code cannot misbehave".
//! * [`parse_connect`]: the `connect` list of `sideroom.json`, the only way a game names an extra network
//!   host. Fetches to anything else are refused by the browser.
//! * [`csp`]: the policy. **CSP is per host, not per URL**: `connect-src https://bucket.example` admits every
//!   path on that host, so the reviewer approves hosts, and a presigned URL is only as narrow as the host that
//!   serves it.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

/// Extensions that are executable code. `html` is not: inline script is refused by the CSP, so a page can only
/// run the `.js`/`.mjs`/`.wasm` files that are hashed here.
pub const CODE_EXTENSIONS: &[&str] = &["js", "mjs", "wasm"];

/// Most hosts a bundle may name in `connect`.
pub const MAX_CONNECT_HOSTS: usize = 4;

/// The domain separation prefix of [`code_sha256`].
const HASH_DOMAIN: &[u8] = b"sideroom/arcade-code-bundle/v1\0";

/// Whether `path` is executable code (by extension, case-insensitive).
pub fn is_code_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| CODE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// SHA-256 (lowercase hex) of the executable code of a bundle: the code files sorted by path, each framed as
/// `u32be len(path) || path || u64be len(data) || data` after the domain prefix, so two different file sets can
/// never produce the same byte stream. Non-code files in `files` are ignored. `None` when there is no code at
/// all (a bundle of pure HTML has nothing to sign).
pub fn code_sha256<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Option<String> {
    let mut code: Vec<(&str, &[u8])> = files.into_iter().filter(|(p, _)| is_code_path(p)).collect();
    if code.is_empty() {
        return None;
    }
    code.sort_by(|a, b| a.0.cmp(b.0));
    let mut h = Sha256::new();
    h.update(HASH_DOMAIN);
    for (path, data) in code {
        h.update(u32::try_from(path.len()).unwrap_or(u32::MAX).to_be_bytes());
        h.update(path.as_bytes());
        h.update((data.len() as u64).to_be_bytes());
        h.update(data);
    }
    Some(data_encoding::HEXLOWER.encode(&h.finalize()))
}

/// How bad an audit finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The operator must not sign this code.
    Block,
    /// Worth a human's look; does not stop a signature.
    Warn,
}

/// One thing the audit found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Finding {
    pub path: String,
    pub rule: String,
    pub severity: Severity,
    pub detail: String,
}

/// What [`audit`] saw. Stored with the signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AuditReport {
    /// Audit rule set version; a new rule set makes older signatures look older, never invalid.
    pub version: u32,
    pub files_scanned: usize,
    pub bytes_scanned: u64,
    pub findings: Vec<Finding>,
    /// Import module names found in `.wasm` files (information for the reviewer).
    pub wasm_import_modules: Vec<String>,
}

impl AuditReport {
    /// No `Block` finding.
    pub fn passed(&self) -> bool {
        !self.findings.iter().any(|f| f.severity == Severity::Block)
    }
}

/// Current rule set.
pub const AUDIT_VERSION: u32 = 1;

/// (rule id, needle, detail). A code file containing the needle is blocked.
const BLOCK: &[(&str, &str, &str)] = &[
    ("eval", "eval(", "dynamic code evaluation"),
    ("function-ctor", "new Function(", "dynamic code evaluation"),
    ("cookie", "document.cookie", "cookie access"),
    ("domain", "document.domain", "origin relaxation"),
    ("xhr", "XMLHttpRequest", "direct network access"),
    ("websocket", "WebSocket(", "direct network access"),
    ("event-source", "EventSource(", "direct network access"),
    ("beacon", "sendBeacon", "direct network access"),
    (
        "webrtc",
        "RTCPeerConnection",
        "direct peer connections (use the SDK data channel)",
    ),
    ("webtransport", "WebTransport(", "direct network access"),
    ("import-scripts", "importScripts(", "loading outside code"),
    ("service-worker", "serviceWorker", "service workers"),
    ("top-window", "window.top", "reaching the top window"),
    ("top-window", "top.location", "reaching the top window"),
    (
        "parent-document",
        "parent.document",
        "reaching the parent page",
    ),
    (
        "parent-document",
        "parent.window",
        "reaching the parent page",
    ),
    (
        "parent-document",
        "parent.location",
        "reaching the parent page",
    ),
    ("opener", "window.opener", "reaching the opener"),
    (
        "popup",
        "window.open(",
        "opening windows (use the SDK openExternalLink)",
    ),
];

/// Needles that are only worth a look.
const WARN: &[(&str, &str, &str)] = &[
    (
        "fetch",
        "fetch(",
        "network call; the CSP refuses hosts the manifest did not declare",
    ),
    (
        "worker",
        "new Worker(",
        "worker script; it needs a hosted file on the same origin",
    ),
    (
        "local-storage",
        "localStorage",
        "no storage in the sandbox (use sdk.storage)",
    ),
    ("indexed-db", "indexedDB", "no storage in the sandbox"),
    ("dynamic-import", "import(", "dynamic import"),
];

/// Absolute URLs a script may name: XML namespaces and nothing that is fetched.
const URL_OK: &[&str] = &[
    "http://www.w3.org/",
    "https://www.w3.org/",
    "http://www.apple.com/DTDs/",
    "https://reactjs.org/docs/",
    "https://react.dev/",
    "https://github.com/",
    "https://developer.mozilla.org/",
];

/// Whether `text` has `needle` as a whole token: a needle that starts like an identifier must not continue one
/// (`eval(` is found in `x.eval(1)`, not in `retrieval(1)`).
fn has_token(text: &str, needle: &str) -> bool {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$';
    let starts_ident = needle.chars().next().is_some_and(ident);
    let mut from = 0;
    while let Some(i) = text[from..].find(needle) {
        let at = from + i;
        if !starts_ident || !text[..at].chars().next_back().is_some_and(ident) {
            return true;
        }
        from = at + needle.len();
    }
    false
}

/// Scan a code file's text for absolute URLs outside [`URL_OK`] and the declared `connect` hosts.
fn url_findings(path: &str, text: &str, connect: &[String], out: &mut Vec<Finding>) {
    let mut seen = BTreeSet::new();
    let mut rest = text;
    while let Some(i) = rest.find("http") {
        let tail = &rest[i..];
        let is_url = tail.starts_with("http://") || tail.starts_with("https://");
        if is_url {
            let end = tail
                .find(|c: char| {
                    c.is_whitespace() || matches!(c, '"' | '\'' | '`' | ')' | '<' | '>' | '\\')
                })
                .unwrap_or(tail.len());
            let url = &tail[..end];
            let ok = URL_OK.iter().any(|p| url.starts_with(p))
                || connect
                    .iter()
                    .any(|c| c != "self" && url.starts_with(c.as_str()));
            if !ok && seen.insert(url.to_owned()) && seen.len() <= 20 {
                out.push(Finding {
                    path: path.to_owned(),
                    rule: "absolute-url".to_owned(),
                    severity: Severity::Block,
                    detail: format!(
                        "names the address {}",
                        url.chars().take(80).collect::<String>()
                    ),
                });
            }
        }
        rest = &tail[4..];
    }
}

/// The names of the modules a `.wasm` file imports, or `None` when the file is not a well-formed module
/// header (magic `\0asm`, version 1) or its import section is cut off.
pub fn wasm_import_modules(data: &[u8]) -> Option<Vec<String>> {
    if data.len() < 8 || &data[..4] != b"\0asm" || data[4..8] != [1, 0, 0, 0] {
        return None;
    }
    fn leb(d: &[u8], i: &mut usize) -> Option<u64> {
        let mut r = 0u64;
        for shift in (0..70).step_by(7) {
            let b = *d.get(*i)?;
            *i += 1;
            r |= u64::from(b & 0x7f).checked_shl(shift).unwrap_or(0);
            if b & 0x80 == 0 {
                return Some(r);
            }
        }
        None
    }
    let mut i = 8;
    let mut mods = BTreeSet::new();
    while i < data.len() {
        let id = data[i];
        i += 1;
        let size = usize::try_from(leb(data, &mut i)?).ok()?;
        let end = i.checked_add(size).filter(|e| *e <= data.len())?;
        if id == 2 {
            let mut j = i;
            let count = leb(data, &mut j)?;
            for _ in 0..count {
                let n = usize::try_from(leb(data, &mut j)?).ok()?;
                let name = data.get(j..j.checked_add(n)?)?;
                j += n;
                mods.insert(String::from_utf8_lossy(name).into_owned());
                let n = usize::try_from(leb(data, &mut j)?).ok()?;
                j = j.checked_add(n)?;
                let kind = *data.get(j)?;
                j += 1;
                match kind {
                    0 => {
                        leb(data, &mut j)?;
                    }
                    1 => {
                        // table: reftype, limits
                        j += 1;
                        let flag = *data.get(j)?;
                        j += 1;
                        leb(data, &mut j)?;
                        if flag & 1 == 1 {
                            leb(data, &mut j)?;
                        }
                    }
                    2 => {
                        let flag = *data.get(j)?;
                        j += 1;
                        leb(data, &mut j)?;
                        if flag & 1 == 1 {
                            leb(data, &mut j)?;
                        }
                    }
                    3 => j += 2,
                    4 => {
                        j += 1;
                        leb(data, &mut j)?;
                    }
                    _ => return None,
                }
            }
        }
        i = end;
    }
    Some(mods.into_iter().collect())
}

/// Audit the code files of a bundle. `files` are `(path, bytes)` of any files; only code is read. `connect` is
/// the manifest's `connect` list (see [`parse_connect`]); a URL under one of those hosts is not a finding.
pub fn audit(files: &[(&str, &[u8])], connect: &[String]) -> AuditReport {
    let mut findings = Vec::new();
    let mut scanned = 0usize;
    let mut bytes = 0u64;
    let mut modules = BTreeSet::new();
    for (path, data) in files.iter().filter(|(p, _)| is_code_path(p)) {
        scanned += 1;
        bytes += data.len() as u64;
        if path.to_ascii_lowercase().ends_with(".wasm") {
            match wasm_import_modules(data) {
                Some(m) => modules.extend(m),
                None => findings.push(Finding {
                    path: (*path).to_owned(),
                    rule: "wasm-malformed".to_owned(),
                    severity: Severity::Block,
                    detail: "is not a well-formed WebAssembly module".to_owned(),
                }),
            }
            continue;
        }
        let Ok(text) = std::str::from_utf8(data) else {
            findings.push(Finding {
                path: (*path).to_owned(),
                rule: "not-utf8".to_owned(),
                severity: Severity::Block,
                detail: "is not UTF-8 text".to_owned(),
            });
            continue;
        };
        let mut hit = BTreeSet::new();
        for (rule, needle, detail) in BLOCK {
            if has_token(text, needle) && hit.insert(*rule) {
                findings.push(Finding {
                    path: (*path).to_owned(),
                    rule: (*rule).to_owned(),
                    severity: Severity::Block,
                    detail: format!("uses {detail}"),
                });
            }
        }
        for (rule, needle, detail) in WARN {
            if has_token(text, needle) && hit.insert(*rule) {
                findings.push(Finding {
                    path: (*path).to_owned(),
                    rule: (*rule).to_owned(),
                    severity: Severity::Warn,
                    detail: format!("uses {detail}"),
                });
            }
        }
        url_findings(path, text, connect, &mut findings);
    }
    if scanned == 0 {
        findings.push(Finding {
            path: String::new(),
            rule: "no-code".to_owned(),
            severity: Severity::Block,
            detail: "the bundle has no .js, .mjs or .wasm file to sign".to_owned(),
        });
    }
    findings.sort_by(|a, b| (&a.path, &a.rule).cmp(&(&b.path, &b.rule)));
    AuditReport {
        version: AUDIT_VERSION,
        files_scanned: scanned,
        bytes_scanned: bytes,
        findings,
        wasm_import_modules: modules.into_iter().collect(),
    }
}

/// The `connect` list of a `sideroom.json`: `"self"` (the sandbox host's own files) or `https://host[:port]`
/// origins. At most [`MAX_CONNECT_HOSTS`]; no path, query, fragment or credentials, no IP literals, no
/// `localhost`, no wildcard, and a host with a dot. Sorted, no duplicates. A manifest without `connect` gives
/// an empty list (`connect-src 'none'`).
pub fn parse_connect(manifest: &[u8]) -> Result<Vec<String>, String> {
    let v: serde_json::Value =
        serde_json::from_slice(manifest).map_err(|_| "is not valid JSON".to_owned())?;
    let Some(list) = v.get("connect") else {
        return Ok(Vec::new());
    };
    let list = list
        .as_array()
        .ok_or_else(|| "has a connect list that is not a list".to_owned())?;
    let mut out = BTreeSet::new();
    for item in list {
        let s = item
            .as_str()
            .ok_or_else(|| "has a connect entry that is not text".to_owned())?;
        if s == "self" {
            out.insert("self".to_owned());
            continue;
        }
        let u = url::Url::parse(s)
            .map_err(|_| format!("has the connect entry {s:?}, not an https origin"))?;
        let host = u.host_str().unwrap_or_default();
        let origin_only = u.path() == "/" && u.query().is_none() && u.fragment().is_none();
        let bad = u.scheme() != "https"
            || !u.username().is_empty()
            || u.password().is_some()
            || !origin_only
            || !host.contains('.')
            || host.contains('*')
            || matches!(u.host(), Some(url::Host::Ipv4(_) | url::Host::Ipv6(_)))
            || host.eq_ignore_ascii_case("localhost")
            || host.ends_with(".local")
            || host.ends_with(".internal");
        if bad || s.contains('*') {
            return Err(format!(
                "has the connect entry {:?}; use \"self\" or a public https origin such as https://assets.example.com",
                s.chars().take(80).collect::<String>()
            ));
        }
        out.insert(u.origin().ascii_serialization());
    }
    if out.len() > MAX_CONNECT_HOSTS {
        return Err(format!("names more than {MAX_CONNECT_HOSTS} connect hosts"));
    }
    Ok(out.into_iter().collect())
}

/// What the CSP of one activity response depends on.
#[derive(Debug, Clone, Copy)]
pub struct CspInput<'a> {
    /// The app shell's origin: the only page allowed to frame an activity.
    pub app_origin: &'a str,
    /// A development build served live from its owner's URL (owner and testers only).
    pub development: bool,
    /// The application has URL mappings (requests under them reach the developer's server).
    pub has_mappings: bool,
    /// The reviewed bundle's `connect` list.
    pub connect: &'a [String],
}

/// The `connect-src` value: `'none'` unless the reviewed manifest named hosts. `"self"` means the sandbox
/// host's own static files and is only honoured when the application has no URL mappings, because a mapping
/// turns the host into a pipe to the developer's server (any GET under it could carry data out). A
/// development build keeps `'self'` so its owner can work against mapped endpoints.
pub fn connect_src(i: &CspInput<'_>) -> String {
    let mut tokens: Vec<String> = Vec::new();
    if i.development {
        tokens.push("'self'".to_owned());
    }
    for c in i.connect {
        if c == "self" {
            if !i.has_mappings && !i.development {
                tokens.push("'self'".to_owned());
            }
        } else {
            tokens.push(c.clone());
        }
    }
    if tokens.is_empty() {
        "'none'".to_owned()
    } else {
        tokens.join(" ")
    }
}

/// The Content-Security-Policy of the activity origin. `sandbox` makes the server enforce the iframe's
/// restrictions itself, so opening the activity's address directly (or framing it with a weaker `sandbox`
/// attribute) gives an opaque origin too, with the same two permissions as the app's iframe.
pub fn csp(i: &CspInput<'_>) -> String {
    format!(
        "sandbox allow-scripts allow-pointer-lock; \
         default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: blob:; font-src 'self'; media-src 'self' blob:; connect-src {}; \
         worker-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; \
         frame-src 'none'; frame-ancestors {}",
        connect_src(i),
        i.app_origin
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files<'a>(v: &'a [(&'a str, &'a [u8])]) -> Vec<(&'a str, &'a [u8])> {
        v.to_vec()
    }

    #[test]
    fn code_hash_covers_code_only_and_ignores_order() {
        let a = code_sha256(files(&[
            ("main.js", b"x"),
            ("pic.png", b"1"),
            ("g.wasm", b"\0asm"),
        ]));
        let b = code_sha256(files(&[
            ("g.wasm", b"\0asm"),
            ("pic.png", b"2"),
            ("main.js", b"x"),
        ]));
        assert_eq!(a, b, "assets do not change the hash, order does not matter");
        let c = code_sha256(files(&[("main.js", b"y"), ("g.wasm", b"\0asm")]));
        assert_ne!(a, c, "different code, different hash");
        assert_eq!(a.as_deref().map(str::len), Some(64));
        assert_eq!(code_sha256(files(&[("index.html", b"<p>")])), None);
    }

    #[test]
    fn code_hash_framing_prevents_boundary_games() {
        // "a.js"+"bc" vs "a.jsb"... no: moving bytes between a path and its data must change the hash.
        let one = code_sha256(files(&[("a.js", b"xb.jsy")]));
        let two = code_sha256(files(&[("a.jsxb.js", b"y")]));
        assert_ne!(one, two);
        let three = code_sha256(files(&[("a.js", b"x"), ("b.js", b"y")]));
        let four = code_sha256(files(&[("a.js", b"xy")]));
        assert_ne!(three, four);
    }

    #[test]
    fn extension_check_is_case_insensitive_and_exact() {
        assert!(is_code_path("a/b/Main.JS"));
        assert!(is_code_path("x.wasm"));
        assert!(is_code_path("x.mjs"));
        assert!(!is_code_path("x.json"));
        assert!(!is_code_path("js"));
        assert!(!is_code_path("x.js.map"));
    }

    #[test]
    fn a_clean_script_passes_and_a_dangerous_one_is_blocked() {
        let clean = b"const c = document.getElementById('game'); sdk.state.set({a:1}); // https://www.w3.org/2000/svg";
        let r = audit(&[("main.js", clean)], &[]);
        assert!(r.passed(), "{:?}", r.findings);
        assert_eq!(r.files_scanned, 1);

        for (src, rule) in [
            (&b"eval('1')"[..], "eval"),
            (b"new Function('return 1')", "function-ctor"),
            (b"document.cookie = 'a=b'", "cookie"),
            (b"new XMLHttpRequest()", "xhr"),
            (b"new WebSocket('wss://x')", "websocket"),
            (b"navigator.sendBeacon(u)", "beacon"),
            (b"window.top.location = u", "top-window"),
            (b"window.open(u)", "popup"),
            (b"fetch('https://evil.example/x')", "absolute-url"),
        ] {
            let r = audit(&[("m.js", src)], &[]);
            assert!(!r.passed(), "{rule}");
            assert!(
                r.findings.iter().any(|f| f.rule == rule),
                "{rule}: {:?}",
                r.findings
            );
        }
    }

    #[test]
    fn a_needle_inside_a_longer_name_is_not_a_hit() {
        let src = b"function retrieval(x){return x} retrieval(1); const desktop = prefetch(2); myFunction('a')";
        let r = audit(&[("m.js", src)], &[]);
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        let r = audit(&[("m.js", b"obj.eval(1)")], &[]);
        assert!(!r.passed());
    }

    #[test]
    fn warnings_do_not_block_and_declared_hosts_are_not_urls_to_flag() {
        let r = audit(
            &[("m.js", b"fetch('x.json'); localStorage.getItem('a')")],
            &[],
        );
        assert!(r.passed());
        assert!(r.findings.iter().all(|f| f.severity == Severity::Warn));
        assert_eq!(r.findings.len(), 2);
        let src = b"fetch('https://assets.example.com/a.bin')";
        assert!(!audit(&[("m.js", src)], &[]).passed());
        assert!(audit(&[("m.js", src)], &["https://assets.example.com".to_owned()]).passed());
    }

    #[test]
    fn a_bundle_without_code_cannot_pass() {
        let r = audit(&[("index.html", b"<p>")], &[]);
        assert!(!r.passed());
        assert_eq!(r.findings[0].rule, "no-code");
    }

    /// `(module (import "env" "f" (func)) (import "wasi_snapshot_preview1" "g" (func)))`.
    fn wasm() -> Vec<u8> {
        let mut m = b"\0asm\x01\0\0\0".to_vec();
        // type section: one func type () -> ()
        m.extend_from_slice(&[1, 4, 1, 0x60, 0, 0]);
        // import section
        let mut sec = vec![2u8]; // count
        for (module, name) in [("env", "f"), ("wasi_snapshot_preview1", "g")] {
            sec.push(module.len() as u8);
            sec.extend_from_slice(module.as_bytes());
            sec.push(name.len() as u8);
            sec.extend_from_slice(name.as_bytes());
            sec.extend_from_slice(&[0, 0]); // func, type 0
        }
        m.push(2);
        m.push(sec.len() as u8);
        m.extend_from_slice(&sec);
        m
    }

    #[test]
    fn wasm_imports_are_listed_and_garbage_is_blocked() {
        let w = wasm();
        assert_eq!(
            wasm_import_modules(&w),
            Some(vec!["env".to_owned(), "wasi_snapshot_preview1".to_owned()])
        );
        let r = audit(&[("g.wasm", &w)], &[]);
        assert!(r.passed());
        assert_eq!(r.wasm_import_modules, vec!["env", "wasi_snapshot_preview1"]);
        let bad = audit(&[("g.wasm", b"MZ not wasm")], &[]);
        assert!(!bad.passed());
        assert_eq!(bad.findings[0].rule, "wasm-malformed");
        // A cut-off section is malformed, never a panic.
        assert_eq!(wasm_import_modules(&w[..w.len() - 3]), None);
        assert_eq!(wasm_import_modules(&[]), None);
    }

    #[test]
    fn connect_entries_are_public_https_origins_or_self() {
        assert_eq!(parse_connect(b"{}").unwrap(), Vec::<String>::new());
        assert_eq!(
            parse_connect(
                br#"{"connect":["https://B.example.com","self","https://assets.example.com:8443/","self"]}"#
            )
            .unwrap(),
            vec!["https://assets.example.com:8443", "https://b.example.com", "self"]
        );
        for bad in [
            r#"{"connect":["http://a.example.com"]}"#,
            r#"{"connect":["https://a.example.com/path"]}"#,
            r#"{"connect":["https://a.example.com/?q=1"]}"#,
            r#"{"connect":["https://*.example.com"]}"#,
            r#"{"connect":["https://user:pw@a.example.com"]}"#,
            r#"{"connect":["https://10.0.0.1"]}"#,
            r#"{"connect":["https://[::1]"]}"#,
            r#"{"connect":["https://localhost"]}"#,
            r#"{"connect":["https://intranet"]}"#,
            r#"{"connect":["https://a.internal"]}"#,
            r#"{"connect":["*"]}"#,
            r#"{"connect":[7]}"#,
            r#"{"connect":"self"}"#,
            r#"{"connect":["https://a.example.com","https://b.example.com","https://c.example.com","https://d.example.com","https://e.example.com"]}"#,
        ] {
            assert!(parse_connect(bad.as_bytes()).is_err(), "{bad}");
        }
    }

    fn input<'a>(dev: bool, mappings: bool, connect: &'a [String]) -> CspInput<'a> {
        CspInput {
            app_origin: "https://chat.example.org",
            development: dev,
            has_mappings: mappings,
            connect,
        }
    }

    #[test]
    fn connect_src_is_none_unless_the_reviewed_manifest_named_hosts() {
        assert_eq!(connect_src(&input(false, false, &[])), "'none'");
        assert_eq!(connect_src(&input(false, true, &[])), "'none'");
        let hosts = vec!["https://a.example.com".to_owned(), "self".to_owned()];
        assert_eq!(
            connect_src(&input(false, false, &hosts)),
            "https://a.example.com 'self'"
        );
        // A mapping turns 'self' into a pipe to the developer's server: it is not honoured.
        assert_eq!(
            connect_src(&input(false, true, &hosts)),
            "https://a.example.com"
        );
        assert_eq!(connect_src(&input(true, true, &[])), "'self'");
    }

    #[test]
    fn the_policy_enforces_the_sandbox_and_frames_only_the_shell() {
        let p = csp(&input(false, false, &[]));
        assert!(
            p.starts_with("sandbox allow-scripts allow-pointer-lock;"),
            "{p}"
        );
        assert!(!p.contains("allow-same-origin"));
        assert!(p.contains("default-src 'none'"));
        assert!(p.contains("connect-src 'none';"));
        assert!(p.contains("script-src 'self' 'wasm-unsafe-eval';"));
        assert!(!p.contains("'unsafe-eval'"));
        assert!(p.contains("frame-ancestors https://chat.example.org"));
        assert!(p.contains("form-action 'none'"));
    }
}
