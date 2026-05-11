# Changelog

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
