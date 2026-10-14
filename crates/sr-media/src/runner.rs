//! The `CommandRunner` port: run an external tool with an argument vector, a timeout and
//! bounded output. The production [`ProcessRunner`] never uses a shell; tests use
//! [`FakeRunner`] with canned output.

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;

/// One external command: a program and an argument vector. No shell is ever involved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    /// Kill the process after this long.
    pub timeout: Duration,
    /// Captured stdout beyond this many bytes is dropped and flagged.
    pub max_stdout_bytes: usize,
    /// Only the last this-many bytes of stderr are kept (the useful part of an ffmpeg log).
    pub max_stderr_bytes: usize,
    pub cwd: Option<PathBuf>,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            timeout: Duration::from_secs(60),
            max_stdout_bytes: 4 * 1024 * 1024,
            max_stderr_bytes: 16 * 1024,
            cwd: None,
        }
    }
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    #[must_use]
    pub fn with_max_stdout(mut self, bytes: usize) -> Self {
        self.max_stdout_bytes = bytes;
        self
    }
    #[must_use]
    pub fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd = Some(cwd);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_tail: String,
    pub timed_out: bool,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out
    }
}

/// What the streaming callback wants done after a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Continue,
    /// Kill the process now (cancel).
    Kill,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitInfo {
    pub exit_code: Option<i32>,
    pub stderr_tail: String,
    pub timed_out: bool,
    /// The callback asked for a kill.
    pub killed: bool,
}

impl ExitInfo {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out && !self.killed
    }
}

#[derive(Debug, Error)]
pub enum RunError {
    #[error("could not start {program:?}: {message}")]
    Spawn { program: String, message: String },
    #[error("i/o error while running the command: {0}")]
    Io(String),
}

pub trait CommandRunner: Send + Sync {
    /// Run to completion, capturing (bounded) stdout.
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, RunError>;
    /// Run, handing each stdout line to `on_line` as it appears (ffmpeg `-progress pipe:1`).
    fn run_streaming(
        &self,
        spec: &CommandSpec,
        on_line: &mut dyn FnMut(&str) -> Control,
    ) -> Result<ExitInfo, RunError>;
}

/// Spawns real processes.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessRunner;

fn read_tail(mut reader: impl Read, cap: usize) -> String {
    let mut tail: Vec<u8> = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                tail.extend_from_slice(&buf[..n]);
                if tail.len() > cap {
                    let excess = tail.len() - cap;
                    tail.drain(..excess);
                }
            }
        }
    }
    String::from_utf8_lossy(&tail).into_owned()
}

impl ProcessRunner {
    /// Spawn, run `pump` over stdout under a watchdog, then reap the child.
    fn supervise<R>(
        spec: &CommandSpec,
        pump: impl FnOnce(std::process::ChildStdout) -> (R, bool),
    ) -> Result<(R, ExitInfo), RunError> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = &spec.cwd {
            cmd.current_dir(cwd);
        }
        let mut child = cmd.spawn().map_err(|e| RunError::Spawn {
            program: spec.program.clone(),
            message: e.to_string(),
        })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RunError::Io("no stdout pipe".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| RunError::Io("no stderr pipe".into()))?;
        let cap = spec.max_stderr_bytes;
        let stderr_thread = std::thread::spawn(move || read_tail(stderr, cap));

        let child: Arc<Mutex<Child>> = Arc::new(Mutex::new(child));
        let timed_out = Arc::new(AtomicBool::new(false));
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let watchdog = {
            let child = Arc::clone(&child);
            let timed_out = Arc::clone(&timed_out);
            let timeout = spec.timeout;
            std::thread::spawn(move || {
                if let Err(RecvTimeoutError::Timeout) = done_rx.recv_timeout(timeout) {
                    timed_out.store(true, Ordering::SeqCst);
                    if let Ok(mut c) = child.lock() {
                        let _ = c.kill();
                    }
                }
            })
        };

        let (result, want_kill) = pump(stdout);
        if want_kill && let Ok(mut c) = child.lock() {
            let _ = c.kill();
        }
        // Reap, still bounded by the deadline through the watchdog.
        let started = Instant::now();
        let status = loop {
            let polled = child
                .lock()
                .map_err(|_| RunError::Io("child lock poisoned".into()))?
                .try_wait()
                .map_err(|e| RunError::Io(e.to_string()))?;
            if let Some(s) = polled {
                break s;
            }
            if started.elapsed() > spec.timeout + Duration::from_secs(5) {
                timed_out.store(true, Ordering::SeqCst);
                if let Ok(mut c) = child.lock() {
                    let _ = c.kill();
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        drop(done_tx);
        let _ = watchdog.join();
        let stderr_tail = stderr_thread.join().unwrap_or_default();
        Ok((
            result,
            ExitInfo {
                exit_code: status.code(),
                stderr_tail,
                timed_out: timed_out.load(Ordering::SeqCst),
                killed: want_kill,
            },
        ))
    }
}

impl CommandRunner for ProcessRunner {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, RunError> {
        let cap = spec.max_stdout_bytes;
        let ((stdout, truncated), info) = Self::supervise(spec, |out| {
            let mut buf = Vec::new();
            let mut limited = out.take(cap as u64 + 1);
            let _ = limited.read_to_end(&mut buf);
            let truncated = buf.len() > cap;
            buf.truncate(cap);
            // Drain the rest so the child is never blocked on a full pipe.
            let mut rest = limited.into_inner();
            let _ = std::io::copy(&mut rest, &mut std::io::sink());
            ((buf, truncated), false)
        })?;
        Ok(CommandOutput {
            exit_code: info.exit_code,
            stdout,
            stdout_truncated: truncated,
            stderr_tail: info.stderr_tail,
            timed_out: info.timed_out,
        })
    }

    fn run_streaming(
        &self,
        spec: &CommandSpec,
        on_line: &mut dyn FnMut(&str) -> Control,
    ) -> Result<ExitInfo, RunError> {
        let ((), info) = Self::supervise(spec, |out| {
            let mut reader = BufReader::new(out);
            let mut line = Vec::new();
            loop {
                line.clear();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => return ((), false),
                    Ok(_) => {
                        let text = String::from_utf8_lossy(&line);
                        if on_line(text.trim_end_matches(['\r', '\n'])) == Control::Kill {
                            return ((), true);
                        }
                    }
                }
            }
        })?;
        Ok(info)
    }
}

/// Canned result for [`FakeRunner`].
#[derive(Debug, Clone, Default)]
pub struct FakeResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl FakeResult {
    pub fn ok(stdout: impl Into<String>) -> Self {
        Self {
            exit_code: Some(0),
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }
    pub fn fail(code: i32, stderr: impl Into<String>) -> Self {
        Self {
            exit_code: Some(code),
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }
}

type FakeHandler = Box<dyn FnMut(&CommandSpec) -> FakeResult + Send>;

/// Test double: records every command and answers from a closure.
pub struct FakeRunner {
    handler: Mutex<FakeHandler>,
    calls: Mutex<Vec<CommandSpec>>,
}

impl FakeRunner {
    pub fn new(handler: impl FnMut(&CommandSpec) -> FakeResult + Send + 'static) -> Self {
        Self {
            handler: Mutex::new(Box::new(handler)),
            calls: Mutex::new(Vec::new()),
        }
    }
    /// Always answer with `stdout` and exit code 0.
    pub fn fixed(stdout: impl Into<String>) -> Self {
        let stdout = stdout.into();
        Self::new(move |_| FakeResult::ok(stdout.clone()))
    }
    pub fn calls(&self) -> Vec<CommandSpec> {
        self.calls.lock().map(|c| c.clone()).unwrap_or_default()
    }
    fn answer(&self, spec: &CommandSpec) -> FakeResult {
        if let Ok(mut c) = self.calls.lock() {
            c.push(spec.clone());
        }
        match self.handler.lock() {
            Ok(mut h) => h(spec),
            Err(_) => FakeResult::fail(-1, "handler poisoned"),
        }
    }
}

impl CommandRunner for FakeRunner {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, RunError> {
        let r = self.answer(spec);
        let mut stdout = r.stdout.into_bytes();
        let truncated = stdout.len() > spec.max_stdout_bytes;
        stdout.truncate(spec.max_stdout_bytes);
        Ok(CommandOutput {
            exit_code: r.exit_code,
            stdout,
            stdout_truncated: truncated,
            stderr_tail: r.stderr,
            timed_out: false,
        })
    }

    fn run_streaming(
        &self,
        spec: &CommandSpec,
        on_line: &mut dyn FnMut(&str) -> Control,
    ) -> Result<ExitInfo, RunError> {
        let r = self.answer(spec);
        let mut killed = false;
        for line in r.stdout.lines() {
            if on_line(line) == Control::Kill {
                killed = true;
                break;
            }
        }
        Ok(ExitInfo {
            exit_code: if killed { None } else { r.exit_code },
            stderr_tail: r.stderr,
            timed_out: false,
            killed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_runner_records_and_truncates() {
        let r = FakeRunner::fixed("0123456789");
        let spec = CommandSpec::new("ffprobe", vec!["-i".into(), "x".into()]).with_max_stdout(4);
        let out = r.run(&spec).unwrap();
        assert!(out.stdout_truncated);
        assert_eq!(out.stdout, b"0123");
        assert_eq!(r.calls().len(), 1);
    }

    #[test]
    fn fake_streaming_kill() {
        let r = FakeRunner::fixed("a\nb\nc\n");
        let mut seen = Vec::new();
        let info = r
            .run_streaming(&CommandSpec::new("x", vec![]), &mut |l| {
                seen.push(l.to_string());
                if l == "b" {
                    Control::Kill
                } else {
                    Control::Continue
                }
            })
            .unwrap();
        assert!(info.killed && !info.success());
        assert_eq!(seen, ["a", "b"]);
    }

    #[test]
    fn process_runner_reports_missing_program() {
        let spec = CommandSpec::new("definitely-not-a-real-program-sr", vec![]);
        assert!(matches!(
            ProcessRunner.run(&spec),
            Err(RunError::Spawn { .. })
        ));
    }
}
