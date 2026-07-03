//! Speech-to-text (and typed-input) listener backends.
//!
//! Two backends ship:
//!
//! * [`StdinListener`] — reads phrases line-by-line from standard input. This is
//!   the always-available, zero-dependency fallback: a user can drive cook mode
//!   by typing, and — crucially — any on-device recognizer that prints
//!   recognized text to a pipe becomes hands-free input for free
//!   (`recognizer | fond cook stew --voice`).
//! * [`CommandListener`] — spawns a user-chosen recognizer process
//!   (`--listen-cmd`) and reads one recognized phrase per line from its stdout.
//!   This is the explicit, clearly-labeled integration point for a local STT
//!   engine (whisper.cpp, Vosk) — or, at the user's discretion, a cloud one.
//!
//! Neither backend performs any network I/O itself; fond never listens to a
//! microphone or contacts a service unless the user wires one in.

use std::io::{self, BufRead, BufReader};
use std::process::{Child, ChildStdout, Command, Stdio};

use crate::speech::Listener;

/// Reads recognized phrases from standard input, one per line.
pub struct StdinListener {
    stdin: io::Stdin,
}

impl Default for StdinListener {
    fn default() -> Self {
        Self { stdin: io::stdin() }
    }
}

impl StdinListener {
    /// Create a listener over the process's standard input.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Listener for StdinListener {
    fn next_phrase(&mut self) -> Option<String> {
        let mut line = String::new();
        match self.stdin.lock().read_line(&mut line) {
            Ok(0) => None, // EOF
            Ok(_) => Some(line.trim().to_string()),
            Err(_) => None,
        }
    }

    fn describe(&self) -> String {
        "typed / piped input (say a command and press enter, or pipe a recognizer)".to_string()
    }
}

/// Spawns an external recognizer command and reads recognized phrases from its
/// stdout, one per line.
pub struct CommandListener {
    child: Child,
    reader: BufReader<ChildStdout>,
    label: String,
}

impl CommandListener {
    /// Spawn `spec` (a shell-style command, split on whitespace) as the
    /// recognizer. Its stdout is consumed as a stream of recognized phrases.
    pub fn spawn(spec: &str) -> io::Result<Self> {
        let mut parts = spec.split_whitespace();
        let program = parts
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty --listen-cmd"))?;
        let args: Vec<&str> = parts.collect();

        let mut child = Command::new(program)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("recognizer produced no stdout"))?;

        Ok(Self {
            child,
            reader: BufReader::new(stdout),
            label: spec.to_string(),
        })
    }
}

impl Listener for CommandListener {
    fn next_phrase(&mut self) -> Option<String> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => None,
            Ok(_) => Some(line.trim().to_string()),
            Err(_) => None,
        }
    }

    fn describe(&self) -> String {
        format!("external recognizer: '{}'", self.label)
    }
}

impl Drop for CommandListener {
    fn drop(&mut self) {
        // Best-effort: stop the recognizer when cook mode ends.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
