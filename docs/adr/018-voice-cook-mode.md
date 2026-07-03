# ADR-018: Voice Cook Mode — Hands-Free, Local-First Speech

**Status**: Proposed
**Date**: 2026-07-02
**Decision**: Add a hands-free voice cook mode (`fond cook <slug> --voice`) that reads steps aloud and is driven by spoken (or typed) commands, built on a pure, UI-independent grammar + cook-state "brain" (`fond-voice`) and thin, swappable speech adapters whose default path is fully on-device. Cloud/external speech is opt-in, clearly labeled, and never required.

## Context

ROADMAP §13 Phase 8 lists **voice cook mode** as the last of three moonshots (alongside multi-recipe coordination, delivered in ADR-016, and community sharing, delivered in ADR-017). The problem is concrete and physical: while cooking, the user's hands are wet, greasy, or full, and touching a keyboard or touchscreen to advance a step, check a quantity, or start a timer is exactly the wrong ergonomics. The existing TUI cook mode (ADR-008 timeline + ratatui) is excellent when you can touch the keyboard, but it is not hands-free.

Two constraints shape the design more than anything else:

1. **Local-first (principle #1) and AI governance (§6).** fond must work fully offline; any capability that needs the cloud is *optional, clearly labeled, and never required*, and the user's private recipes are never sent to a third party without explicit, per-action consent. A voice mode that mandated a cloud STT/TTS service — or that quietly streamed audio somewhere — would violate the core promise of the product.
2. **Honesty and testability.** The value of voice mode is *understanding the user correctly* and *saying the right thing back*. Recognition accuracy in a noisy kitchen is a real risk called out in the issue; the mode must degrade gracefully (never guess wildly, never trap the user) and its behavior must be deterministic enough to test exhaustively.

Bundling a real on-device speech-recognition model (whisper.cpp, Vosk) into the single fond binary was considered and rejected for this iteration: it is a heavyweight, platform-specific dependency that would bloat the binary and complicate the build for every platform, for a Phase-8 research feature with unproven demand. The insight is that the *hard, differentiating logic* of voice mode is not the acoustic model — it is the command grammar and the cook-state responses, which are pure text-in/text-out and can be built and tested now, with the acoustic front-end left as a swappable adapter.

## Decision

fond gains a new **`fond-voice`** crate that owns the two pieces of voice mode worth testing in isolation, plus thin speech adapters. The `fond` binary owns only the I/O loop.

### Grammar and brain (pure, in `fond-voice`)

- **`command`** — a forgiving natural-language parser: `parse_command(&str) -> Option<VoiceCommand>`. It normalizes noisy input (lowercasing, dropping apostrophes so contractions collapse, treating punctuation as separators) and matches salient keywords/phrases rather than a rigid syntax. It recognizes step navigation (`next`, `back`, `repeat`, `what's next`, `go to step three`), ingredient queries (`how much butter`, `list ingredients`), and timers (`set a timer for ten minutes`, `start timer`, `stop/pause/resume timer`, `how much time is left`), plus `help` and `quit`. Unrecognized input returns `None` so the caller says "I didn't catch that" instead of acting on a mishearing.
- **`state`** — `VoiceCookState`, the cook-mode brain. It holds the recipe, optional backward schedule, current step, and running timers, and turns each `VoiceCommand` into a spoken `VoiceResponse`. **Every line the user hears is generated here**, decoupled from any speech backend, so navigation, quantity answers, ingredient lists, and timer behavior are all unit-tested end to end. Timer duration for a step is inferred exactly as the TUI does (schedule node duration, falling back to the step's parsed timer), keeping the two cook surfaces consistent.

### Speech adapters (swappable, default on-device)

- **`speech`** — `Speaker` (TTS) and `Listener` (STT/typed-input) traits, plus `NullSpeaker` for text-only mode. Keeping the brain behind these traits is what lets a real on-device recognizer drop in later without touching the grammar or state code.
- **`tts::SystemSpeaker`** — on-device text-to-speech by shelling out to the platform's native command: `say` (macOS), `spd-say`/`espeak`/`espeak-ng` (Linux), PowerShell `System.Speech` (Windows). No bundled model, no network. If none is found, `detect()` returns `None` and the caller degrades to on-screen text.
- **`listener`** — `StdinListener` reads phrases line-by-line from stdin (the always-available fallback: type commands, *or* pipe any recognizer's text output in for hands-free input), and `CommandListener` spawns a user-chosen recognizer (`--listen-cmd`) and reads one recognized phrase per line from its stdout. Neither performs network I/O; fond never opens a microphone or contacts a service on its own.

### Surface

- **CLI**: `fond cook <slug> --voice` (single recipe). Flags: `--no-speak` (text-only, still voice-driven), `--tts-cmd <CMD>` (override the speaker), `--listen-cmd <CMD>` (external recognizer). The mode prints a banner naming the active speech-in / speech-out backends so any external/cloud tool the user wires in is **clearly labeled**.
- **I/O loop** (in the `fond` binary): a background thread drives the listener and forwards phrases over a channel; the main loop `recv_timeout`s on it so running timers keep ticking and **announce themselves aloud while the session waits for the next command** — the mechanism that makes hands-free timers work. Completed sessions flow into the same cook-log prompt as the TUI.
- **Graceful fallback**: with no speaker available the mode is still fully usable via printed text; with no recognizer, typing (or a piped recognizer) drives it. Nothing about voice mode is load-bearing for the rest of fond.

## Rationale

- **Local-first by construction**: the default path uses on-device OS speech and typed/piped input; there is no cloud dependency and no audio ever leaves the machine unless the user explicitly points `--listen-cmd`/`--tts-cmd` at an external tool of their choosing, which the banner labels plainly.
- **The testable value is built now**: command parsing and every spoken response are pure functions with exhaustive unit tests; recognition accuracy (the noisy-kitchen risk) is isolated behind the `Listener` trait and can be improved independently.
- **Honest degradation**: unrecognized speech is acknowledged, not guessed; missing TTS falls back to text; the mode never traps the user. This mirrors ADR-008's "untimed stays untimed / never fabricate" stance.
- **Consistent with existing cook mode**: timer inference and the cook-log flow are shared with the TUI, so voice mode is a new *surface* over the same engine, not a fork.
- **Additive and low-risk**: a new crate plus a `--voice` branch; single-recipe `fond cook` and the TUI are otherwise untouched, and no new heavyweight dependency enters the build.

## Alternatives Considered

| Alternative | Rejected Because |
|------------|-----------------|
| Bundle an on-device STT model (whisper.cpp / Vosk) into the binary | Heavyweight, platform-specific dependency that bloats the single binary and complicates every platform's build; premature for a Phase-8 research feature. Kept as a swappable `--listen-cmd` backend instead. |
| Require a cloud STT/TTS service | Violates local-first (#1) and the §6 AI-governance principle; would send private recipes/audio off-device and make the core feature depend on the network. |
| Drive voice inside the ratatui TUI (alt-screen) | The TUI takes raw-mode ownership of stdin, which conflicts with reading recognized phrases; a plain line/channel REPL is a cleaner, genuinely hands-free surface. |
| Rigid command syntax ("fond: next step") | Brittle against real, noisy kitchen speech; a forgiving keyword/phrase grammar with an explicit "didn't catch that" is more usable and still deterministic. |
| Always-listening microphone capture built into fond | Privacy risk flagged in the issue; fond deliberately never opens a mic itself — recognition is an explicit, user-supplied pipeline. |

## Consequences

- **Upside**: hands-free navigation, ingredient queries, and timers that read aloud — the Phase-8 moonshot — with zero cloud dependency and a fully offline default.
- **Upside**: the command grammar and spoken responses are pure and exhaustively tested; a better recognizer or a native platform TTS can be added behind the existing traits without reworking behavior.
- **Upside**: graceful fallback everywhere means the feature can never break the rest of cook mode; text-only and typed-input paths always work.
- **Tradeoff**: out of the box, hands-free *input* depends on the user supplying a recognizer (via `--listen-cmd` or a pipe); without one, the mode is voice-*output* + typed input. This is a deliberate consequence of not bundling an acoustic model, and is a pure additive upgrade later.
- **Tradeoff**: shelling out to the platform TTS command means speech quality/voices vary by OS and are the platform's, not fond's — acceptable for a local-first tool that avoids bundling a synth.
- **Non-goal for now**: wake-word / always-on listening, multi-recipe voice coordination, and voice-driven editing of `.cook` files. The `Listener`/`Speaker` traits leave room for these without a format or schema change.
