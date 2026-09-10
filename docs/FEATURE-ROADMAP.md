# Feature Roadmap

Status: stages 1 through 4 and 7 implemented; long-form work moved to Podsonic.

This roadmap covers the next personal-feature cycle after the ratings,
playback-filter, keybinding, ReplayGain, and responsive-footer work. Each stage
should remain a focused change with its own regression coverage and broad
verification before the next stage begins.

## Completed foundation

- The footer now measures its shortcut content and wraps complete
  key/description pairs on narrow terminals.
- Tall, narrow displays use spare rows for the footer while wide displays keep
  the existing side-by-side status layout.
- Notifications and sample-rate status remain visible.
- Render coverage includes all six pages at 60x50 as well as the existing
  80x24 snapshots.

## 1. In-app playback-filter editor

**Implemented.**

The existing Settings rows already edit minimum rating, year, and duration
filters and persist them through `SetPlaybackFilters`. Extend this path rather
than adding another filter subsystem.

Initial scope:

- Add a Settings action that opens a modal list editor.
- Edit excluded genres and excluded artists, which are currently
  `config.toml`-only.
- Support add, remove, cancel, and save operations.
- Trim entries, reject empty values, and prevent case-insensitive duplicates.
- Save the complete `PlaybackFilters` value through the existing daemon IPC
  request so daemon-backed and standalone modes behave identically.
- Keep filters non-retroactive: changing them does not alter the current queue.

Acceptance criteria:

- Both lists round-trip through config persistence.
- Failed saves leave the prior daemon configuration intact and show an error.
- Keyboard and mouse paths are consistent where controls are clickable.
- The editor remains usable on narrow and short terminals.

## 2. In-app global-keybinding editor

**Implemented.**

Build on `config::keybind::{GlobalAction, KeyChord, resolve}`. The first version
edits all 15 global actions; page-specific and modal bindings stay
fixed until there is a context-aware conflict model.

Initial scope:

- Show every global action and its effective chord.
- Capture the next pressed chord when editing an action.
- Reject reserved rating chords and report collisions before saving.
- Reset one action or all actions to defaults.
- Add daemon IPC/config persistence for the full override map.
- Re-resolve the running TUI keymap immediately after a successful save.
- Make footer hints use effective bindings instead of hard-coded defaults.

Acceptance criteria:

- Changes take effect without restarting either mode.
- A failed write does not leave the displayed and active maps inconsistent.
- Navigation remains possible after editing, including a safe route to reset
  an unusable mapping.
- Persistence, collision, capture, and footer-display behavior have regression
  coverage.

## 3. Remaining responsive-layout pass

**Implemented.**

Use the footer behavior as the model for width-aware layout decisions.

Initial scope:

- Give narrow terminals a header layout that keeps every page reachable and
  does not let the fixed transport area cut off tabs.
- Stack Library and Playlists panes vertically below a measured width
  threshold while preserving focus and mouse hit-testing.
- Stack Quick Play's option and song panes when the fixed option column would
  starve the song list.
- Budget cover-art and cava space against the available width and height.
- Prefer content measurement over monitor-orientation settings.

Acceptance criteria:

- Add representative wide, 80x24, narrow-tall, and narrow-short renders.
- Keyboard focus, mouse regions, resize handling, and empty states remain
  correct in both orientations.

## 4. Lyrics

**Implemented.**

Add lyrics as a non-blocking overlay available from any page while a track is
loaded.

Initial scope:

- Detect the OpenSubsonic `songLyrics` extension.
- Prefer `getLyricsBySongId` for structured and synchronized lyrics.
- Fall back to classic `getLyrics` for compatible servers without the
  extension.
- Cache results by song ID and never fetch in the render loop.
- Support manual scrolling, follow-current-line mode, language/source
  selection when multiple results exist, and clear loading/empty/error states.
- Keep fetched lyrics client-side unless daemon ownership becomes necessary.

Acceptance criteria:

- Unsynchronized, synchronized, missing, malformed, and unsupported responses
  are covered with wiremock tests.
- Playback remains unaffected by slow or failed lyric requests.
- The overlay works after daemon reconnect and on narrow terminals.

References:

- <https://opensubsonic.netlify.app/docs/endpoints/getlyricsbysongid/>
- <https://opensubsonic.netlify.app/docs/endpoints/getlyrics/>
- <https://www.navidrome.org/docs/developers/subsonic-api/>

## 5. Audiobook and long-form playback

**Moved to Podsonic.**

The initial Ferrosonic implementation was completed and live-verified, then
removed after the decision to keep music and long-form playback in separate
clients. Podsonic will use Navidrome for audiobook media and bookmarks.

## 6. Podcasts

**Moved to Podsonic.**

Official Navidrome returns HTTP 501 for all Subsonic podcast endpoints and
cannot subscribe to RSS feeds. Direct RSS support also requires source-aware
queue, progress, persistence, refresh, and security models that do not belong
in Ferrosonic's music-oriented architecture. Audiobooks and podcasts will be
developed as the separate Podsonic long-form client. The former Ferrosonic
audiobook implementation has been removed.

The Podsonic planning package is in the workspace-root `podsonic/` directory.

References:

- <https://opensubsonic.netlify.app/docs/endpoints/getpodcasts/>
- <https://github.com/navidrome/navidrome/blob/master/server/subsonic/api.go>

## 7. Expanded Quick Play discovery

**Implemented.**

Quick Play now exposes four additional album-focused choices using the
standard Subsonic `getAlbumList2` endpoint:

- Newest Album (`newest`)
- Recently Played (`recent`)
- Most Played (`frequent`)
- Highest Rated (`highest`)

Each selection fetches the category's first album and then its complete track
list. Results have independent daemon caches, cross daemon IPC events, active
music-folder scoping, star/rating synchronization, stale-server protection,
and explicit empty states. The compact responsive selector calculates enough
columns to keep all seven Quick Play modes visible and uses matching mouse hit
regions.

Reference:

- <https://opensubsonic.netlify.app/docs/endpoints/getalbumlist2/>

## Verification for every stage

At minimum, run formatting, both repository Clippy checks, all-target nextest,
and doc tests. UI work also requires reviewed snapshots at relevant terminal
sizes. Daemon, IPC, or playback changes require focused fake-mpv and wiremock
regressions plus manual smoke testing when automated coverage cannot
represent the behavior.
