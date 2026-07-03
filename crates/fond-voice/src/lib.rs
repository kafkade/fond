//! Hands-free **voice cook mode** for fond (ROADMAP §13 Phase 8).
//!
//! This crate provides the pieces of voice cook mode, kept independent of any
//! UI so they can be tested exhaustively and reused across front-ends:
//!
//! * [`command`] — a forgiving natural-language grammar that turns a recognized
//!   phrase into a structured [`VoiceCommand`].
//! * [`state`] — [`VoiceCookState`], the cook-mode "brain": it applies commands
//!   to a recipe + schedule and produces the spoken [`VoiceResponse`], including
//!   step navigation, ingredient-quantity queries, and timers.
//! * [`speech`] / [`tts`] / [`listener`] — thin, swappable speech adapters. The
//!   default path is **on-device** (principle #1: local-first, offline-capable);
//!   any external/cloud engine is opt-in via `--tts-cmd` / `--listen-cmd` and is
//!   always clearly labeled and never required.
//!
//! The actual I/O loop (threading a listener against a timer tick and a speaker)
//! lives in the `fond` binary, which owns stdin/stdout.

pub mod command;
pub mod listener;
pub mod speech;
pub mod state;
pub mod tts;

pub use command::{VoiceCommand, parse_command};
pub use listener::{CommandListener, StdinListener};
pub use speech::{Listener, NullSpeaker, SpeakOutcome, Speaker};
pub use state::{VoiceCookState, VoiceResponse};
pub use tts::SystemSpeaker;
