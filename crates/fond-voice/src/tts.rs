//! On-device text-to-speech via the platform's native speech command.
//!
//! This shells out to whatever local TTS binary the OS ships — `say` on macOS,
//! `spd-say`/`espeak`/`espeak-ng` on Linux, PowerShell's `System.Speech` on
//! Windows — so speech stays fully on-device with no bundled model and no
//! network. If nothing is found, [`SystemSpeaker::detect`] returns `None` and
//! the caller degrades gracefully to on-screen text.

use std::process::{Command, Stdio};

use crate::speech::{SpeakOutcome, Speaker, in_path};

/// A text-to-speech backend that invokes a local command per utterance.
pub struct SystemSpeaker {
    program: String,
    leading_args: Vec<String>,
    /// True when the user supplied the command explicitly (so we label it as
    /// user-configured rather than claiming it is the built-in on-device path).
    custom: bool,
    /// When set, the spoken text is embedded into this template (replacing
    /// `{text}`) and run through the shell — needed for Windows PowerShell.
    shell_template: Option<String>,
}

impl SystemSpeaker {
    /// Detect the platform's default on-device TTS command, if present.
    pub fn detect() -> Option<Self> {
        if cfg!(target_os = "macos") && in_path("say") {
            return Some(Self {
                program: "say".to_string(),
                leading_args: Vec::new(),
                custom: false,
                shell_template: None,
            });
        }
        for exe in ["spd-say", "espeak-ng", "espeak"] {
            if in_path(exe) {
                return Some(Self {
                    program: exe.to_string(),
                    leading_args: Vec::new(),
                    custom: false,
                    shell_template: None,
                });
            }
        }
        if cfg!(target_os = "windows") && in_path("powershell") {
            return Some(Self {
                program: "powershell".to_string(),
                leading_args: Vec::new(),
                custom: false,
                shell_template: Some(
                    "Add-Type -AssemblyName System.Speech; \
                     (New-Object System.Speech.Synthesis.SpeechSynthesizer).Speak('{text}')"
                        .to_string(),
                ),
            });
        }
        None
    }

    /// Build a speaker from a user-supplied command spec (e.g. `"say -v Alex"`).
    /// The recognized/spoken text is appended as the final argument.
    pub fn from_command(spec: &str) -> Self {
        let mut parts = spec.split_whitespace().map(str::to_string);
        let program = parts.next().unwrap_or_default();
        Self {
            program,
            leading_args: parts.collect(),
            custom: true,
            shell_template: None,
        }
    }
}

impl Speaker for SystemSpeaker {
    fn speak(&mut self, text: &str) -> SpeakOutcome {
        let status = if let Some(template) = &self.shell_template {
            // Windows PowerShell path: embed sanitized text into the template.
            let safe = text.replace('\'', "''");
            let command = template.replace("{text}", &safe);
            Command::new(&self.program)
                .args(["-NoProfile", "-Command", &command])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
        } else {
            Command::new(&self.program)
                .args(&self.leading_args)
                .arg(text)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
        };

        match status {
            Ok(s) if s.success() => SpeakOutcome::Spoken,
            _ => SpeakOutcome::Unavailable,
        }
    }

    fn is_available(&self) -> bool {
        in_path(&self.program)
    }

    fn describe(&self) -> String {
        if self.custom {
            format!("user-configured text-to-speech via '{}'", self.program)
        } else {
            format!("on-device text-to-speech via '{}'", self.program)
        }
    }
}
