//! Hands-free voice cook mode: the I/O loop that wires fond-voice's brain and
//! speech adapters to stdin/stdout.
//!
//! Design: a background thread drives the [`Listener`] and forwards recognized
//! phrases over a channel. The main loop `recv_timeout`s on that channel so it
//! can tick running timers (and announce fired ones) even while waiting for the
//! next command — this is what makes timers work hands-free. When no speaker is
//! available we degrade gracefully to on-screen text; the mode stays fully
//! usable by typing commands.

use std::io::{BufRead, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use fond_domain::Recipe;
use fond_timeline::ScheduledTimeline;
use fond_voice::{
    CommandListener, Listener, NullSpeaker, Speaker, StdinListener, SystemSpeaker, VoiceCookState,
    parse_command,
};

use crate::VoiceOptions;

/// Outcome of a voice cook session, for the shared cook-log flow.
pub struct VoiceSession {
    pub steps_completed: usize,
    pub total_steps: usize,
    pub duration: Duration,
    /// Reads the answers to the post-session cook-log prompts. With the default
    /// stdin listener this pulls from the listener channel (so stdin has a
    /// single consumer); with an external `--listen-cmd` recognizer, stdin is
    /// free and this reads it directly.
    pub prompt_reader: Box<dyn FnMut() -> Option<String>>,
}

/// Build the configured text-to-speech backend, honoring `--no-speak` and
/// `--tts-cmd`, and falling back to text-only when nothing is available.
fn build_speaker(voice: &VoiceOptions) -> Box<dyn Speaker> {
    if voice.no_speak {
        return Box::new(NullSpeaker);
    }
    if let Some(cmd) = &voice.tts_cmd {
        let speaker = SystemSpeaker::from_command(cmd);
        if speaker.is_available() {
            return Box::new(speaker);
        }
        eprintln!("  Note: --tts-cmd '{cmd}' not found on PATH; continuing text-only.");
        return Box::new(NullSpeaker);
    }
    match SystemSpeaker::detect() {
        Some(speaker) => Box::new(speaker),
        None => Box::new(NullSpeaker),
    }
}

/// Speak (best-effort) and always print a line, so text is a graceful fallback.
fn emit(speaker: &mut dyn Speaker, text: &str) {
    println!("  🔊 {text}");
    let _ = std::io::stdout().flush();
    speaker.speak(text);
}

/// Run hands-free voice cook mode. Returns session stats for the cook log.
pub fn run(
    recipe: Recipe,
    schedule: Option<ScheduledTimeline>,
    voice: &VoiceOptions,
) -> Result<VoiceSession> {
    let mut speaker = build_speaker(voice);
    let mut state = VoiceCookState::new(recipe, schedule);
    let started = Instant::now();

    // Describe the active backends up front so cloud/external ones are clearly
    // labeled (AI governance principle #6 / local-first #1).
    let listen_desc = match &voice.listen_cmd {
        Some(cmd) => format!("external recognizer: '{cmd}'"),
        None => StdinListener::new().describe(),
    };
    println!();
    println!("  ┌─ fond voice cook mode ─ hands-free, local-first ─┐");
    println!("  │ speech out: {}", speaker.describe());
    println!("  │ speech in:  {listen_desc}");
    println!("  │ Everything runs on your machine. Say 'help' for commands,");
    println!("  │ 'quit' to exit. (Type commands if you prefer.)");
    println!("  └──────────────────────────────────────────────────┘");

    emit(speaker.as_mut(), &state.intro());

    // Spawn the listener on a background thread; forward phrases over a channel
    // so the main loop can tick timers while waiting for input.
    let using_stdin_listener = voice.listen_cmd.is_none();
    let (tx, rx) = mpsc::channel::<String>();
    let listen_cmd = voice.listen_cmd.clone();
    let listener_handle = thread::spawn(move || {
        let mut listener: Box<dyn Listener> = match listen_cmd {
            Some(cmd) => match CommandListener::spawn(&cmd) {
                Ok(l) => Box::new(l),
                Err(e) => {
                    eprintln!(
                        "  Could not start recognizer '{cmd}': {e}. Falling back to typed input."
                    );
                    Box::new(StdinListener::new())
                }
            },
            None => Box::new(StdinListener::new()),
        };
        while let Some(phrase) = listener.next_phrase() {
            if tx.send(phrase).is_err() {
                break;
            }
        }
    });

    // Main loop: react to phrases, tick timers on idle.
    loop {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(phrase) => {
                let trimmed = phrase.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match parse_command(trimmed) {
                    Some(cmd) => {
                        let response = state.apply(cmd);
                        emit(speaker.as_mut(), &response.speech);
                        if response.should_quit {
                            break;
                        }
                    }
                    None => {
                        emit(
                            speaker.as_mut(),
                            "Sorry, I didn't catch that. Say 'help' to hear the commands.",
                        );
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                for announcement in state.tick() {
                    print!("\x07"); // audible terminal bell
                    emit(speaker.as_mut(), &announcement);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // Input ended (EOF / recognizer exited): finish gracefully.
                break;
            }
        }
    }

    // Build the post-session prompt reader. When the default stdin listener is
    // in use it is the sole owner of stdin, so route the cook-log prompt answers
    // through the same channel to keep stdin single-consumer (the detached
    // listener thread keeps forwarding lines, and its live `tx` keeps `rx.recv`
    // blocking rather than disconnecting). With an external recognizer, stdin is
    // untouched, so read it directly.
    drop(listener_handle); // detach; the thread lives until its input closes
    let prompt_reader: Box<dyn FnMut() -> Option<String>> = if using_stdin_listener {
        Box::new(move || rx.recv().ok())
    } else {
        Box::new(|| {
            let mut buf = String::new();
            match std::io::stdin().lock().read_line(&mut buf) {
                Ok(0) => None,
                Ok(_) => Some(buf.trim().to_string()),
                Err(_) => None,
            }
        })
    };

    Ok(VoiceSession {
        steps_completed: state.steps_completed(),
        total_steps: state.total_steps(),
        duration: started.elapsed(),
        prompt_reader,
    })
}
