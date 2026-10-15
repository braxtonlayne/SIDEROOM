//! `sr-cli package validate` runs the theme validators on a folder without any server, config or database.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

static N: AtomicU32 = AtomicU32::new(0);

struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "sr-cli-package-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn write(&self, rel: &str, bytes: &[u8]) -> &Self {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
        self
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &[u8] = br#"{"id":"night-owl","name":"Night Owl","version":"1.0.0","author":"Ada","targets":["lounge"],"scheme":"dark"}"#;

fn run(dir: &Dir, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sr-cli"))
        // No configuration may be needed (or consulted) for this command.
        .env_remove("SR_CONFIG")
        .args(["package", "validate"])
        .arg(dir.path())
        .args(extra)
        .output()
        .expect("run sr-cli")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn a_good_theme_passes() {
    let d = Dir::new();
    d.write("theme.json", MANIFEST)
        .write("tokens.json", br##"{"--ui-accent":"#7c5cff"}"##);
    let o = run(&d, &[]);
    assert!(o.status.success(), "{}", stdout(&o));
    assert!(
        stdout(&o).contains("night-owl: 0 error(s)"),
        "{}",
        stdout(&o)
    );
}

#[test]
fn problems_are_listed_and_fail_the_exit_code() {
    let d = Dir::new();
    d.write("theme.json", MANIFEST)
        .write("tokens.json", br##"{"--not-a-token":"#fff"}"##)
        .write("assets/clip.gif", b"GIF89a")
        .write("notes.txt", b"todo");
    let o = run(&d, &[]);
    assert_eq!(o.status.code(), Some(1));
    let out = stdout(&o);
    assert!(out.contains("unknown-token"), "{out}");
    assert!(out.contains("invalid-asset-type"), "{out}");
    assert!(out.contains("notes.txt"), "{out}");
}

#[test]
fn json_output_is_machine_readable() {
    let d = Dir::new();
    d.write("theme.json", MANIFEST)
        .write("tokens.json", br##"{"--not-a-token":"#fff"}"##);
    let o = run(&d, &["--json"]);
    assert_eq!(o.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["valid"], false);
    assert_eq!(v["issues"][0]["code"], "unknown-token");
}

#[test]
fn an_unreadable_folder_is_reported() {
    let d = Dir::new();
    d.write("tokens.json", b"{}");
    let o = run(&d, &[]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        stdout(&o).contains("theme.json is missing"),
        "{}",
        stdout(&o)
    );
    let missing = Command::new(env!("CARGO_BIN_EXE_sr-cli"))
        .args(["package", "validate", "/definitely/not/here"])
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(1));
}
