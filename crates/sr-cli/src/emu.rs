//! `sr-cli emu fetch-cores` (ACT-EMU-003): download the pinned emulator core files and verify their SHA-256.
//!
//! The pin list is `deploy/emu-cores.lock` (compiled in, so the binary and the repo cannot drift); the same list drives
//! `deploy/scripts/fetch-emu-cores.sh` and the table in `deploy/EMULATOR.md`. The downloaded files are untrusted data
//! until their digest matches: they land in a `.part` file next to the target, are hashed, and only then renamed into
//! place. A mismatch removes the part file and fails the command, so a wrong file is never served.
//!
//! The transfer itself is done by the system `curl` (https only, https-only redirects, a size cap), because this crate
//! has no HTTP client and the wave adds no dependency. The digest check is in Rust and does not trust curl.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// The pinned list, tab-separated: dest, url, sha256, bytes, licence.
pub const BUNDLED_LOCK: &str = include_str!("../../../deploy/emu-cores.lock");
/// Every pinned URL starts with this; `--base-url` replaces it (tests, mirrors).
pub const PINNED_BASE: &str = "https://cdn.emulatorjs.org/4.2.3/data/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub dest: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
    pub licence: String,
}

/// Parse the lock text. Rejects anything that could write outside the destination or fetch over plain http.
pub fn parse_lock(text: &str) -> Result<Vec<Entry>> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = || format!("emu-cores.lock line {}", i + 1);
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() != 5 {
            bail!(
                "{}: expected 5 tab-separated columns, got {}",
                at(),
                cols.len()
            );
        }
        let dest = cols[0];
        let p = Path::new(dest);
        if dest.is_empty()
            || p.is_absolute()
            || dest.contains('\\')
            || p.components().any(|c| !matches!(c, Component::Normal(_)))
        {
            bail!("{}: unsafe destination {dest:?}", at());
        }
        if !cols[1].starts_with(PINNED_BASE) {
            bail!("{}: url must start with {PINNED_BASE}", at());
        }
        let sha = cols[2];
        if sha.len() != 64 || !sha.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            bail!("{}: sha256 must be 64 lower-case hex digits", at());
        }
        let bytes = cols[3]
            .parse::<u64>()
            .with_context(|| format!("{}: bad size", at()))?;
        out.push(Entry {
            dest: dest.to_string(),
            url: cols[1].to_string(),
            sha256: sha.to_string(),
            bytes,
            licence: cols[4].to_string(),
        });
    }
    if out.is_empty() {
        bail!("emu-cores.lock has no entries");
    }
    Ok(out)
}

#[derive(Debug)]
pub struct Options {
    /// The served cores directory (dev: apps/web/public/emu/cores).
    pub dest: PathBuf,
    /// Replace the pinned CDN prefix. Must be https, or http to a loopback address (tests).
    pub base_url: Option<String>,
    /// Another pin list (tests); the bundled one by default.
    pub lock: Option<PathBuf>,
    /// Download even when a file with the right digest is already there.
    pub force: bool,
    /// Do not download: report whether the files in `dest` match the pins; fail if any does not.
    pub verify_only: bool,
}

fn base_is_allowed(base: &str) -> bool {
    if base.starts_with("https://") {
        return true;
    }
    ["http://127.0.0.1", "http://localhost", "http://[::1]"]
        .iter()
        .any(|p| {
            base.strip_prefix(p).is_some_and(|rest| {
                rest.is_empty() || rest.starts_with(':') || rest.starts_with('/')
            })
        })
}

/// The URL to fetch for an entry, with `--base-url` applied.
pub fn effective_url(entry: &Entry, base: Option<&str>) -> String {
    match base {
        Some(b) => {
            let b = b.trim_end_matches('/');
            let tail = entry.url.strip_prefix(PINNED_BASE).unwrap_or(&entry.url);
            format!("{b}/{tail}")
        }
        None => entry.url.clone(),
    }
}

/// SHA-256 and length of a file.
fn digest(path: &Path) -> Result<(String, u64)> {
    crate::ops::sha256_file(path)
}

fn matches_pin(path: &Path, e: &Entry) -> bool {
    path.is_file() && digest(path).is_ok_and(|(d, n)| d == e.sha256 && n == e.bytes)
}

fn download(url: &str, to: &Path, max_bytes: u64, insecure_loopback: bool) -> Result<()> {
    let proto = if insecure_loopback {
        "=http,https"
    } else {
        "=https"
    };
    let status = Command::new("curl")
        .args(["-fL", "-sS", "--proto", proto, "--proto-redir", "=https"])
        .args(["--max-filesize", &max_bytes.to_string()])
        .args(["--max-time", "300", "-o"])
        .arg(to)
        .arg(url)
        .status()
        .context("could not run curl: it must be installed and on PATH")?;
    if !status.success() {
        bail!("curl failed for {url} ({status})");
    }
    Ok(())
}

/// Fetch (or verify) every pinned file. Returns how many files were downloaded.
pub fn fetch_cores(opts: &Options) -> Result<usize> {
    let text = match &opts.lock {
        Some(p) => fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?,
        None => BUNDLED_LOCK.to_string(),
    };
    let entries = parse_lock(&text)?;
    if let Some(b) = &opts.base_url
        && !base_is_allowed(b)
    {
        bail!("--base-url must be https (http only for 127.0.0.1, localhost or [::1])");
    }
    let loopback = opts
        .base_url
        .as_deref()
        .is_some_and(|b| b.starts_with("http://"));

    let mut downloaded = 0;
    let mut bad = Vec::new();
    for e in &entries {
        let target = opts.dest.join(&e.dest);
        if !opts.force && matches_pin(&target, e) {
            println!("ok       {} ({} bytes, {})", e.dest, e.bytes, e.licence);
            continue;
        }
        if opts.verify_only {
            println!("MISSING  {} (absent or digest differs)", e.dest);
            bad.push(e.dest.clone());
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let part = target.with_extension("part");
        let _ = fs::remove_file(&part);
        let url = effective_url(e, opts.base_url.as_deref());
        // Allow a small margin over the pinned size so a longer file is caught by the digest check, not truncated.
        if let Err(err) = download(&url, &part, e.bytes + 4096, loopback) {
            let _ = fs::remove_file(&part);
            return Err(err);
        }
        let (got, len) = digest(&part)?;
        if got != e.sha256 || len != e.bytes {
            let _ = fs::remove_file(&part);
            bail!(
                "digest mismatch for {}: expected {} ({} bytes), got {got} ({len} bytes); the file was discarded",
                e.dest,
                e.sha256,
                e.bytes
            );
        }
        fs::rename(&part, &target).with_context(|| format!("moving into {}", target.display()))?;
        println!("fetched  {} ({} bytes, {})", e.dest, e.bytes, e.licence);
        downloaded += 1;
    }
    if !bad.is_empty() {
        bail!(
            "{} pinned file(s) missing or different in {}",
            bad.len(),
            opts.dest.display()
        );
    }
    Ok(downloaded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_lock_parses_and_has_only_allowed_cores() {
        let entries = parse_lock(BUNDLED_LOCK).unwrap();
        assert!(entries.len() >= 7);
        for e in &entries {
            for banned in [
                "snes9x",
                "genesis_plus_gx",
                "picodrive",
                "fbneo",
                "fbalpha",
                "mame",
                "nestopia",
            ] {
                assert!(!e.dest.contains(banned), "{} is a rejected core", e.dest);
            }
            assert!(e.url.starts_with(PINNED_BASE));
        }
        assert!(entries.iter().any(|e| e.dest == "fceumm/fceumm-wasm.data"));
        assert!(entries.iter().any(|e| e.dest == "extract7z.js"));
    }

    #[test]
    fn a_bad_lock_is_refused() {
        let ok = format!("a/b.data\t{PINNED_BASE}x\t{}\t3\tGPL-2.0\n", "a".repeat(64));
        assert!(parse_lock(&ok).is_ok());
        for bad in [
            format!("../b\t{PINNED_BASE}x\t{}\t3\tL\n", "a".repeat(64)),
            format!("/abs\t{PINNED_BASE}x\t{}\t3\tL\n", "a".repeat(64)),
            format!("a\\b\t{PINNED_BASE}x\t{}\t3\tL\n", "a".repeat(64)),
            format!("a\thttp://cdn.example/x\t{}\t3\tL\n", "a".repeat(64)),
            format!("a\t{PINNED_BASE}x\t{}\t3\tL\n", "A".repeat(64)),
            format!("a\t{PINNED_BASE}x\tshort\t3\tL\n"),
            format!("a\t{PINNED_BASE}x\t{}\tnope\tL\n", "a".repeat(64)),
            "only\ttwo\n".to_string(),
            "# nothing\n".to_string(),
        ] {
            assert!(parse_lock(&bad).is_err(), "should refuse {bad:?}");
        }
    }

    #[test]
    fn base_urls_are_https_or_loopback() {
        assert!(base_is_allowed("https://mirror.example/emu"));
        assert!(base_is_allowed("http://127.0.0.1:8095"));
        assert!(base_is_allowed("http://localhost:8095/x"));
        assert!(!base_is_allowed("http://mirror.example"));
        assert!(!base_is_allowed("http://127.0.0.1.evil.example"));
        assert!(!base_is_allowed("ftp://x"));
    }

    #[test]
    fn the_base_replaces_the_pinned_prefix() {
        let e = parse_lock(BUNDLED_LOCK).unwrap().remove(1);
        let u = effective_url(&e, Some("http://127.0.0.1:8095/"));
        assert!(u.starts_with("http://127.0.0.1:8095/cores/"), "{u}");
        assert_eq!(effective_url(&e, None), e.url);
    }
}
