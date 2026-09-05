//! Global keybinding configuration: human-readable key-chord strings
//! (`"q"`, `"F1"`, `"Space"`, `"Ctrl+r"`) parsed into [`KeyChord`], and the
//! fixed set of remappable global actions ([`GlobalAction`]).
//!
//! Only the page-independent bindings in `app::input`'s global match are
//! configurable via the `[Keybindings]` config table; per-page bindings and
//! modal overlays (quit-confirm, the playlist picker, Settings' own
//! h/l/space field navigation) stay hardcoded. Two default bindings are
//! also intentionally excluded from remapping: the `1`-`5` song-rating
//! keys (five keys feeding one conceptual action, which doesn't fit this
//! module's one-action-one-chord model) and `p` as a secondary alias for
//! `TogglePause` (kept working unconditionally alongside the configurable
//! `Space` binding so existing muscle memory never breaks).

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use crossterm::event::{KeyCode, KeyModifiers};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A key press: a [`KeyCode`] plus the modifiers that must be held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    /// The base key.
    pub code: KeyCode,
    /// Modifiers that must be held alongside `code`.
    pub modifiers: KeyModifiers,
}

impl KeyChord {
    const fn plain(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::NONE,
        }
    }

    const fn ctrl(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::CONTROL,
        }
    }

    const fn shift(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::SHIFT,
        }
    }
}

impl From<crossterm::event::KeyEvent> for KeyChord {
    fn from(key: crossterm::event::KeyEvent) -> Self {
        Self {
            code: key.code,
            modifiers: key.modifiers,
        }
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            write!(f, "Ctrl+")?;
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            write!(f, "Alt+")?;
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) {
            write!(f, "Shift+")?;
        }
        match self.code {
            KeyCode::Char(' ') => write!(f, "Space"),
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::F(n) => write!(f, "F{n}"),
            KeyCode::Enter => write!(f, "Enter"),
            KeyCode::Esc => write!(f, "Esc"),
            KeyCode::Tab => write!(f, "Tab"),
            KeyCode::BackTab => write!(f, "BackTab"),
            KeyCode::Backspace => write!(f, "Backspace"),
            KeyCode::Delete => write!(f, "Delete"),
            KeyCode::Insert => write!(f, "Insert"),
            KeyCode::Home => write!(f, "Home"),
            KeyCode::End => write!(f, "End"),
            KeyCode::PageUp => write!(f, "PageUp"),
            KeyCode::PageDown => write!(f, "PageDown"),
            KeyCode::Up => write!(f, "Up"),
            KeyCode::Down => write!(f, "Down"),
            KeyCode::Left => write!(f, "Left"),
            KeyCode::Right => write!(f, "Right"),
            other => write!(f, "{other:?}"),
        }
    }
}

/// A `[Keybindings]` value that could not be parsed as a key chord.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseKeyChordError(String);

impl fmt::Display for ParseKeyChordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid key chord: {:?}", self.0)
    }
}

impl std::error::Error for ParseKeyChordError {}

impl FromStr for KeyChord {
    type Err = ParseKeyChordError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut modifiers = KeyModifiers::NONE;
        let mut rest = s;
        loop {
            if let Some(r) = rest.strip_prefix("Ctrl+") {
                modifiers |= KeyModifiers::CONTROL;
                rest = r;
            } else if let Some(r) = rest.strip_prefix("Alt+") {
                modifiers |= KeyModifiers::ALT;
                rest = r;
            } else if let Some(r) = rest.strip_prefix("Shift+") {
                modifiers |= KeyModifiers::SHIFT;
                rest = r;
            } else {
                break;
            }
        }

        let err = || ParseKeyChordError(s.to_string());
        let code = match rest {
            "Space" => KeyCode::Char(' '),
            "Enter" => KeyCode::Enter,
            "Esc" | "Escape" => KeyCode::Esc,
            "Tab" => KeyCode::Tab,
            "BackTab" => KeyCode::BackTab,
            "Backspace" => KeyCode::Backspace,
            "Delete" => KeyCode::Delete,
            "Insert" => KeyCode::Insert,
            "Home" => KeyCode::Home,
            "End" => KeyCode::End,
            "PageUp" => KeyCode::PageUp,
            "PageDown" => KeyCode::PageDown,
            "Up" => KeyCode::Up,
            "Down" => KeyCode::Down,
            "Left" => KeyCode::Left,
            "Right" => KeyCode::Right,
            _ if rest.len() > 1
                && rest.starts_with('F')
                && rest[1..].bytes().all(|b| b.is_ascii_digit()) =>
            {
                KeyCode::F(rest[1..].parse().map_err(|_| err())?)
            }
            _ => {
                let mut chars = rest.chars();
                let c = chars.next().ok_or_else(err)?;
                if chars.next().is_some() {
                    return Err(err());
                }
                // crossterm attaches SHIFT to every uppercase-letter key
                // event regardless of an explicit "Shift+" prefix (see
                // `char_code_to_event` in its unix parser), so a chord
                // written as the bare capital letter (e.g. "T") must imply
                // SHIFT here too or it can never match a real keypress.
                if c.is_uppercase() {
                    modifiers |= KeyModifiers::SHIFT;
                }
                let c = if modifiers.contains(KeyModifiers::SHIFT) {
                    c.to_ascii_uppercase()
                } else {
                    c
                };
                KeyCode::Char(c)
            }
        };
        Ok(Self { code, modifiers })
    }
}

impl Serialize for KeyChord {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for KeyChord {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// One of the global (page-independent) keyboard actions remappable via
/// the `[Keybindings]` config table.
///
/// Per-page bindings and modal overlays are not configurable; see the
/// module docs for the two default bindings excluded from this set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GlobalAction {
    /// Quit the app (or prompt for confirmation when daemon-backed).
    Quit,
    /// Advance to the next queue track.
    NextTrack,
    /// Go back to the previous queue track.
    PreviousTrack,
    /// Pause/resume playback.
    TogglePause,
    /// Toggle the star on the currently-playing song.
    StarPlaying,
    /// Replace the queue with a fresh shuffled batch from the library.
    ShuffleLibrary,
    /// Step the repeat mode: Off -> One -> All -> Off.
    CycleRepeat,
    /// Re-fetch starred/random/artists/playlists from the server.
    Refresh,
    /// Switch to the Library page.
    GoToLibrary,
    /// Switch to the Queue page.
    GoToQueue,
    /// Switch to the Quick Play page.
    GoToQuickPlay,
    /// Switch to the Playlists page.
    GoToPlaylists,
    /// Switch to the Server page.
    GoToServer,
    /// Switch to the Settings page.
    GoToSettings,
}

impl GlobalAction {
    /// Whether this action switches the active page. `app::input` uses
    /// this to run each page's "revert unsaved edits" cleanup based on the
    /// resolved action rather than the literal key, so remapping a page
    /// switch away from its default F-key keeps that cleanup working.
    #[must_use]
    pub const fn is_page_switch(self) -> bool {
        matches!(
            self,
            Self::GoToLibrary
                | Self::GoToQueue
                | Self::GoToQuickPlay
                | Self::GoToPlaylists
                | Self::GoToServer
                | Self::GoToSettings
        )
    }
}

/// Default key chord for each global action, in a fixed order used to
/// resolve chord collisions deterministically when merging in overrides
/// (see [`resolve`]).
pub const DEFAULT_BINDINGS: [(GlobalAction, KeyChord); 14] = [
    (GlobalAction::Quit, KeyChord::plain(KeyCode::Char('q'))),
    (GlobalAction::GoToLibrary, KeyChord::plain(KeyCode::F(1))),
    (GlobalAction::GoToQueue, KeyChord::plain(KeyCode::F(2))),
    (GlobalAction::GoToQuickPlay, KeyChord::plain(KeyCode::F(3))),
    (GlobalAction::GoToPlaylists, KeyChord::plain(KeyCode::F(4))),
    (GlobalAction::GoToServer, KeyChord::plain(KeyCode::F(5))),
    (GlobalAction::GoToSettings, KeyChord::plain(KeyCode::F(6))),
    (
        GlobalAction::TogglePause,
        KeyChord::plain(KeyCode::Char(' ')),
    ),
    (GlobalAction::NextTrack, KeyChord::plain(KeyCode::Char('l'))),
    (
        GlobalAction::PreviousTrack,
        KeyChord::plain(KeyCode::Char('h')),
    ),
    (
        GlobalAction::StarPlaying,
        KeyChord::plain(KeyCode::Char('n')),
    ),
    (
        GlobalAction::ShuffleLibrary,
        // crossterm reports a real Shift+T keypress as Char('T') with the
        // SHIFT modifier set (it infers SHIFT from the uppercase char), so
        // the default chord must include it too or this can never fire.
        KeyChord::shift(KeyCode::Char('T')),
    ),
    (
        GlobalAction::CycleRepeat,
        KeyChord::plain(KeyCode::Char('r')),
    ),
    (GlobalAction::Refresh, KeyChord::ctrl(KeyCode::Char('r'))),
];

/// Merge user overrides onto [`DEFAULT_BINDINGS`], then invert into a
/// press-to-action lookup for `app::input` to consult on every keypress.
///
/// On a chord collision (two actions ending up bound to the same physical
/// key, which can only happen from a user override since the defaults
/// don't collide), the earlier action in `DEFAULT_BINDINGS` order wins and
/// the later one becomes unreachable; a warning is logged, and also
/// returned as a human-readable string so a caller can surface it in the
/// TUI (a typo'd `[Keybindings]` entry shouldn't fail silently there too).
#[must_use]
// Always called with Config's std-hasher HashMap; generalizing over
// BuildHasher adds a type param with no real caller.
#[allow(clippy::implicit_hasher)]
pub fn resolve(
    overrides: &HashMap<GlobalAction, KeyChord>,
) -> (HashMap<KeyChord, GlobalAction>, Vec<String>) {
    let mut resolved: HashMap<KeyChord, GlobalAction> =
        HashMap::with_capacity(DEFAULT_BINDINGS.len());
    let mut warnings = Vec::new();
    for (action, default_chord) in DEFAULT_BINDINGS {
        let chord = overrides.get(&action).copied().unwrap_or(default_chord);
        if chord.modifiers == KeyModifiers::NONE
            && (matches!(chord.code, KeyCode::Char('1'..='5'))
                || (chord.code == KeyCode::Char('p') && action != GlobalAction::TogglePause))
        {
            let message = format!(
                "Keybinding conflict: {chord} is reserved; {action:?} is unreachable until you fix [Keybindings] in config.toml"
            );
            tracing::warn!("{message}");
            warnings.push(message);
            continue;
        }
        if let Some(existing) = resolved.get(&chord) {
            let message = format!(
                "Keybinding conflict: {chord} is already bound to {existing:?}; {action:?} is unreachable until you fix [Keybindings] in config.toml"
            );
            tracing::warn!("{message}");
            warnings.push(message);
            continue;
        }
        resolved.insert(chord, action);
    }
    (resolved, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_binding_round_trips_through_the_parser() {
        for (action, chord) in DEFAULT_BINDINGS {
            let s = chord.to_string();
            let parsed: KeyChord = s.parse().unwrap_or_else(|e| {
                panic!("default binding for {action:?} ({s:?}) failed to parse: {e}")
            });
            assert_eq!(parsed, chord, "round-trip mismatch for {action:?} ({s})");
        }
    }

    #[test]
    fn parses_modifier_prefixes() {
        assert_eq!(
            "Ctrl+r".parse::<KeyChord>().unwrap(),
            KeyChord::ctrl(KeyCode::Char('r'))
        );
        assert_eq!(
            "Alt+Shift+F5".parse::<KeyChord>().unwrap(),
            KeyChord {
                code: KeyCode::F(5),
                modifiers: KeyModifiers::ALT | KeyModifiers::SHIFT,
            }
        );
    }

    #[test]
    fn bare_uppercase_letter_implies_shift() {
        // crossterm reports a real Shift+T keypress as Char('T') with the
        // SHIFT modifier set (it infers SHIFT from the uppercase char, see
        // `char_code_to_event` in its unix parser); a config chord written
        // as the bare capital letter must resolve to the same thing or it
        // can never match that real keypress.
        assert_eq!(
            "T".parse::<KeyChord>().unwrap(),
            KeyChord::shift(KeyCode::Char('T'))
        );
        // Lowercase is unaffected.
        assert_eq!(
            "t".parse::<KeyChord>().unwrap(),
            KeyChord::plain(KeyCode::Char('t'))
        );
    }

    #[test]
    fn default_keymap_resolves_a_real_shift_t_keypress_to_shuffle_library() {
        let (resolved, _warnings) = resolve(&HashMap::new());
        let real_keypress = KeyChord::shift(KeyCode::Char('T'));
        assert_eq!(
            resolved.get(&real_keypress),
            Some(&GlobalAction::ShuffleLibrary),
            "a real Shift+T keypress (SHIFT modifier set) must resolve to ShuffleLibrary"
        );
    }

    #[test]
    fn parses_named_keys() {
        assert_eq!(
            "Space".parse::<KeyChord>().unwrap(),
            KeyChord::plain(KeyCode::Char(' '))
        );
        assert_eq!(
            "F12".parse::<KeyChord>().unwrap(),
            KeyChord::plain(KeyCode::F(12))
        );
        assert_eq!(
            "Enter".parse::<KeyChord>().unwrap(),
            KeyChord::plain(KeyCode::Enter)
        );
    }

    #[test]
    fn rejects_multi_char_garbage() {
        assert!("Fxyz".parse::<KeyChord>().is_err());
        assert!("ab".parse::<KeyChord>().is_err());
        assert!("".parse::<KeyChord>().is_err());
        assert!("Ctrl+".parse::<KeyChord>().is_err());
    }

    #[test]
    fn resolve_with_no_overrides_matches_defaults_exactly() {
        let (resolved, warnings) = resolve(&HashMap::new());
        assert_eq!(resolved.len(), DEFAULT_BINDINGS.len());
        for (action, chord) in DEFAULT_BINDINGS {
            assert_eq!(resolved.get(&chord), Some(&action));
        }
        assert!(warnings.is_empty());
    }

    #[test]
    fn resolve_applies_an_override() {
        let mut overrides = HashMap::new();
        overrides.insert(GlobalAction::NextTrack, "j".parse::<KeyChord>().unwrap());
        let (resolved, warnings) = resolve(&overrides);

        assert_eq!(
            resolved.get(&"j".parse::<KeyChord>().unwrap()),
            Some(&GlobalAction::NextTrack)
        );
        assert_eq!(
            resolved.get(&KeyChord::plain(KeyCode::Char('l'))),
            None,
            "the old default chord must no longer resolve once overridden away"
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn resolve_on_collision_keeps_the_earlier_action_in_default_order() {
        // Quit (default 'q') comes before StarPlaying (default 'n') in
        // DEFAULT_BINDINGS; remapping StarPlaying onto 'q' must not steal
        // the key away from Quit.
        let mut overrides = HashMap::new();
        overrides.insert(
            GlobalAction::StarPlaying,
            KeyChord::plain(KeyCode::Char('q')),
        );
        let (resolved, warnings) = resolve(&overrides);

        assert_eq!(
            resolved.get(&KeyChord::plain(KeyCode::Char('q'))),
            Some(&GlobalAction::Quit),
            "Quit must keep 'q' since it resolves earlier in DEFAULT_BINDINGS order"
        );
        assert_eq!(
            warnings.len(),
            1,
            "the dropped StarPlaying override must be reported so a caller can surface it"
        );
        assert!(warnings[0].contains("StarPlaying"));
    }

    #[test]
    fn display_round_trips_control_and_space() {
        assert_eq!(KeyChord::ctrl(KeyCode::Char('r')).to_string(), "Ctrl+r");
        assert_eq!(KeyChord::plain(KeyCode::Char(' ')).to_string(), "Space");
        assert_eq!(KeyChord::plain(KeyCode::F(6)).to_string(), "F6");
    }
}
