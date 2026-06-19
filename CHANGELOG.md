# Changelog

## [Unreleased]

### Changed

- **No gap at song start when the sample rate changes.** Loading an album
  recorded at a different sample rate than the last one used to start the
  song and then re-clock the audio device a moment later, leaving an audible
  gap in the first instant of music. Now the track loads silently, the device
  re-clocks to the new rate during that silence, and playback begins only once
  the rate is locked. Tracks at the same rate start with no added delay. A new
  advanced setting `RateSwitchDelayMs` (default 250) tunes the silent settle
  for DACs that re-lock slowly.

### Fixed

- **Streams are no longer transcoded by the server.** The stream request now
  asks for the original file (`format=raw`), so playback is bit-perfect from
  source instead of whatever format the server would transcode to by default.

## [0.5.1] - 2026-06-19

### Added

- **Scrobbling.** Plays are reported to the server so play counts, last-played,
  and Last.fm/ListenBrainz (if you've linked them in Navidrome) all update. On
  servers with the OpenSubsonic `playbackReport` extension (Navidrome 0.62+) it
  reports playback state and the server decides; otherwise it uses classic
  `scrobble`, marking a play once you've heard half the track or four minutes.
  On by default; toggle under Settings -> Scrobble.
- **Save the queue as a playlist.** Press `s` on the Queue page, type a
  name, and Enter creates a server-side playlist from the queue in order.
  Esc cancels.

### Changed

- **Drill into any artist from search.** Enter on a matched artist opens its
  albums; a greyed artist shown because one of its albums *or songs* matched
  is now selectable too, so Enter on it reveals the rest of that artist's
  catalogue. The matched song stays nested under its own album, and the
  album's other tracks load into the right pane when you highlight it. Press
  Enter on an album to play it.
- **Matched text is highlighted** in search results, so you can see at a
  glance why each row came back.
- **Highlighting a search song follows its album.** Moving onto a song in
  the search results loads that song's whole album into the right pane with
  the song itself selected, so pressing Right lands on the matched track
  instead of the first one.
- **Library search is now one unified search.** Typing after `/` searches
  artists, albums, and songs at once; the old `/` `//` `///` scope cycle is
  gone. Results form a tree that stops at the match: an artist match is a
  single row (Enter expands its full catalog), a matched album nests under
  its artist (greyed when only the album matched) and loads its tracks into
  the song pane when you highlight it, and a matched song nests under its
  album and artist (both greyed) so you can see where it lives.

### Fixed

- Searching for an artist now lists and plays their albums; expanding a
  searched artist was previously a no-op.
- Search matches songs by title only. A query like `beach` no longer lists
  every track by an artist whose name contains it; albums likewise show only
  when their own name matches.
- `q` while viewing search results returns to the artist tree instead of
  quitting; an active search box types a literal `q`.

## [0.5.0] - 2026-06-15

### Added

- **Album-list view on the Library page.** Press `v` to flip the left
  pane between the artist tree (default) and a flat list of every album;
  `s` cycles the sort between album name (A-Z) and original release date
  (oldest first). Rows lead with the album name, then year and artist
  (muted). The right pane follows the cursor, and the pane title shows an
  `Artists -> Albums` toggle hint that recolours with the active mode.
- **Albums in the artist tree** are now ordered by original release date.

### Changed

- **Toolbar Stop now clears the queue** (it was keeping it, like Pause).
  MPRIS / media-key Stop still keeps the current track per spec.
- **The queue is cleared when the daemon exits**, so a fresh start no
  longer brings back the album you were last playing.
- **The daemon exits on its own** once nothing is playing and no TUI is
  connected, instead of lingering forever.

### Fixed

- **Quit no longer hangs.** The TUI now exits immediately on quit /
  `super+q` / close instead of getting stuck with the runtime unable to
  shut down.
- **No more black background blocks.** Empty areas (after text, blank
  list rows) now show the terminal background instead of black.
- Name sort ignores leading punctuation, so `"Heroes"` sorts under H.
- Transport buttons line up; the Play glyph matches the others.
- Test runs no longer leak background daemons or their mpv children.

## [0.4.1] - 2026-05-11

Two papercuts from 0.4.0 sanded down.

### Changed

- **Stop keeps your queue.** Hitting Stop (the header button,
  bluetooth headphone Stop key, the media widget in waybar / KDE /
  GNOME / Plasma, or `playerctl stop`) no longer wipes the queue.
  Playback halts, the track sits at 0:00, your queue stays exactly
  as it was. Press Play and the same track resumes from the start.
  To actually empty the queue, use the Queue page's Clear action or
  pick a new album / playlist / search result.

### Fixed

- **Daemon failures now show up instead of hiding.** If ferrosonic
  can't reach `ferrosonicd` on launch, you get a clear error
  pointing at the daemon log file plus what to try next (remove a
  stale socket, run with `--standalone`, set `Daemon=false` in
  your config). Previously the TUI silently started in
  single-process mode, so a daemon crash looked like "music stops
  when I close the terminal again" with no clue why.

## [0.4.0] - 2026-05-11

Album art, library-wide search, repeat modes, and seamless album
switches. The Library page and the Now Playing section both got
meaningful upgrades.

### Added

- **Cover art** in the now-playing section. Splits the row
  horizontally — info on the left, art on the right, progress bar
  across the bottom. Detects kitty / iTerm2 / sixel image protocols
  automatically; falls back to half-blocks for terminals that don't
  do graphics (alacritty, foot without sixel, plain xterm). When the
  `chafa` library is installed it's used for substantially higher
  fidelity half-blocks — sextants, octants, braille, Floyd-Steinberg
  dithering, truecolor. Toggled and sized on F6 (Settings). Section
  height auto-shrinks when there's no art to show.
- **Library-wide search.** `/` opens the search bar; press `/` again
  on an empty query to cycle the scope: artists (`/`), albums (`//`),
  songs (`///`). Anything you type hits Subsonic's `search3`
  endpoint, so you find tracks the artist isn't expanded for. The
  bar lights up in your theme's accent colour while you're searching
  so you can tell at a glance. Stale replies from fast typing are
  dropped.
- **Repeat modes.** `r` cycles Off → One → All. `One` re-preloads
  the current track so gapless still works on the loop; `All` wraps
  the queue position when the last track ends. Persisted in config.
- **Auto-continue.** When the queue empties at the end of the last
  track, ferrosonic fetches a fresh roll of random songs from the
  server and keeps playing. Off by default, toggled on F6.
- **Seamless album switches.** Picking a new album or shuffling the
  library no longer mid-cuts audio. Audio stops immediately
  (cleanly, no click), the new track's bytes pre-buffer to local
  disk, and only then does playback resume — guaranteed to start
  cleanly from frame 0 with no stutter regardless of network
  latency. Rapid switches cancel previous downloads so the audio
  always reflects the most recent choice. Gapless playback between
  songs within a queue is unchanged.
- **Settings page redesign**, grouped into Display / Now Playing /
  Playback / System sections. New knobs:
  - **Cover Art Size** (8-24 rows, step 2) — controls the
    now-playing section height when art is visible.
  - **Repeat** (Off / One / All) — mirrors the `r` global key.
  - **Cover Art** (On / Off) — mirrors enabling cover-art display.
- **Two-line footer** so every keybind fits without scrolling.
  Notifications now appear bottom-right under the sample rate
  instead of hiding the keybinds.

### Changed

- **Shuffle keys.** `r` is now Repeat, so shuffle moved: `t`
  shuffles the current context (artist / album / playlist / queue),
  `Shift+T` shuffles the whole library. (Was `s` / `Shift+R` in
  0.3.0.)
- **Global `t` no longer cycles themes.** The theme picker on F6 is
  the entry point; `t` is shuffle-context now.
- The Library page footer puts `n: Star playing` next to
  `m: Star selected` for easier scanning.
- mpv tuning for cleaner transitions: keeps the audio device open
  across track changes (`--audio-stream-silence=yes`), starts
  playback as soon as the decoder has bytes
  (`--cache-pause-initial=no`), no pause-on-underrun
  (`--cache-pause=no`).
- The audio-quality row (format / bit depth / sample rate /
  channels) now appears within ~250 ms of a track change instead of
  waiting up to half a second.
- README documents `chafa` as an optional runtime dependency for
  high-fidelity cover-art rendering.

### Fixed

- The ▶ play indicator no longer sticks on the previous track
  during gapless track advance.
- Protocol-version skew between an old daemon and a new TUI (or
  vice versa) no longer severs the IPC connection — unknown request
  / response variants are reported as errors and the connection
  stays alive.
- Cover art with WebP-encoded album art (Navidrome's default
  output) now decodes correctly; previously only JPEG/PNG worked.

## [0.3.0] - 2026-05-11

The big one. Ferrosonic is now two binaries instead of one, so music keeps
playing when you close the terminal. Page navigation and a few keybinds
have shuffled around to match.

### Added

- `ferrosonicd`, a per-user daemon that owns mpv, the queue, the library
  cache, and the MPRIS server. The TUI talks to it over a Unix socket
  at `$XDG_RUNTIME_DIR/ferrosonic/ferrosonicd.sock`. Music keeps playing
  when you close the terminal, and the queue (with current track index)
  is restored across daemon restarts.
- The TUI auto-spawns `ferrosonicd` on launch if it isn't already
  running. No manual setup needed. Pass `--standalone` to force the
  single-process mode if you want it.
- Star/unstar songs against the Subsonic server. `n` toggles the
  currently-playing song; `m` toggles the highlighted song. Starred
  tracks show a ★ in every list and populate the Quick Play "Starred"
  view.
- Quick Play page (F3) replaces the old Songs page. Two modes: Starred
  (your favourites) and Random (500-song roll, fresh on each visit).
- Settings page gains a Daemon On/Off toggle. Off forces standalone
  mode on subsequent launches; the toggle survives restarts.
- systemd user unit at `contrib/ferrosonicd.service` for users who want
  the daemon at login.
- MPRIS now pushes `PropertiesChanged` signals instead of waiting to be
  polled, so waybar and similar clients see updates immediately.

### Changed

- F-key page order. F1 is now Library (was Songs), F2 is Queue (was
  Artists), F3 is Quick Play (was Queue). Playlists, Server, Settings
  stay at F4/F5/F6.
- Page renames. Songs to Quick Play, Artists to Library.
- `n` is now "star currently-playing". "Add next" moved to `i` on
  Library and Playlists. `m` is new, "star highlighted song".
- Config gains `Daemon = true/false` (default true), `Cava`, `CavaSize`,
  and an optional `PasswordFile` field that reads the password from a
  separate file.
- Render path holds a read lock on daemon state and a write lock on
  client state. Previously it held a write lock across both halves
  for the duration of each frame, which contended with the playback
  poll, MPRIS, and the event pump. These now run in parallel with
  rendering.
- mpv IPC and PipeWire shell-outs are async. No more blocking syscalls
  on the tokio runtime, so a slow mpv command can't freeze a worker
  thread.
- Cava config file is per-process via `tempfile`. Running two TUIs at
  once no longer fights over a shared config path.
- Wire schema (`DaemonRequest` / `DaemonResponse` / `DaemonEvent`) is
  externally tagged JSON, inspectable with `socat`.

### Fixed

- Removing the currently-playing song from the queue advances to the
  next song, or stops cleanly if it was the last one. Previously mpv
  kept playing a "ghost" track that wasn't in the queue.
- mpv health check verifies the child process hasn't exited. Previously
  it only checked whether we still held the buffered reader handle.
- The three id-keyed library caches (artist albums, album songs,
  playlist songs) are now bounded at 50/100/50. They were growing
  without limit.
- Footer sample rate uses float math. 44.1kHz instead of 44kHz.
- Terminal is restored on panic via `panic::set_hook` and an RAII
  guard. Signal handlers (SIGTERM/SIGINT/SIGHUP) trigger a clean exit
  too. No more raw-mode-stuck shells after a crash.
- MPRIS property updates no longer hold the state lock across the
  D-Bus `await`. This fixes an artist-expand freeze caused by D-Bus
  contention.
- Subsonic password is scrubbed before crossing the daemon socket.
  The TUI doesn't need it; only the daemon makes server requests.
- F1-F6 work even while typing in a Server text field or the Library
  filter input. Leaving the Server page with unsaved edits reverts
  the form.

### Removed

- The "ferrosonic-ng fork" notice in the README. The install script
  and README now point at `jaidaken/ferrosonic`.

---

## [0.2.2] - 2026-01-31

- Fix artist list scrolling to bottom on click. Clicking an artist no
  longer jumps the viewport.

## [0.2.1] - 2026-01-29

- Add OpenSSL dev headers to build dependencies in README.

## [0.2.0] - 2026-01-28

Internal refactor; no behaviour changes.

- Refactor `app/mod.rs` (2495 lines) into 10 focused submodules
  (~300 lines each).
- Split `mouse.rs` into page-specific handler files (`mouse_artists.rs`,
  `mouse_playlists.rs`).
- Extract built-in theme TOML data into `theme_builtins.rs`.
- Remove ~620 lines of dead code (`audio/queue.rs`, unused methods and
  structs).
- Remove blanket `#![allow(dead_code)]` from three modules.
- Fix all 16 clippy warnings.
- Add missing `tempfile` dev-dependency for config tests.

## [0.1.0] - 2026-01-27

Initial public release.
