# Changelog

## [Unreleased]

### Fixed

- **Configuration transactions.** Concurrent settings, server, and music-folder
  updates now serialize persistence with the matching live/client commit;
  malformed server URLs are rejected before credential or disk side effects.
- **Playback transition races.** Pre-buffer failure fallbacks recheck
  cancellation while holding mpv, manual Next coalesces with an in-flight EOF
  advance, and resuming from exactly zero no longer starts a second play instance.
- **Quick Play request and mouse ordering.** A late older category refresh can
  no longer replace a newer result, and pane borders or unused grid remainder
  cells no longer select invisible options.
- **Server detail HTTP errors.** Artist, album, and playlist detail requests now
  return the same sanitized typed status errors as the common request path.
- **Server password editor reversion.** Leaving the page discards dirty password
  text while retaining the last locally committed credential despite scrubbed
  daemon snapshots.
- **Shortcut hints on narrow terminals.** The footer now wraps complete
  key/description pairs across the extra rows available on tall, narrow
  displays, while keeping notifications and sample-rate status visible.
- **Narrow and vertical terminal layouts.** Header tabs wrap so every page and
  transport control remains visible; Library, Playlists, and Quick Play panes
  stack when horizontal space is limited. Content, cover art, now-playing, and
  cava now share the available height without blanking short form pages.
- **Settings label spacing.** The controls row now uses the shorter “Global
  Keybinds” label so its value keeps the same visual gap as other settings.
- **Server URLs with a path prefix.** A `BaseURL` such as
  `https://example.com/music` had its last path segment dropped when
  endpoints were resolved, so requests went to `/rest/...` at the domain
  root and a reverse proxy's HTML 404 surfaced as "Failed to parse
  response". Base URLs are now normalized to end in `/`, so both spellings
  address the same server.
- **Rating and star synchronization.** Failed rating writes are reported,
  Library search rows update immediately, and queue copies are persisted so
  confirmed rating/star changes survive a daemon restart. `Alt+1`-`Alt+5`
  can rate the highlighted row, with a clear notification when no song is
  highlighted.
- **Per-connection shutdown deadlock.** A disconnected client left the daemon
  connection task (and its client guard) alive forever, so the daemon never
  auto-exited after the TUI closed. Connections now tear down promptly.
- **Superseded pre-buffers.** A `Buffered` download is now cancelled by a
  direct load, pause, stop, or end-of-queue, so it can no longer load and
  unpause a track the user had already replaced or paused.
- **Repeat and restart play reporting.** Repeat-one / duplicated queue entries
  are scrobbled once per play, and the OpenSubsonic `playbackReport` path emits
  the required `starting` marker on each new play instance.
- **MPRIS track ids.** Song ids containing `-`, `.`, `:` or other path-unsafe
  characters now yield a valid `mpris:trackid` instead of a silently missing one.
- **Cover art retry.** A transient cover-art fetch failure no longer leaves the
  track permanently blank; the next update retries.
- **Now-playing click seek.** The click column is computed from the same
  geometry as the drawn bar, so clicks land where the user clicked on tracks
  of any length. A one-row now-playing strip no longer underflows.
- **Modifier-blind page shortcuts.** Ctrl/Alt chords that are not bound to a
  global action no longer fall through to page handlers (previously Ctrl+D on
  the Queue removed the highlighted song and Ctrl+C cleared history).
- **Mouse page switches.** Clicking a header tab now discards the same
  search/modal/editor state as the keyboard page switch.
- **Malformed theme colors.** A six-byte non-ASCII color value no longer panics
  theme loading.
- **Gapless advance race.** The mpv end-of-file listener and the idle tick can
  no longer both advance the queue for one track end.
- **Pre-buffer write failure.** A disk-write error mid-download falls back to a
  direct `loadfile` instead of dropping the chosen track.
- **Server HTTP errors.** A 4xx/5xx response is now reported as an HTTP status
  (with the authenticated URL stripped) instead of a misleading parse error.
- **Starred view scoping.** The Starred list honors the selected music folder
  like every other browse view.
- **Full album paging.** `get_all_albums` pages past a server that caps the
  page size below the requested 500, with a duplicate/loop guard.
- **mpv process lifecycle.** An exited or orphaned mpv child is now killed and
  reaped, and a spawn whose IPC connect failed no longer wedges the backend.
- **cava process lifecycle.** `openpty`/`dup`/`fcntl` results are checked and an
  exited cava is reaped through the full cleanup path.
- **Long-track duration in lists.** Tracks of an hour or more now render
  `HH:MM:SS` in the queue, playlist, and search rows, matching the player.

### Changed

- **Authoritative MPRIS volume.** Volume now lives in daemon now-playing state,
  so MPRIS reflects changes from every control path and handles non-finite input.
- **MPRIS repeat and volume.** `LoopStatus` now reflects and sets the repeat
  mode; `Volume` round-trips through `SetVolume` (clamped to 0.0-1.0) instead
  of always reporting 100%.
- **Quick Play keyboard navigation.** On panes that render multiple option
  columns, Up/Down move a full grid row; `Ctrl+R` keeps and refreshes the active
  Quick Play mode instead of resetting to Starred.
- **Daemon disconnect.** Requests now time out after 30s, and a dropped daemon
  connection tells the TUI to exit with an error instead of rendering stale
  state.

### Security

- **Environment password is not persisted.** `FERROSONIC_PASSWORD` remains a
  non-persistent override; a settings save no longer writes it to `config.toml`.
- **Keychain failures are surfaced.** A reachable-but-failing OS keychain no
  longer silently downgrades to an inline plaintext credential.
- **IPC config scrub.** `PasswordEval` (which may embed a secret) and the
  keyring marker are stripped from snapshots and `ConfigChanged`, like the
  password.
- **MPRIS art URL.** The `Metadata` getter no longer publishes the
  authenticated remote cover-art URL (which embeds a reusable token); only a
  locally mirrored `file://` URL is exposed. Intermediate property snapshots
  now carry only the cover id and never construct the authenticated URL.
- **Owner-only files.** The config directory is created `0700` and the log file
  is enforced as `0600`, including pre-existing permissive log files.

### Added

- **Stream on start.** Starting a cold queue (picking an album/artist/song,
  shuffling the library, or auto-continue) now streams the track from the
  server and begins playback as soon as mpv has enough data, instead of
  downloading the whole file to disk first. The new `StreamOnStart` setting
  (`F6` Settings) defaults on; turn it off to restore full pre-buffering for a
  guaranteed clean start on slow or flaky networks. Gapless playback is
  unaffected: the next track is still preloaded and prefetched by mpv.
- **Offline track cache.** Opt-in (`OfflineCacheEnabled`, `F6` Settings):
  streamed tracks are written to `$XDG_CACHE_HOME/ferrosonic/tracks` with an
  atomic index and LRU eviction under `OfflineCacheMaxMb` (default 2048 MiB),
  and subsequent plays load the cached file instead of re-fetching. Gapless
  preload uses a cached next track when present. Partial downloads are never
  indexed.
- **Artist/album information.** Press `I` on a highlighted artist or album to
  open a scrollable overlay with biography/liner notes, similar artists, and
  Last.fm/MusicBrainz links, fetched on demand from
  `getArtistInfo2`/`getAlbumInfo2`. Servers without an external integration
  (including stock Navidrome) show a graceful empty state.
- **Resume where you left off.** With `ResumeOnStart` on (the default), a
  graceful daemon shutdown now persists the queue, current track, and playhead;
  the next start restores the session paused at the saved offset. Set
  `AutoplayOnStart` to start playing immediately instead. `ResumeOnStart=false`
  restores the previous behavior of starting empty.
- **Search polish.** Library search now debounces keystrokes
  (`SearchDebounceMs`, default 200), remembers the last 20 queries and recalls
  them with Up/Down while editing the filter, shows artist/album/song result
  counts in the pane title, and honors configurable
  `SearchArtistLimit`/`SearchAlbumLimit`/`SearchSongLimit` instead of fixed
  caps. Match highlighting in the result tree was already present.
- **Expanded Quick Play discovery.** Quick Play now includes Newest Album,
  Recently Played, Most Played, and Highest Rated album modes backed by the
  standard `getAlbumList2` categories. Each mode loads the selected album's
  complete track list, follows the active music folder, and remains usable in
  compact multi-column layouts.
- **Lyrics overlay.** Press `y` from any page to show lyrics for the playing
  track. Structured and synchronized OpenSubsonic lyrics are preferred, with
  classic Subsonic fallback, per-song caching, manual scrolling,
  exact timestamp following or estimated progress following for untimed lyrics,
  source/language selection, and explicit loading, empty, and error states.

- **Song ratings.** Rate the currently-playing track 1-5 with the `1`-`5`
  keys, or the highlighted row with `Alt+1`-`Alt+5` (press the current
  rating again to clear it) - the same playing/highlighted split as `n`
  and `m` for stars. Synced to the server and exposed to desktop media
  controls via MPRIS's `userRating`.
- **Playback filters.** Exclude songs from ever entering the queue by
  minimum rating, year range, or duration (`F6` Settings page), or by genre
  or artist exclude-list (`[PlaybackFilters]` in `config.toml`). Applies
  wherever songs enter the queue - adding, shuffling, and auto-continue's
  random pick - not retroactively to an already-persisted queue.
- **In-app filter editors.** Excluded genre and artist lists can now be
  added to, removed from, cancelled, and saved from Settings. Entries are
  trimmed and checked for empty or case-insensitive duplicates.
- **Random Album quick play.** A third Quick Play (`F3`) mode alongside
  Starred and Random: loads a full random album, re-rolled when switching
  into the option from another one. Returning to `F3`, or clicking the
  already-selected option, keeps the current album.
- **Configurable global keybindings.** The 15 page-independent shortcuts
  (quit, play/pause, next/previous, star-playing, shuffle-library,
  cycle-repeat, refresh, and the six `F1`-`F6` page switches) can be
  remapped via a `[Keybindings]` table in `config.toml`. A chord collision
  is reported both in the log and as a startup notification in the TUI.
- **In-app keybinding editor.** Global shortcuts can now be captured, reset,
  persisted, and applied immediately from Settings. Reserved keys and chord
  collisions are rejected before save, and footer hints show active bindings.
- **ReplayGain.** Persisted `ReplayGainMode` (`"no"`/`"track"`/`"album"`),
  `ReplayGainPreamp` (clamped to -15..+15 dB), and `ReplayGainClip`
  clipping prevention, with three `F6` Settings rows. Applied to mpv at
  startup and pushed live while a track is playing; the latest values are
  reapplied if mpv restarts.

## [0.6.1] - 2026-06-27

### Fixed

- **MPRIS media controls on GNOME.** The GNOME Shell media-controls widget now
  appears and shows cover art. Two bugs were behind it: `CanPlay` was never
  pushed via `PropertiesChanged`, so spec-compliant caching consumers (GNOME)
  kept it `false` forever and never showed the widget; and the cover art
  pointed at a remote authenticated Subsonic URL the widget won't load. The
  cover is now mirrored to a local `file://`, the same way the desktop
  notifications already do. Thanks to @semsemyonoff.
- **PipeWire stream name.** The audio stream now identifies as "ferrosonic" in
  the mixer (pavucontrol and similar) instead of "mpv".

## [0.6.0] - 2026-06-21

### Added

- **OS keychain credential storage (default).** Entering your password on the
  Server page (F5) now stores it in the operating system's keychain (Secret
  Service / GNOME Keyring / KWallet on Linux, Keychain on macOS) and writes
  only a `PasswordKeyring = true` marker to `config.toml`, never the plaintext;
  an existing inline password migrates on next save. On a machine with no
  usable keychain (headless, no unlocked Secret Service) it falls back to an
  inline write and the Server page says so. The Linux backend is pure-Rust
  (zbus Secret Service), so the static release binary keeps no `libdbus` C
  dependency. Resolution order is now
  env > `PasswordEval` > `PasswordFile` > keychain > inline.

- **`PasswordEval` config option.** Run a command and use its output as the
  password, so no secret need sit in `config.toml`. Accepts a shell string
  (`PasswordEval = "pass show navidrome"`) or an argv array
  (`["sops", "-d", "~/x"]`), resolves at startup in the order
  env > `PasswordEval` > `PasswordFile` > inline. Hardened for the headless
  daemon: stdin closed, own session, 30s timeout with a process-group kill,
  fatal-on-failure (never a stale fallback), output zeroized, and the secret
  never passed via argv or env. Works across distros and macOS via `/bin/sh`.

- **Desktop notifications on track change.** A freedesktop.org notification
  (any Linux daemon: mako, dunst, GNOME, KDE) with cover art, fired from the
  daemon so it shows whether or not the TUI is open. On by default; toggle
  under Settings, Notifications. Cover fetched at 512px for sharp icons.

- **Library selection.** On servers with more than one music folder, press `f`
  on the Library page to cycle the server's libraries and "All" (`getMusicFolders`).
  Defaults to the server's first (default) library rather than all; the choice
  scopes the artist tree, album list, random songs, and search via
  `musicFolderId`, shows in the pane title, and is remembered across restarts.

- **Playlist editing.** Rename, delete, add songs, remove songs, and reorder
  server playlists from the Playlists page (`R` rename, `D` delete with a
  confirm, `d` remove a song, `J`/`K` reorder). Press `a` on any highlighted
  song (Library, Queue, Quick Play, or a playlist) to add it to a playlist via
  a picker. Reordering rewrites the playlist in one request, since the Subsonic
  API has no in-place move.

### Changed

- **`config.toml` is now written owner-only (`0600`).** It may hold an inline
  password in the keychain-fallback case, so the file is no longer
  world-readable on shared machines.
- **Resume re-clocks cleanly.** Pausing releases the audio-device rate pin so
  other apps (a browser) play at their own rate; resuming compares the device
  rate to the track's and, if they differ, switches and waits the settle
  before audio, so resume is gapless. Same rate resumes at once.
- **Auto-continue no longer replays a track** until the library is exhausted.
- **Changed cover art shows fresh.** The in-memory cover cache clears on a
  library refresh (also at startup and `Ctrl+R`) and when a new album plays.

- **No gap at song start when the sample rate changes.** Loading an album
  recorded at a different sample rate than the last one used to start the
  song and then re-clock the audio device a moment later, leaving an audible
  gap in the first instant of music. Now the track loads silently, the device
  re-clocks to the new rate during that silence, and playback begins only once
  the rate is locked. Tracks at the same rate start with no added delay. A new
  advanced setting `RateSwitchDelayMs` (default 500) tunes the silent settle
  for DACs that re-lock slowly.

### Fixed

- **Resume works on mpv older than 0.38 (issue #30).** Resume-from-pause
  reloads the track at the saved offset using mpv's 5-argument `loadfile`
  (`start=`), which only exists in mpv 0.38+; on older mpv the command was
  rejected (`invalid parameter`) and playback skipped to the next track. The
  daemon now detects the mpv version and falls back to a load-then-seek path
  below 0.38, while keeping the precise decode-from-offset form on 0.38+. A
  non-finite saved position can no longer emit a malformed `start=`, and the
  Server page and daemon log advise when mpv is below 0.38.
- **Albums show the current cover, not stale embedded art.** The now-playing
  cover and the desktop notification now use the album cover rather than the
  song's embedded image, which Navidrome keeps serving even after the album
  cover is changed.
- **Gapless no longer desyncs the queue.** A race let two preloads append the
  same next track to mpv, leaving a duplicate that played once extra, so the
  highlight and now-playing ran one song ahead of the audio. Preload is now
  single-flight, guarded under the mpv lock.
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
