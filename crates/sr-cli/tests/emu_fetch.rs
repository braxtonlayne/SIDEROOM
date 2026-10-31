//! `sr-cli emu fetch-cores` against a fake local HTTP server (ACT-EMU-003): the happy path, the skip of files that
//! already match, a digest mismatch (the wrong file is discarded and nothing is left in the served directory), a size
//! overrun, a 404, and the refusal of a plain-http mirror that is not loopback. Needs `curl` on PATH (the command does).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use sha2::{Digest, Sha256};

static N: AtomicU32 = AtomicU32::new(0);
const BASE: &str = "https://cdn.emulatorjs.org/4.2.3/data/";

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "sr-cli-emu-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sha(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

/// A server that answers GET <path> from the map (path without the leading slash), 404 otherwise.
fn serve(files: HashMap<String, Vec<u8>>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let hits = Arc::new(Mutex::new(Vec::new()));
    let h = hits.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 2048];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = req
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .trim_start_matches('/')
                .to_string();
            h.lock().unwrap().push(path.clone());
            match files.get(&path) {
                Some(body) => {
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = s.write_all(head.as_bytes());
                    let _ = s.write_all(body);
                }
                None => {
                    let _ = s.write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                }
            }
        }
    });
    (base, hits)
}

fn lock(dir: &Dir, rows: &[(&str, &[u8])]) -> PathBuf {
    let mut text = String::from("# test pins\n");
    for (dest, body) in rows {
        // the dest is also the URL tail (the real lock names them differently, the base handling is the same)
        text.push_str(&format!(
            "{dest}\t{BASE}{dest}\t{}\t{}\tGPL-2.0\n",
            sha(body),
            body.len()
        ));
    }
    let p = dir.0.join("test.lock");
    std::fs::write(&p, text).unwrap();
    p
}

fn run(lock: &Path, dest: &Path, base: &str, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sr-cli"))
        .args(["emu", "fetch-cores", "--lock"])
        .arg(lock)
        .arg("--dest")
        .arg(dest)
        .args(["--base-url", base])
        .args(extra)
        .output()
        .unwrap()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

const A: &[u8] = b"7z-pretend core archive A";
const B: &[u8] = b"pretend helper script B, a bit longer";

#[test]
fn fetches_verifies_and_skips_what_already_matches() {
    let dir = Dir::new();
    let lock = lock(&dir, &[("cores/a.data", A), ("compression/b.js", B)]);
    let (base, hits) = serve(HashMap::from([
        ("cores/a.data".to_string(), A.to_vec()),
        ("compression/b.js".to_string(), B.to_vec()),
    ]));
    let dest = dir.0.join("served");

    let first = run(&lock, &dest, &base, &[]);
    assert!(first.status.success(), "{}", text(&first));
    assert_eq!(std::fs::read(dest.join("cores/a.data")).unwrap(), A);
    assert_eq!(std::fs::read(dest.join("compression/b.js")).unwrap(), B);
    assert_eq!(hits.lock().unwrap().len(), 2);

    // Second run: both files already match, so nothing is requested.
    let second = run(&lock, &dest, &base, &[]);
    assert!(second.status.success(), "{}", text(&second));
    assert_eq!(hits.lock().unwrap().len(), 2, "{}", text(&second));
    // --force downloads again.
    let forced = run(&lock, &dest, &base, &["--force"]);
    assert!(forced.status.success());
    assert_eq!(hits.lock().unwrap().len(), 4);

    // --verify-only is happy when the files match and fails when one is changed.
    assert!(
        run(&lock, &dest, &base, &["--verify-only"])
            .status
            .success()
    );
    std::fs::write(dest.join("cores/a.data"), b"tampered").unwrap();
    let v = run(&lock, &dest, &base, &["--verify-only"]);
    assert!(!v.status.success());
    assert!(text(&v).contains("MISSING"), "{}", text(&v));
}

#[test]
fn a_digest_mismatch_discards_the_download_and_fails() {
    let dir = Dir::new();
    let lock = lock(&dir, &[("cores/a.data", A)]);
    // Same length, other bytes: only the digest can catch it.
    let wrong = b"7z-pretend core archive X".to_vec();
    assert_eq!(wrong.len(), A.len());
    let (base, _) = serve(HashMap::from([("cores/a.data".to_string(), wrong)]));
    let dest = dir.0.join("served");

    let o = run(&lock, &dest, &base, &[]);
    assert!(!o.status.success());
    assert!(text(&o).contains("digest mismatch"), "{}", text(&o));
    assert!(
        !dest.join("cores/a.data").exists(),
        "the wrong file must not be left in place"
    );
    assert!(
        !dest.join("cores/a.part").exists(),
        "the part file must be removed"
    );
}

#[test]
fn a_longer_file_is_rejected_and_a_404_fails() {
    let dir = Dir::new();
    let lock = lock(&dir, &[("cores/a.data", A), ("cores/missing.data", B)]);
    let mut long = A.to_vec();
    long.extend(std::iter::repeat_n(b'x', 10_000));
    let (base, _) = serve(HashMap::from([("cores/a.data".to_string(), long)]));
    let dest = dir.0.join("served");

    let o = run(&lock, &dest, &base, &[]);
    assert!(!o.status.success());
    assert!(!dest.join("cores/a.data").exists());

    // The first entry now serves correctly; the second one is a 404 and fails the run.
    let (base2, _) = serve(HashMap::from([("cores/a.data".to_string(), A.to_vec())]));
    let o2 = run(&lock, &dest, &base2, &[]);
    assert!(!o2.status.success(), "{}", text(&o2));
    assert!(!dest.join("cores/missing.data").exists());
}

#[test]
fn a_plain_http_mirror_that_is_not_loopback_is_refused() {
    let dir = Dir::new();
    let lock = lock(&dir, &[("cores/a.data", A)]);
    let o = run(&lock, &dir.0.join("served"), "http://mirror.example", &[]);
    assert!(!o.status.success());
    assert!(
        text(&o).contains("--base-url must be https"),
        "{}",
        text(&o)
    );
}
