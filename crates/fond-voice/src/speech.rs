//! Speech backend abstractions.
//!
//! fond is **local-first** (principle #1): the on-device path is the default and
//! the product works fully offline. Any cloud speech is *optional, clearly
//! labeled, and never required* — it only ever happens when the user explicitly
//! points [`--listen-cmd`](crate) / `--tts-cmd` at an external tool of their
//! choosing. These traits keep the cook-state brain independent of whichever
//! backend is wired in, so a real on-device recognizer (whisper.cpp, Vosk, the
//! platform speech APIs) drops in without touching the grammar or state code.

/// Result of attempting to speak a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeakOutcome {
    /// The text was handed to the speech engine successfully.
    Spoken,
    /// No speech engine was available; the caller should fall back to text.
    Unavailable,
}

/// A text-to-speech backend.
pub trait Speaker {
    /// Speak a line of text aloud. Should not block longer than the utterance.
    fn speak(&mut self, text: &str) -> SpeakOutcome;

    /// Whether this speaker is currently usable.
    fn is_available(&self) -> bool;

    /// A short, user-facing description of this backend, for the mode banner
    /// (so cloud/external engines are always clearly labeled).
    fn describe(&self) -> String;
}

/// A speech-to-text (or typed-input) backend that yields recognized phrases.
pub trait Listener {
    /// Block until the next phrase is available. Returns `None` at end of input
    /// (EOF / recognizer exit), which the caller treats as "stop listening".
    fn next_phrase(&mut self) -> Option<String>;

    /// A short, user-facing description of this backend for the mode banner.
    fn describe(&self) -> String;
}

/// A speaker that discards everything — used for `--no-speak` (text-only) mode.
/// The session still prints every line, so it remains fully usable.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSpeaker;

impl Speaker for NullSpeaker {
    fn speak(&mut self, _text: &str) -> SpeakOutcome {
        SpeakOutcome::Unavailable
    }

    fn is_available(&self) -> bool {
        false
    }

    fn describe(&self) -> String {
        "text-only (speech off)".to_string()
    }
}

/// Whether an executable named `exe` is resolvable on the current `PATH`.
pub(crate) fn in_path(exe: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(exe).is_file())
}
