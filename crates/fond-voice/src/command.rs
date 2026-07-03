//! Voice command grammar.
//!
//! Turns a recognized natural-language phrase (from any speech-to-text backend,
//! or typed text) into a structured [`VoiceCommand`]. This is pure, deterministic
//! logic with no I/O — the heart of hands-free cook mode — so it can be tested
//! exhaustively and reused across every front-end.
//!
//! The grammar is intentionally forgiving: real kitchen speech is noisy and
//! imprecise, so we match on salient keywords and phrases rather than a rigid
//! syntax. Unrecognized input returns `None` so callers can say "I didn't catch
//! that" instead of guessing.

use fond_timeline::duration::parse_duration_str;

/// A structured, recognized voice command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceCommand {
    /// Advance to the next step (marking the current one done).
    Next,
    /// Go back to the previous step.
    Previous,
    /// Repeat / re-read the current step aloud.
    Repeat,
    /// Preview the next step without advancing.
    WhatsNext,
    /// Jump to a specific step number (1-based).
    GoToStep(usize),
    /// "How much / how many <ingredient>?" — answer a quantity query.
    HowMuch(String),
    /// Read back the full ingredient list.
    ListIngredients,
    /// Start the current step's timer (duration inferred from the step).
    StartTimer,
    /// Start a timer with an explicit duration.
    SetTimer {
        /// Total duration in seconds.
        seconds: u64,
        /// Optional spoken label (e.g. "pasta").
        label: Option<String>,
    },
    /// Stop / cancel the active timer(s).
    StopTimer,
    /// Pause the active timer(s).
    PauseTimer,
    /// Resume the paused timer(s).
    ResumeTimer,
    /// "How much time is left?" — report remaining time on active timers.
    TimerStatus,
    /// List the commands the user can say.
    Help,
    /// Leave voice cook mode.
    Quit,
}

/// Normalize raw recognized text for matching: lowercase, strip most
/// punctuation, and collapse runs of whitespace.
fn normalize(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_space = false;
    for ch in input.chars() {
        if ch.is_alphanumeric() {
            last_was_space = false;
            out.push(ch.to_ascii_lowercase());
        } else if ch == '\'' || ch == '\u{2019}' {
            // Drop apostrophes so contractions collapse ("what's" -> "whats").
            continue;
        } else {
            // Treat any other punctuation/whitespace as a separator.
            if last_was_space {
                continue;
            }
            last_was_space = true;
            out.push(' ');
        }
    }
    out.trim().to_string()
}

/// Whether `text` contains `needle` as a whole-word/phrase match.
fn has_phrase(text: &str, needle: &str) -> bool {
    // `text` is already space-normalized; pad both sides so we match whole words.
    let padded = format!(" {text} ");
    padded.contains(&format!(" {needle} "))
}

/// Try to read a small English number word or digit sequence as a usize.
fn word_to_number(word: &str) -> Option<usize> {
    if let Ok(n) = word.parse::<usize>() {
        return Some(n);
    }
    let n = match word {
        "one" | "first" => 1,
        "two" | "second" | "to" | "too" => 2, // common STT homophones
        "three" | "third" => 3,
        "four" | "fourth" | "for" => 4,
        "five" | "fifth" => 5,
        "six" | "sixth" => 6,
        "seven" | "seventh" => 7,
        "eight" | "eighth" | "ate" => 8,
        "nine" | "ninth" => 9,
        "ten" | "tenth" => 10,
        "eleven" => 11,
        "twelve" => 12,
        _ => return None,
    };
    Some(n)
}

/// Parse a recognized phrase into a [`VoiceCommand`].
///
/// Returns `None` when nothing matches, so the caller can prompt the user to
/// repeat rather than acting on a misheard command.
pub fn parse_command(input: &str) -> Option<VoiceCommand> {
    let text = normalize(input);
    if text.is_empty() {
        return None;
    }

    // --- Timers (checked before navigation: a duration phrase like "set a
    // timer for 2 minutes" also contains number homophones, so handle first).
    // Stop/pause/resume/status are checked before the generic set/start so that
    // "stop timer" etc. are never misread as a request to start one. ---
    if has_phrase(&text, "stop timer")
        || has_phrase(&text, "cancel timer")
        || is_any(&text, &["stop the timer", "cancel the timer"])
    {
        return Some(VoiceCommand::StopTimer);
    }
    if has_phrase(&text, "pause timer") || is_any(&text, &["pause the timer", "hold the timer"]) {
        return Some(VoiceCommand::PauseTimer);
    }
    if has_phrase(&text, "resume timer")
        || is_any(&text, &["resume the timer", "unpause the timer"])
    {
        return Some(VoiceCommand::ResumeTimer);
    }
    if is_timer_status(&text) {
        return Some(VoiceCommand::TimerStatus);
    }
    // Any remaining phrase that mentions a timer is a request to start one:
    // with an explicit duration it's a SetTimer, otherwise a StartTimer.
    if is_timer_set(&text) {
        if let Some(seconds) = extract_any_duration(&text) {
            let label = timer_label(&text);
            return Some(VoiceCommand::SetTimer { seconds, label });
        }
        return Some(VoiceCommand::StartTimer);
    }

    // --- Ingredient queries ("how much butter", "how many eggs"). ---
    if let Some(ingredient) = ingredient_query(&text) {
        return Some(VoiceCommand::HowMuch(ingredient));
    }
    if is_any(
        &text,
        &[
            "ingredients",
            "list ingredients",
            "what do i need",
            "what are the ingredients",
            "read the ingredients",
            "read ingredients",
        ],
    ) {
        return Some(VoiceCommand::ListIngredients);
    }

    // --- Step navigation. ---
    if let Some(n) = goto_step(&text) {
        return Some(VoiceCommand::GoToStep(n));
    }
    if is_any(
        &text,
        &[
            "whats next",
            "what is next",
            "next up",
            "preview",
            "what comes next",
            "what is the next step",
            "whats the next step",
        ],
    ) {
        return Some(VoiceCommand::WhatsNext);
    }
    if is_any(
        &text,
        &[
            "next",
            "next step",
            "continue",
            "go on",
            "keep going",
            "done",
            "im done",
            "finished",
            "move on",
            "advance",
            "forward",
        ],
    ) {
        return Some(VoiceCommand::Next);
    }
    if is_any(
        &text,
        &[
            "back",
            "go back",
            "previous",
            "previous step",
            "last step",
            "step back",
            "backup",
        ],
    ) {
        return Some(VoiceCommand::Previous);
    }
    if is_any(
        &text,
        &[
            "repeat",
            "again",
            "say again",
            "say that again",
            "what was that",
            "read that again",
            "current step",
            "read step",
            "read the step",
            "where was i",
        ],
    ) {
        return Some(VoiceCommand::Repeat);
    }

    // --- Meta. ---
    if is_any(
        &text,
        &[
            "help",
            "what can i say",
            "what can you do",
            "commands",
            "options",
        ],
    ) {
        return Some(VoiceCommand::Help);
    }
    if is_any(
        &text,
        &[
            "quit",
            "exit",
            "stop cooking",
            "stop",
            "im finished cooking",
            "end",
            "goodbye",
            "close",
        ],
    ) {
        return Some(VoiceCommand::Quit);
    }

    None
}

/// True when `text` exactly equals any candidate, or contains it as a phrase.
fn is_any(text: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|c| text == *c || has_phrase(text, c))
}

fn is_timer_set(text: &str) -> bool {
    // Stop/pause/resume/status are already handled before this is called, so any
    // remaining mention of a timer is a request to start/set one.
    has_phrase(text, "timer")
}

fn is_timer_status(text: &str) -> bool {
    is_any(
        text,
        &[
            "time left",
            "how much time",
            "how much time left",
            "how long left",
            "how long is left",
            "time remaining",
            "how much longer",
            "timer status",
            "check timer",
            "hows the timer",
        ],
    )
}

/// Scan normalized text for the first "<number> <unit>" (or compact "10m")
/// duration and return it in seconds. Unlike the timeline heuristic extractor,
/// this does not require a leading "for/about" keyword, so it also handles
/// "start a 5 minute timer".
fn extract_any_duration(text: &str) -> Option<u64> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    for i in 0..tokens.len() {
        let starts_digit = tokens[i].chars().next().is_some_and(|c| c.is_ascii_digit());
        if !starts_digit {
            continue;
        }
        // "<num> <unit>" (e.g. "10 minutes").
        if let Some(unit) = tokens.get(i + 1) {
            let candidate = format!("{} {}", tokens[i], unit);
            if let Some(secs) = parse_duration_str(&candidate) {
                return Some(secs);
            }
        }
        // Compact "<num><unit>" (e.g. "10m").
        if let Some(secs) = parse_duration_str(tokens[i]) {
            return Some(secs);
        }
    }
    None
}

/// Extract an optional label from a "set a timer for X" phrase, e.g.
/// "set a timer for the pasta for 8 minutes" -> Some("pasta").
fn timer_label(text: &str) -> Option<String> {
    // Look for "for the <word...>" that is not the duration clause.
    let tokens: Vec<&str> = text.split_whitespace().collect();
    // Find a "for the" pair whose following word is non-numeric.
    for i in 0..tokens.len().saturating_sub(2) {
        if tokens[i] == "for" && tokens[i + 1] == "the" {
            let candidate = tokens[i + 2];
            if word_to_number(candidate).is_none()
                && candidate != "timer"
                && candidate.chars().all(|c| c.is_ascii_alphabetic())
            {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

/// Parse "how much/many <ingredient>" queries.
fn ingredient_query(text: &str) -> Option<String> {
    for prefix in ["how much", "how many"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            let mut cleaned = rest.trim();
            // Drop leading filler and articles ("of the flour" -> "flour").
            loop {
                let trimmed = cleaned
                    .strip_prefix("of ")
                    .or_else(|| cleaned.strip_prefix("the "))
                    .or_else(|| cleaned.strip_prefix("a "))
                    .or_else(|| cleaned.strip_prefix("an "))
                    .or_else(|| cleaned.strip_prefix("some "))
                    .or_else(|| cleaned.strip_prefix("my "))
                    .map(str::trim);
                match trimmed {
                    Some(t) if t != cleaned => cleaned = t,
                    _ => break,
                }
            }
            // Drop trailing filler ("... do i need", "... go in").
            for suffix in [
                " do i need",
                " is there",
                " go in",
                " does it need",
                " left",
            ] {
                if let Some(stripped) = cleaned.strip_suffix(suffix) {
                    cleaned = stripped.trim();
                }
            }
            if cleaned.is_empty() {
                return None;
            }
            return Some(cleaned.to_string());
        }
    }
    None
}

/// Parse "go to step N" / "step N" / "jump to step N".
fn goto_step(text: &str) -> Option<usize> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let idx = tokens.iter().position(|&t| t == "step")?;
    // The number appears right after "step" ("step 3", "go to step five").
    let word = tokens.get(idx + 1)?;
    word_to_number(word)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(s: &str) -> Option<VoiceCommand> {
        parse_command(s)
    }

    #[test]
    fn navigation_next() {
        for phrase in [
            "next",
            "Next step",
            "continue",
            "go on!",
            "done",
            "I'm done",
            "keep going",
            "move on",
        ] {
            assert_eq!(cmd(phrase), Some(VoiceCommand::Next), "phrase: {phrase}");
        }
    }

    #[test]
    fn navigation_previous() {
        for phrase in ["back", "go back", "previous step", "last step"] {
            assert_eq!(
                cmd(phrase),
                Some(VoiceCommand::Previous),
                "phrase: {phrase}"
            );
        }
    }

    #[test]
    fn repeat_and_whats_next() {
        assert_eq!(cmd("repeat"), Some(VoiceCommand::Repeat));
        assert_eq!(cmd("say that again"), Some(VoiceCommand::Repeat));
        assert_eq!(cmd("what was that?"), Some(VoiceCommand::Repeat));
        assert_eq!(cmd("what's next?"), Some(VoiceCommand::WhatsNext));
        assert_eq!(cmd("what is the next step"), Some(VoiceCommand::WhatsNext));
    }

    #[test]
    fn goto_step_digit_and_word() {
        assert_eq!(cmd("go to step 3"), Some(VoiceCommand::GoToStep(3)));
        assert_eq!(cmd("jump to step five"), Some(VoiceCommand::GoToStep(5)));
        assert_eq!(cmd("step 12"), Some(VoiceCommand::GoToStep(12)));
    }

    #[test]
    fn ingredient_how_much() {
        assert_eq!(
            cmd("how much butter"),
            Some(VoiceCommand::HowMuch("butter".into()))
        );
        assert_eq!(
            cmd("how many eggs"),
            Some(VoiceCommand::HowMuch("eggs".into()))
        );
        assert_eq!(
            cmd("how much of the flour do I need"),
            Some(VoiceCommand::HowMuch("flour".into()))
        );
    }

    #[test]
    fn ingredient_list() {
        assert_eq!(cmd("ingredients"), Some(VoiceCommand::ListIngredients));
        assert_eq!(cmd("what do I need?"), Some(VoiceCommand::ListIngredients));
    }

    #[test]
    fn timer_with_duration() {
        assert_eq!(
            cmd("set a timer for 10 minutes"),
            Some(VoiceCommand::SetTimer {
                seconds: 600,
                label: None
            })
        );
        assert_eq!(
            cmd("start a 5 minute timer"),
            Some(VoiceCommand::SetTimer {
                seconds: 300,
                label: None
            })
        );
    }

    #[test]
    fn timer_with_label() {
        assert_eq!(
            cmd("set a timer for the pasta for 8 minutes"),
            Some(VoiceCommand::SetTimer {
                seconds: 480,
                label: Some("pasta".into())
            })
        );
    }

    #[test]
    fn timer_start_without_duration() {
        assert_eq!(cmd("start the timer"), Some(VoiceCommand::StartTimer));
    }

    #[test]
    fn timer_controls() {
        assert_eq!(cmd("stop timer"), Some(VoiceCommand::StopTimer));
        assert_eq!(cmd("cancel the timer"), Some(VoiceCommand::StopTimer));
        assert_eq!(cmd("pause timer"), Some(VoiceCommand::PauseTimer));
        assert_eq!(cmd("resume timer"), Some(VoiceCommand::ResumeTimer));
    }

    #[test]
    fn timer_status() {
        assert_eq!(cmd("how much time left"), Some(VoiceCommand::TimerStatus));
        assert_eq!(cmd("time remaining"), Some(VoiceCommand::TimerStatus));
        assert_eq!(cmd("how much longer"), Some(VoiceCommand::TimerStatus));
    }

    #[test]
    fn meta_commands() {
        assert_eq!(cmd("help"), Some(VoiceCommand::Help));
        assert_eq!(cmd("what can I say"), Some(VoiceCommand::Help));
        assert_eq!(cmd("quit"), Some(VoiceCommand::Quit));
        assert_eq!(cmd("stop cooking"), Some(VoiceCommand::Quit));
    }

    #[test]
    fn unrecognized_returns_none() {
        assert_eq!(cmd(""), None);
        assert_eq!(cmd("the weather is nice today"), None);
        assert_eq!(cmd("!!!"), None);
    }

    #[test]
    fn timer_status_not_confused_with_ingredient() {
        // "how much time left" must be a timer status, not an ingredient query.
        assert_eq!(cmd("how much time left"), Some(VoiceCommand::TimerStatus));
    }
}
