//! Command-line parsing for `sr-transcoder` (no external parser; the surface is small).

use std::path::PathBuf;
use thiserror::Error;

pub const USAGE: &str = "\
sr-transcoder: SIDE ROOM transcode worker

USAGE:
    sr-transcoder --server <URL> [OPTIONS]
    sr-transcoder --check [--ffmpeg <PATH>]

OPTIONS:
    --server <URL>         Base URL of the SIDE ROOM server (https://...)
    --token-file <PATH>    File holding the worker token (preferred)
    --token <TOKEN>        Worker token (visible in the process list; prefer the file or
                           the SIDEROOM_WORKER_TOKEN environment variable)
    --concurrency <N>      Jobs at once (default 1)
    --work-dir <PATH>      Scratch directory (default ./sr-transcoder-work)
    --ffmpeg <PATH>        ffmpeg binary (default: ffmpeg on PATH)
    --name <NAME>          Worker name shown in Admin (default: hostname-like 'worker')
    --force-cpu            Never use NVENC
    --check                Print detected encoders and exit
    -h, --help             This text
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cli {
    pub server: Option<String>,
    pub token: Option<String>,
    pub concurrency: u32,
    pub work_dir: PathBuf,
    pub ffmpeg: String,
    pub name: String,
    pub force_cpu: bool,
    pub check: bool,
    pub help: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CliError {
    #[error("unknown option {0}")]
    Unknown(String),
    #[error("option {0} needs a value")]
    MissingValue(String),
    #[error("bad value for {0}")]
    BadValue(String),
    #[error("--server is required (or use --check)")]
    NoServer,
    #[error("no worker token: use --token-file, --token or SIDEROOM_WORKER_TOKEN")]
    NoToken,
    #[error("server URL must start with https:// (http:// is allowed only for localhost)")]
    InsecureServer,
    #[error("could not read the token file: {0}")]
    TokenFile(String),
}

/// `https://` always; `http://` only for a loopback host (development).
pub fn check_server_url(url: &str) -> Result<(), CliError> {
    if url.starts_with("https://") {
        return Ok(());
    }
    let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"];
    if local.iter().any(|p| {
        url.strip_prefix(p)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with([':', '/']))
    }) {
        return Ok(());
    }
    Err(CliError::InsecureServer)
}

/// Parse `args` (without the program name). `env` reads environment variables and
/// `read_file` reads the token file, so the function stays pure for tests.
pub fn parse(
    args: impl IntoIterator<Item = String>,
    env: &dyn Fn(&str) -> Option<String>,
    read_file: &dyn Fn(&str) -> Result<String, String>,
) -> Result<Cli, CliError> {
    let mut cli = Cli {
        server: None,
        token: None,
        concurrency: 1,
        work_dir: PathBuf::from("sr-transcoder-work"),
        ffmpeg: "ffmpeg".to_string(),
        name: "worker".to_string(),
        force_cpu: false,
        check: false,
        help: false,
    };
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| {
            it.next()
                .ok_or_else(|| CliError::MissingValue(name.to_string()))
        };
        match a.as_str() {
            "--server" => cli.server = Some(value("--server")?.trim_end_matches('/').to_string()),
            "--token" => cli.token = Some(value("--token")?),
            "--token-file" => {
                let path = value("--token-file")?;
                let t = read_file(&path).map_err(CliError::TokenFile)?;
                cli.token = Some(t.trim().to_string());
            }
            "--concurrency" => {
                cli.concurrency = value("--concurrency")?
                    .parse::<u32>()
                    .ok()
                    .filter(|n| (1..=16).contains(n))
                    .ok_or_else(|| CliError::BadValue("--concurrency".into()))?;
            }
            "--work-dir" => cli.work_dir = PathBuf::from(value("--work-dir")?),
            "--ffmpeg" => cli.ffmpeg = value("--ffmpeg")?,
            "--name" => {
                let n = value("--name")?;
                if n.is_empty() || n.len() > 64 || n.chars().any(char::is_control) {
                    return Err(CliError::BadValue("--name".into()));
                }
                cli.name = n;
            }
            "--force-cpu" => cli.force_cpu = true,
            "--check" => cli.check = true,
            "-h" | "--help" => cli.help = true,
            other => return Err(CliError::Unknown(other.to_string())),
        }
    }
    if cli.token.is_none() {
        cli.token = env("SIDEROOM_WORKER_TOKEN").filter(|t| !t.is_empty());
    }
    if !cli.check && !cli.help {
        let server = cli.server.as_deref().ok_or(CliError::NoServer)?;
        check_server_url(server)?;
        if cli.token.as_deref().is_none_or(str::is_empty) {
            return Err(CliError::NoToken);
        }
    }
    Ok(cli)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str], env: Option<&str>) -> Result<Cli, CliError> {
        parse(
            args.iter().map(|s| (*s).to_string()),
            &|k| {
                (k == "SIDEROOM_WORKER_TOKEN")
                    .then(|| env.map(String::from))
                    .flatten()
            },
            &|p| {
                if p == "tok.txt" {
                    Ok("  filetoken\n".to_string())
                } else {
                    Err("no such file".to_string())
                }
            },
        )
    }

    #[test]
    fn full_command_line() {
        let c = run(
            &[
                "--server",
                "https://sr.example/",
                "--token-file",
                "tok.txt",
                "--concurrency",
                "2",
                "--force-cpu",
                "--name",
                "gpu1",
            ],
            None,
        )
        .unwrap();
        assert_eq!(c.server.as_deref(), Some("https://sr.example"));
        assert_eq!(c.token.as_deref(), Some("filetoken"));
        assert_eq!(c.concurrency, 2);
        assert!(c.force_cpu && !c.check);
        assert_eq!(c.name, "gpu1");
    }

    #[test]
    fn token_from_environment() {
        let c = run(&["--server", "https://sr.example"], Some("envtoken")).unwrap();
        assert_eq!(c.token.as_deref(), Some("envtoken"));
        assert_eq!(
            run(&["--server", "https://sr.example"], None),
            Err(CliError::NoToken)
        );
    }

    #[test]
    fn check_needs_nothing_else() {
        let c = run(&["--check", "--ffmpeg", "/opt/ffmpeg"], None).unwrap();
        assert!(c.check);
        assert_eq!(c.ffmpeg, "/opt/ffmpeg");
        assert!(run(&["--help"], None).unwrap().help);
    }

    #[test]
    fn errors() {
        assert_eq!(run(&[], None), Err(CliError::NoServer));
        assert_eq!(
            run(&["--bogus"], None),
            Err(CliError::Unknown("--bogus".into()))
        );
        assert_eq!(
            run(&["--server"], None),
            Err(CliError::MissingValue("--server".into()))
        );
        assert_eq!(
            run(
                &[
                    "--server",
                    "https://x",
                    "--token",
                    "t",
                    "--concurrency",
                    "0"
                ],
                None
            ),
            Err(CliError::BadValue("--concurrency".into()))
        );
        assert_eq!(
            run(&["--server", "https://x", "--token-file", "nope"], None),
            Err(CliError::TokenFile("no such file".into()))
        );
        assert_eq!(
            run(&["--server", "http://example.com", "--token", "t"], None),
            Err(CliError::InsecureServer)
        );
        assert!(run(&["--server", "http://localhost:8080", "--token", "t"], None).is_ok());
        assert!(run(&["--server", "http://127.0.0.1", "--token", "t"], None).is_ok());
        assert_eq!(
            run(
                &["--server", "http://localhost.evil.com", "--token", "t"],
                None
            ),
            Err(CliError::InsecureServer)
        );
    }
}
