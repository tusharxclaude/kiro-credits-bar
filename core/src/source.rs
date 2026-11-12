//! Getting a reading from Kiro, with the credit-burn guard from ADR 0001.
//!
//! The command is pinned to `--v2`: on the V3 engine, `/usage` in headless mode is
//! sent to the AI model as a prompt and costs credits. So any surprise (output that
//! does not parse, a timeout, a non-zero exit) halts polling. It is never retried
//! automatically; the user must resume.

use crate::usage::{parse_usage, UsageReading};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The exact arguments. Do not change without re-reading ADR 0001.
pub const USAGE_ARGS: [&str; 4] = ["chat", "--v2", "--no-interactive", "/usage"];

#[derive(Debug, Clone, PartialEq)]
pub enum SourceError {
    /// The CLI could not be started at all (not installed, not on PATH). Safe to retry.
    NotInstalled(String),
    /// The CLI ran but misbehaved: timeout, non-zero exit. Might have reached the model.
    Failed(String),
}

/// Something that can produce the raw text of `/usage`.
pub trait UsageSource {
    fn read(&mut self) -> Result<String, SourceError>;
}

/// Runs the real `kiro-cli`.
pub struct CliSource {
    pub program: PathBuf,
    pub timeout: Duration,
}

impl CliSource {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        CliSource {
            program: program.into(),
            timeout: Duration::from_secs(30),
        }
    }
}

impl UsageSource for CliSource {
    fn read(&mut self) -> Result<String, SourceError> {
        let mut child = Command::new(&self.program)
            .args(USAGE_ARGS)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| SourceError::NotInstalled(e.to_string()))?;

        // Drain both pipes on threads so a full pipe can never stall the child.
        let mut out = child.stdout.take().expect("piped");
        let mut err = child.stderr.take().expect("piped");
        let out_t = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = out.read_to_string(&mut s);
            s
        });
        let err_t = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = err.read_to_string(&mut s);
            s
        });

        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(SourceError::Failed(format!(
                        "timed out after {}s",
                        self.timeout.as_secs()
                    )));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => return Err(SourceError::Failed(e.to_string())),
            }
        };

        let stdout = out_t.join().unwrap_or_default();
        let stderr = err_t.join().unwrap_or_default();
        if !status.success() {
            let hint = stderr
                .lines()
                .chain(stdout.lines())
                .find(|l| !l.trim().is_empty());
            return Err(SourceError::Failed(format!(
                "exited with {status}{}",
                hint.map(|h| format!(": {}", h.trim())).unwrap_or_default()
            )));
        }
        Ok(stdout)
    }
}

/// What one poll produced.
#[derive(Debug, Clone, PartialEq)]
pub enum PollOutcome {
    Reading(UsageReading),
    /// The CLI is missing. Show the last reading as stale; the next poll may try again.
    Unavailable(String),
    /// Polling has stopped and will not resume on its own. Offer manual entry and a Resume button.
    Halted(String),
}

/// Wraps a source with the halt-on-surprise rule.
pub struct Poller<S: UsageSource> {
    source: S,
    halted: Option<String>,
}

impl<S: UsageSource> Poller<S> {
    pub fn new(source: S) -> Self {
        Poller {
            source,
            halted: None,
        }
    }

    /// Access to the source, e.g. to point it at a different `kiro-cli` path.
    pub fn source_mut(&mut self) -> &mut S {
        &mut self.source
    }

    pub fn halted_reason(&self) -> Option<&str> {
        self.halted.as_deref()
    }

    /// The user pressed Resume after fixing the cause.
    pub fn resume(&mut self) {
        self.halted = None;
    }

    pub fn poll(&mut self) -> PollOutcome {
        if let Some(reason) = &self.halted {
            // Do not touch the CLI at all while halted.
            return PollOutcome::Halted(reason.clone());
        }
        let text = match self.source.read() {
            Ok(t) => t,
            Err(SourceError::NotInstalled(why)) => return PollOutcome::Unavailable(why),
            Err(SourceError::Failed(why)) => return self.halt(format!("kiro-cli failed: {why}")),
        };
        match parse_usage(&text) {
            Ok(reading) => PollOutcome::Reading(reading),
            Err(e) => self.halt(e.to_string()),
        }
    }

    fn halt(&mut self, reason: String) -> PollOutcome {
        self.halted = Some(reason.clone());
        PollOutcome::Halted(reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    const PRO_PLUS: &str = include_str!("../../tests/fixtures/usage_pro_plus_org_managed.txt");

    /// A scripted source that counts how many times the CLI was "run".
    struct Fake {
        replies: Vec<Result<String, SourceError>>,
        calls: Rc<Cell<u32>>,
    }

    impl UsageSource for Fake {
        fn read(&mut self) -> Result<String, SourceError> {
            self.calls.set(self.calls.get() + 1);
            if self.replies.len() > 1 {
                self.replies.remove(0)
            } else {
                self.replies[0].clone()
            }
        }
    }

    fn poller(replies: Vec<Result<String, SourceError>>) -> (Poller<Fake>, Rc<Cell<u32>>) {
        let calls = Rc::new(Cell::new(0));
        (
            Poller::new(Fake {
                replies,
                calls: calls.clone(),
            }),
            calls,
        )
    }

    #[test]
    fn pins_the_v2_engine_in_the_arguments() {
        assert!(USAGE_ARGS.contains(&"--v2"));
        assert!(USAGE_ARGS.contains(&"--no-interactive"));
        assert!(!USAGE_ARGS.contains(&"--v3"));
    }

    #[test]
    fn a_good_reply_becomes_a_reading() {
        let (mut p, _) = poller(vec![Ok(PRO_PLUS.to_string())]);
        match p.poll() {
            PollOutcome::Reading(r) => assert_eq!(r.total, 2000.0),
            other => panic!("expected a reading, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_cli_is_retried_on_the_next_poll() {
        let (mut p, calls) = poller(vec![
            Err(SourceError::NotInstalled("no such file".into())),
            Ok(PRO_PLUS.to_string()),
        ]);
        assert!(matches!(p.poll(), PollOutcome::Unavailable(_)));
        assert!(matches!(p.poll(), PollOutcome::Reading(_)));
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn output_that_does_not_parse_halts_and_is_never_retried() {
        // The V3 failure: the model answers in prose.
        let (mut p, calls) = poller(vec![Ok("Run Kiro CLI and type /usage.".to_string())]);
        assert!(matches!(p.poll(), PollOutcome::Halted(_)));
        for _ in 0..5 {
            assert!(matches!(p.poll(), PollOutcome::Halted(_)));
        }
        assert_eq!(calls.get(), 1, "the CLI must not be run again while halted");
    }

    #[test]
    fn timeouts_and_failures_halt_too() {
        let (mut p, calls) = poller(vec![Err(SourceError::Failed("timed out after 30s".into()))]);
        assert!(matches!(p.poll(), PollOutcome::Halted(_)));
        assert!(matches!(p.poll(), PollOutcome::Halted(_)));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn resume_allows_polling_again() {
        let (mut p, calls) = poller(vec![Ok("garbage".to_string()), Ok(PRO_PLUS.to_string())]);
        assert!(matches!(p.poll(), PollOutcome::Halted(_)));
        assert!(p.halted_reason().is_some());
        p.resume();
        assert!(matches!(p.poll(), PollOutcome::Reading(_)));
        assert_eq!(calls.get(), 2);
    }

    // ---- the real process runner, exercised with tiny stand-in scripts ----

    #[cfg(unix)]
    mod process {
        use super::*;
        use std::os::unix::fs::PermissionsExt;
        use std::sync::{Mutex, MutexGuard};

        /// These tests write a script and then run it. If another test forks a process in between,
        /// the child briefly inherits the still-open write handle and Linux refuses to run the
        /// script ("Text file busy"). Running them one at a time avoids that race.
        static SERIAL: Mutex<()> = Mutex::new(());

        fn serial() -> MutexGuard<'static, ()> {
            SERIAL.lock().unwrap_or_else(|e| e.into_inner())
        }

        fn script(name: &str, body: &str) -> PathBuf {
            let dir = std::env::temp_dir().join(format!("kcb-test-{}-{name}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("fake-kiro-cli");
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        #[test]
        fn passes_the_pinned_arguments_and_returns_stdout() {
            let _guard = serial();
            // The stand-in only prints a valid reading if it was given exactly the pinned args.
            let path = script(
                "args",
                r#"[ "$1 $2 $3 $4" = "chat --v2 --no-interactive /usage" ] || { echo wrong args "$@" >&2; exit 9; }
printf 'Estimated Usage | resets on 2026-12-01 | KIRO PRO+\nCredits (10 of 2000 covered in plan), 0.5%%\n'"#,
            );
            let mut src = CliSource::new(path);
            let text = src.read().unwrap();
            assert!(parse_usage(&text).is_ok());
        }

        #[test]
        fn a_missing_program_is_not_installed() {
            let _guard = serial();
            let mut src = CliSource::new("/definitely/not/here/kiro-cli");
            assert!(matches!(src.read(), Err(SourceError::NotInstalled(_))));
        }

        #[test]
        fn a_nonzero_exit_is_a_failure_with_a_hint() {
            let _guard = serial();
            let path = script("exit", "echo 'not logged in' >&2; exit 1");
            let err = CliSource::new(path).read().unwrap_err();
            match err {
                SourceError::Failed(msg) => assert!(msg.contains("not logged in"), "{msg}"),
                other => panic!("{other:?}"),
            }
        }

        #[test]
        fn a_hung_cli_is_killed_at_the_timeout() {
            let _guard = serial();
            let path = script("hang", "sleep 30");
            let mut src = CliSource::new(path);
            src.timeout = Duration::from_millis(300);
            let started = Instant::now();
            let err = src.read().unwrap_err();
            assert!(matches!(err, SourceError::Failed(m) if m.contains("timed out")));
            assert!(started.elapsed() < Duration::from_secs(5));
        }
    }
}
