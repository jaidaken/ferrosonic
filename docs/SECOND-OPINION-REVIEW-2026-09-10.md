# Independent Second-Opinion Review of `5fe862d`

Date: 2026-09-10  
Repository: `/home/autism/Documents/VibeCode/ferrosonic/upstream`  
Branch reviewed: `personal-features`  
Baseline: `8c7e127`  
Quick Play feature: `6122390`  
Hardening pass: `5fe862d`

## Purpose

This document is a self-contained handoff for a subsequent Codex session. It
records an independent, skeptical review of the Quick Play feature and the
hardening pass above it. The review checked the implementation and tests rather
than relying on commit messages or the changelog.

No fixes were applied during this review. The findings below describe the
current state at `5fe862d`.

## Executive summary

Most of the hardening work is genuine. Of the 29 reviewed claims:

- 25 are confirmed.
- 4 are only partially fixed.
- None were classified as wholly wrong or unverifiable.

The test suite is green, but it does not exercise the most important remaining
race windows. The two highest-priority defects are:

1. A cancelled pre-buffer task can still load an obsolete URL through one of
   its failure fallbacks, potentially overriding a newer direct play, pause,
   stop, halt, or end-of-queue action.
2. Configuration updates are not serialized as transactions. Concurrent
   setters can leave disk and live state inconsistent, and a concurrent server
   update can overwrite unrelated live changes.

The next actioning session should address those two findings first, with
deterministic regression tests before changing implementation. It should then
fix URL validation order, the music-folder state-lock/file-I/O violation, and
the manual-Next/automatic-advance race.

## Review inputs

The following were read before assessing the changes:

- `../AGENTS.md`
- `docs/LOCK-ORDER.md`
- `docs/KNOWN-ISSUES.md`
- `docs/STABILIZATION.md`
- the `[Unreleased]` section of `CHANGELOG.md`
- README controls, Quick Play, Server, and MPRIS documentation
- `git diff 6122390..5fe862d`
- `git diff 8c7e127..6122390`
- `git show 5fe862d --stat`

## Priority action plan

### P0: make all pre-buffer terminal paths cancellation-safe

Relevant code:

- `src/daemon/core.rs:838-855`
- `src/daemon/core.rs:865-1025`
- callers in `src/daemon/playback_ops.rs`

Current behavior:

- `cancel_prebuffer` removes the active token, flips it, then clears the
  loading slot in documented lock order 5 to 6.
- The normal successful download path checks the cancellation token while
  streaming and again while holding the mpv lock immediately before
  `loadfile_paused`.
- The error branches for temporary-file creation, network fetch, local-file
  creation, stream timeout, stream error, and disk-write failure acquire mpv
  and call direct `loadfile` without rechecking the cancellation token.

Why this is a real race:

1. Buffered play A starts downloading.
2. Direct play B, pause, stop, halt, or end-of-queue cancels A.
3. A encounters a failure after cancellation or was already progressing into
   a failure branch.
4. A waits for and later acquires the mpv lock.
5. A calls `loadfile(A)` without checking whether its token was cancelled.
6. The obsolete track can replace the state selected by the newer operation.

This is more serious than the small happy-path window around
`loadfile_paused`: in the happy path, the token is checked while mpv is locked,
so a superseding mpv action normally wins after that guard is released. The
failure branches can acquire mpv after the superseding operation and win last.

Recommended implementation:

- Introduce one helper for direct fallback, for example
  `fallback_direct_if_current(url, cancel)`.
- The helper should acquire mpv, then recheck cancellation and shutdown
  immediately before calling `loadfile`.
- Use it from every fallback branch rather than repeating unguarded loads.
- Prefer a playback/pre-buffer generation or identity token if cancellation
  must distinguish a superseded task from a still-current fallback.
- Do not hold pre-buffer locks while acquiring mpv; preserve the documented
  lock order.

Required regression tests:

- Cancel a delayed buffered request with a direct load, force the buffered
  request into each representative fallback, and prove only the direct URL is
  loaded.
- Repeat with pause, stop, halt, and end-of-queue.
- Include at least disk-write failure and network/stream failure because these
  are the most important uncovered paths.
- Assert final daemon state as well as fake-mpv command order.

### P0: serialize configuration persistence and live commits

Relevant code:

- `src/daemon/settings_ops.rs:28-149`
- `src/daemon/settings_ops.rs:151-177`
- `src/daemon/library_ops.rs:281-329`
- configuration save helpers under `src/config/`

Current `persist_config` sequence:

1. Clone live config under a short state read lock.
2. Apply a mutation to the clone.
3. Save the clone on a blocking thread.
4. Reapply the mutation to whatever live state exists at commit time.

This avoids holding `state` across file I/O, but it is not transactional. If
two setters clone the same starting configuration, each output file omits the
other mutation. Live state may contain both mutations because they are reapplied
individually, while the last disk write contains only one. The next restart
therefore loses a successfully acknowledged setting.

`update_server_config` has a wider version of the same problem. It snapshots the
entire config, performs credential and persistence work, then replaces the
entire live config with that candidate. An unrelated setting changed during
that interval can be lost both on disk and in live state.

Recommended implementation:

- Add a dedicated configuration-transaction mutex to the daemon core.
- Serialize snapshot, validation, persistence, and live commit through it.
- Never retain the main `state` guard across file or keychain I/O.
- Run synchronous filesystem persistence through `spawn_blocking`.
- Commit the same complete candidate that was successfully persisted, rather
  than replaying a closure against potentially different live state.
- If a transaction mutex is undesirable, use a config generation plus retry,
  but account carefully for non-idempotent credential side effects.
- Ensure the mutex's placement is documented in `docs/LOCK-ORDER.md` if it can
  be acquired with another listed lock.

Required regression tests:

- Start two distinct scalar setters at a barrier so both attempt to persist
  concurrently. Assert that both live state and a freshly loaded `config.toml`
  contain both changes.
- Race a scalar setter with `update_server_config`; assert the unrelated field
  survives in memory and on disk.
- Race two server updates and prove the returned/committed client, config file,
  and credential marker all describe one coherent winner.
- Cover `password_from_env`, `password_keyring`, `PasswordEval`, and password
  file configurations so serialization does not weaken secret handling.

### P1: validate server configuration before persisting it

Relevant code: `src/daemon/settings_ops.rs:92-123`.

`candidate.save_default()` currently runs before `SubsonicClient::new` validates
the new base URL. If client construction fails, live state stays unchanged but
the invalid candidate remains on disk. This contradicts the nearby comment
that validation prevents persisted/live disagreement.

Recommended implementation:

- Validate and construct the new client before persisting the candidate.
- Preserve the rule that a failed save does not alter live config or client.
- Carefully order keychain writes and cleanup so a validation failure creates
  no orphan entry and deletes no working old credential.

Required test:

- Attempt to save a malformed URL and assert that live config, live client,
  persisted config, old credential marker, and old keychain identity are all
  unchanged.

### P1: remove synchronous config persistence from the state write lock

Relevant code: `src/daemon/library_ops.rs:293-303`.

`apply_music_folder` mutates config and calls `save_default()` while holding the
daemon state write lock. This blocks a Tokio worker and prevents playback/UI
state access during filesystem latency. It directly violates the repository's
async rules and makes claim 6 only partial.

Recommended implementation:

- Route this setting through the same serialized config transaction used for
  the P0 configuration fix.
- Clear library caches and update the folder-scoped client only after a
  successful persist.
- Preserve the existing `config_gen` rule and the lock order around the
  subsonic client.

### P1: serialize manual and automatic track transitions

Relevant code:

- `src/daemon/playback_ops.rs:134-205`
- EOF and idle-tick callers of `advance_auto`

The new `advance_in_flight` guard correctly prevents two `advance_auto` calls
from running concurrently. Manual `next_track` does not participate. A user
pressing Next during EOF processing can therefore race the automatic advance
and skip a track or issue two loads.

Recommended implementation:

- Serialize all queue-position-changing playback transitions, including
  automatic advance, Next, Previous, explicit queue-position play, and any
  auto-continue extension that commits a position.
- Alternatively, capture play-instance/queue-position generation and commit
  only if it remains current.
- Preserve repeat-one semantics: automatic advance honors it, manual Next does
  not.

Required test:

- Block one transition at a deterministic seam, invoke manual Next and
  automatic advance concurrently, then assert exactly one intended transition
  and one fake-mpv load.

### P1: complete typed HTTP status handling

Relevant code:

- common paths: `src/subsonic/client.rs:125-140`, `389-404`, `433-442`
- incomplete paths: `src/subsonic/client.rs:598-606`, `641-649`, `701-709`

Generic requests now convert non-2xx responses to sanitized
`SubsonicError::HttpStatus`, but `get_artist`, `get_album`, and `get_playlist`
still read and parse response bodies without first checking status. An HTML 500
from those endpoints remains a misleading parse error.

Recommended implementation:

- Reuse the common response/status helper in all three methods.
- Confirm that the error never formats an authentication-bearing URL.

Required tests:

- For each endpoint, return representative 401, 404, and 500 bodies.
- Assert the exact `HttpStatus` variant, status code, and absence of token,
  salt, username, password, and full authenticated URL in `Display`/`Debug`.

## Lower-priority findings

### Resume at exactly zero can create a second play instance

Relevant code: `src/daemon/playback_ops.rs:103-130`, `283-289`.

Resume/new-play intent is inferred from `start_at <= 0.0`. If playback is paused
immediately at exactly zero, resume is classified as a fresh start. This can
bump `play_instance` and emit another OpenSubsonic `starting` report.

Recommended fix: pass an explicit `PlayIntent::Start` or `PlayIntent::Resume`
instead of deriving intent from a floating-point offset.

Required test: pause at exactly zero, resume, and assert no new play instance or
second `starting` action. Also verify explicit restart-at-zero still does create
a new instance.

### Same-category Quick Play refreshes can commit out of order

Relevant code: `src/daemon/library_ops.rs:119-169`.

Quick Play refreshes reject results after a server or music-folder generation
change. They do not distinguish two overlapping requests for the same category
and generation. If the older request finishes last, it overwrites the newer
refresh.

Recommended fix: add a per-category request generation or make each category
single-flight.

Required test: return two different responses in reversed completion order and
assert the later request wins.

### Quick Play mouse hit testing accepts borders and unused remainder cells

Relevant code:

- renderer: `src/ui/pages/songs.rs:50-78`
- mouse mapping: `src/app/mouse.rs:191-200`

The renderer lays options out inside the bordered pane using a floor-sized
column width. Mouse mapping uses saturating subtraction and clamps the computed
column. Clicking a border or unused trailing cell can therefore select an item
that is not visually under the pointer.

Recommended fix: calculate the exact inner rectangle, reject border clicks,
and reject X positions beyond the actual rendered column extent rather than
clamping them.

Required tests: left/right/top/bottom borders, leftover cells after the final
column, wrapped narrow layout, and a valid center click in every column.

### Existing log files are not corrected to `0600`

Relevant code: `src/bin/ferrosonic.rs:60-71`.

`OpenOptionsExt::mode(0o600)` applies at file creation. An existing permissive
log file keeps its old mode, despite the changelog's unconditional owner-only
claim.

Recommended fix: after opening, use the file descriptor or metadata permissions
to enforce `0600` on Unix. Keep non-Unix portability isolated.

Required test: create an existing `0664` log file, initialize logging through a
testable helper, and assert `0600` afterward.

### Server-page password preservation has imprecise semantics

Relevant code:

- `src/app/input.rs:423-447`
- `README.md:348-356`

Preserving the locally resolved credential prevents a scrubbed daemon snapshot
from blanking it during page changes. However, the same exception preserves an
unsaved newly typed password even though the page-switch operation is described
as reverting edits. This retains a secret in memory and can surprise the user
on a later save.

Recommended fix: distinguish committed local credential state from dirty editor
text. Revert dirty text to the committed local value, without replacing that
value from the scrubbed wire configuration.

Documentation should be updated to describe the exact behavior.

### MPRIS volume is a local MPRIS cache, not authoritative playback state

Relevant code: `src/mpris/server.rs:74-94`, `335-349`.

Volume values set through MPRIS round-trip and clamp correctly. The cache is not
updated when volume changes through another path, so the MPRIS getter can become
stale relative to mpv/TUI state.

Recommended fix: make volume part of authoritative daemon state and update it
from every control path, then have MPRIS read that state.

Required tests: change volume through MPRIS and a non-MPRIS path, asserting the
same getter result. Add NaN and infinity setter inputs even though current casts
do not panic.

### Internal MPRIS snapshots still temporarily contain a remote token URL

Relevant code:

- initial metadata construction: `src/mpris/server.rs:513-544`
- getter scrub/mirroring: `src/mpris/server.rs:317-332`
- pushed metadata: `src/mpris/server.rs:603-619`
- property snapshot test: `tests/mpris_property_snapshot.rs:170-194`

No reviewed D-Bus publication path exposes the remote URL: the getter and push
path replace it with a local `file://` mirror. However, an intermediate public
property snapshot still contains the token-bearing URL, and an existing test
expects it. This expands the in-process secret-bearing surface unnecessarily.

Recommended hardening: carry the cover-art identifier separately and construct
only safe external metadata. Do not put the authenticated URL into a generic
metadata map.

## Lock-order and async audit

No new documented lock inversion was found in the reviewed paths.

- `cancel_prebuffer` acquires lock 5 and releases it before lock 6; the guards
  do not overlap (`src/daemon/core.rs:846-855`).
- `QueueChanged` reads daemon state, releases that guard, and then locks client
  UI state (`src/app/event_pump.rs:56-67`). It does not hold daemon then client
  concurrently.
- `update_server_config` releases the main state guard before keychain and file
  work (`src/daemon/settings_ops.rs:34-90`).

The material violation is `apply_music_folder`, which holds the state write
lock across synchronous `save_default()` (`src/daemon/library_ops.rs:293-303`).
This is an async responsiveness problem even though it is not a multi-lock
cycle.

When introducing a configuration-transaction mutex, the actioning session must
decide whether it is acquired alone or where it belongs in
`docs/LOCK-ORDER.md`. Do not acquire a new config mutex after a higher-ranked
playback lock and then reacquire state.

## Secret-handling assessment

Confirmed protections:

- `password_from_env` is serde-skipped and treated as external during disk
  projection (`src/config/mod.rs:106-111`, `315-330`).
- Environment resolution sets the transient flag (`src/config/mod.rs:680-689`).
- Explicit server credential commit clears the transient flag only on the
  candidate being committed (`src/daemon/settings_ops.rs:48-52`).
- Reachable-but-failing keychain backends return an error instead of falling
  back to plaintext (`src/daemon/settings_ops.rs:70-88`).
- Wire snapshots scrub password, password-eval, password-file, and keyring
  markers (`src/daemon/core.rs:452-490`).
- External MPRIS metadata paths expose only local mirrored art
  (`src/mpris/server.rs:317-332`, `603-619`).

Remaining concerns:

- Config transaction races can make the credential marker, persisted config,
  live config, and installed client describe different updates.
- Server URL validation happens after credential and persistence side effects.
- The intermediate MPRIS metadata snapshot still carries the remote token URL.
- Server-page dirty password text is retained across page switches.
- Existing log files can remain group/world-readable.

There was no evidence that `FERROSONIC_PASSWORD` is directly serialized by the
current normal save path, nor that the reviewed wire/D-Bus paths publish a
password or token.

## IPC assessment

The per-connection deadlock fix is correct. The server now aborts the event
forwarder before dropping its own writer sender and awaiting the writer task
(`src/ipc/server.rs:293-303`). The regression test uses a real socket client,
drops it, and waits for daemon idle eligibility
(`tests/ipc_server_edge.rs:92-128`).

Socket request timeout behavior is also structurally correct:

- the pending request is removed after timeout;
- monotonically unique IDs prevent a late response from completing another
  request;
- a late response is instead logged as an unknown response ID;
- socket EOF broadcasts `DaemonEvent::Shutdown`;
- app shutdown handling is idempotent.

Useful additional tests:

- Deliver a response after the timeout and prove no later request receives it.
- Assert the pending map returns to its baseline size.
- Exercise explicit daemon Shutdown followed by EOF and prove duplicate events
  do not produce inconsistent UI/error state.
- Distinguish intentional client teardown, where subscribers are usually gone,
  from a live UI losing its daemon.

## Quick Play feature assessment

The feature is well integrated with existing daemon, IPC, and UI boundaries.

Confirmed:

- Newest, recent, frequent, and highest album categories use the existing
  Subsonic client and standard `getAlbumList2` types.
- Category requests follow the selected music folder.
- Selecting a category album loads its complete track list.
- Empty category results clear the pane and render an explicit empty state.
- Server/folder generation changes prevent responses from an obsolete server
  scope being committed.
- Up/Down use the rendered option-column count and move a full wrapped row.
- Ctrl+R keeps and refreshes the active mode.
- Highlighted-song actions require song-pane focus.

Remaining gaps:

- Same-generation refreshes for the same category have no request-order guard.
- Mouse borders and leftover column cells do not map exactly to renderer
  geometry.
- Tests cover configuration-generation staleness, but not same-category
  reversed completion order.

## Claim-by-claim disposition

1. **Partial:** cancellation exists, but direct fallback branches can load after
   cancellation (`src/daemon/core.rs:846-855`, `874-996`).
2. **Confirmed:** duplicate automatic advancement is single-flight
   (`src/daemon/playback_ops.rs:167-205`); manual Next remains outside it.
3. **Confirmed:** mpv child detection, cleanup, reap, and restart behavior are
   implemented (`src/audio/mpv.rs:220-417`, `865-892`).
4. **Confirmed:** gapless advancement clears format/channels
   (`src/daemon/playback_tick.rs:219-232`).
5. **Confirmed:** disk-write error falls back to direct load
   (`src/daemon/core.rs:984-996`), subject to finding 1.
6. **Partial:** common config and queue/pre-buffer writes improved, but server
   save still blocks an async worker and music-folder save holds state
   (`src/daemon/settings_ops.rs:92-99`, `156-177`;
   `src/daemon/library_ops.rs:293-303`).
7. **Confirmed:** superseded scrobble capability probes are rejected
   (`src/daemon/scrobble.rs:61-87`).
8. **Confirmed:** connection teardown releases `ClientGuard`
   (`src/ipc/server.rs:293-303`; `tests/ipc_server_edge.rs:92-128`).
9. **Confirmed:** requests time out and EOF emits Shutdown
   (`src/ipc/socket_client.rs:105-115`, `150-175`).
10. **Confirmed:** environment passwords are omitted on disk
    (`src/config/mod.rs:106-111`, `315-330`, `680-689`).
11. **Confirmed narrowly:** backend keychain errors are surfaced and I/O is
    outside state (`src/daemon/settings_ops.rs:34-123`); transaction races remain.
12. **Confirmed:** secret-bearing config fields are scrubbed for IPC
    (`src/daemon/core.rs:452-490`).
13. **Partial:** directories are `0700`, new logs `0600`; existing log modes are
    retained (`src/config/paths.rs:64-72`; `src/bin/ferrosonic.rs:60-71`).
14. **Confirmed:** page switches preserve password text
    (`src/app/input.rs:423-447`), with the UX/documentation caveat above.
15. **Confirmed:** album paging handles short pages and loop protection
    (`src/subsonic/client.rs:558-591`).
16. **Confirmed:** Starred is scoped through `with_folder`
    (`src/subsonic/client.rs:463-468`).
17. **Partial:** common endpoints return sanitized `HttpStatus`, but artist,
    album, and playlist detail paths do not (`src/subsonic/client.rs:598-709`).
18. **Confirmed narrowly:** track IDs, repeat, volume clamp/round-trip, metadata
    getter, and pushed metadata are corrected (`src/mpris/server.rs:291-349`,
    `485-510`, `603-619`).
19. **Confirmed:** artist/album detail reconstruction retains cover art and
    metadata (`src/subsonic/client.rs:622-675`).
20. **Confirmed:** render and click paths share progress geometry
    (`src/ui/widget_now_playing.rs:249-305`; `src/app/mouse.rs:110-145`).
21. **Confirmed:** unbound Ctrl/Alt chords stop before page dispatch
    (`src/app/input.rs:397-405`).
22. **Confirmed:** mouse page switches share keyboard cleanup
    (`src/app/mouse.rs:48-62`; `src/app/input.rs:151-156`).
23. **Confirmed:** Ctrl+R retains the Quick Play selection
    (`src/app/input.rs:294-315`; `src/app/mod.rs:480-493`).
24. **Confirmed:** Quick Play navigation moves by wrapped row width
    (`src/app/input_songs.rs:22-67`, `153-179`).
25. **Confirmed:** Quick Play highlighting requires song-pane focus
    (`src/app/state.rs:173-184`).
26. **Confirmed:** `ConfigChanged` mirrors cava, daemon, and theme
    (`src/app/event_pump.rs:266-312`).
27. **Confirmed:** now-playing underflow and multibyte color panics are guarded
    (`src/app/mouse.rs:110-115`; `src/ui/theme.rs:91-107`).
28. **Confirmed:** Library mouse selection updates the song pane and queue
    cursor clamps after queue changes (`src/app/mouse_library.rs:175-189`;
    `src/app/event_pump.rs:56-67`).
29. **Confirmed:** dev-dependency, CI, service, mutants, duration formatting,
    README, and changelog updates are present.

## Behavior-change inventory

### Documented and appropriate

- Unbound Ctrl/Alt chords no longer fall through to destructive page actions
  (`CHANGELOG.md:43-45`, `README.md:324`).
- Quick Play Up/Down moves a complete grid row and Ctrl+R retains the active
  mode (`CHANGELOG.md:72-74`, `README.md:300-324`).
- MPRIS repeat is bidirectional and MPRIS volume round-trips/clamps
  (`CHANGELOG.md:69-71`).
- Long durations use `HH:MM:SS` (`CHANGELOG.md:64-65`).
- Starred results follow the active music folder (`CHANGELOG.md:56-57`).
- Daemon disconnect exits the TUI rather than leaving stale state, and requests
  have a 30-second limit (`CHANGELOG.md:75-77`).
- Remote token-bearing cover art is withheld until a local mirror exists
  (`CHANGELOG.md:88-90`).
- Environment passwords remain non-persistent and keychain backend failures are
  surfaced (`CHANGELOG.md:81-87`).

### Documented but incomplete

- All server 4xx/5xx errors are claimed to become sanitized `HttpStatus`
  (`CHANGELOG.md:54-55`), but three custom detail paths still parse the body
  without checking status.
- Owner-only logging is claimed generally (`CHANGELOG.md:91-92`), but only new
  log files receive `0600`.
- Superseded pre-buffers are claimed never to load after replacement/pause/stop
  (`CHANGELOG.md:30-32`), but failure fallbacks remain unsafe.

### Underdocumented or internally inconsistent

- Server-page password text is preserved while every other unsaved edit is
  reverted. `README.md:356` says unsaved edits are discarded without noting the
  exception.
- MPRIS volume is a local MPRIS cache rather than an authoritative player value;
  the documentation does not state that limitation.

## Test-quality assessment

### Strong tests

- `tests/ipc_server_edge.rs:92-128` uses a real client disconnect and waits for
  daemon idle eligibility. It would catch the original connection-task leak.
- Server configuration wire-scrub tests assert that secret-bearing fields are
  absent rather than merely testing a helper return value.
- Environment-password persistence tests exercise the disk projection.
- Focus and queue-cursor tests validate visible UI state transitions.

### Coupled or incomplete tests

- The progress-click test would catch restoration of the old fixed click
  geometry, so it is not wholly tautological. It derives expected geometry from
  the same helper used by production, however, so matching renderer/click
  mistakes can pass. Inspect a rendered buffer independently.
- The automatic-advance test proves the atomic flag but does not orchestrate
  actual EOF versus idle delivery or manual Next overlap.
- Pre-buffer tests cover a delayed happy-path direct replacement, not failure
  fallbacks, pause, stop, halt, or end-of-queue.
- Scrobble tests often manipulate the play-instance test seam directly. They do
  not prove that production playback transitions bump exactly once and do not
  cover resume at position zero.
- Some endpoint error tests assert only that an error occurs. They should assert
  `SubsonicError::HttpStatus` specifically.
- MPRIS metadata tests cover getter scrubbing but do not observe pushed D-Bus
  metadata end-to-end. One internal snapshot test explicitly expects the remote
  URL in the intermediate metadata structure.
- MPRIS volume tests omit NaN and changes from non-MPRIS control paths.
- Quick Play stale-response tests cover a config generation change but not two
  same-generation refreshes completing in reverse order.
- Quick Play mouse tests target valid cells but omit pane borders and unused
  trailing cells.
- No test begins with an existing permissive log file.
- No config test deterministically overlaps two persistence transactions.

## Verified checks at review time

The following commands were run at `5fe862d` on `personal-features`:

```text
cargo fmt --all -- --check
    PASS

cargo clippy --all-targets --all-features
    PASS (existing warning backlog remains)

cargo clippy --lib --bins --all-features \
    -- -D clippy::unwrap_used -D clippy::expect_used
    PASS

ulimit -n 16384 && \
    cargo nextest run --all-targets --locked --test-threads 4
    PASS: 1749/1749 tests

cargo test --doc --locked
    PASS: 37/37 doctests
```

`cargo deny check --all-features` was not run because `cargo-deny` was not
installed.

The working tree was clean before the report was created. The runtime race
findings were derived from code/control-flow analysis rather than reproduced by
adding instrumentation, because the review phase was read-only.

## Suggested actioning sequence for the next Codex session

1. Read `AGENTS.md`, `docs/LOCK-ORDER.md`, this report, and inspect current git
   status before editing.
2. Add deterministic failing tests for cancelled pre-buffer failure fallbacks.
3. Implement the cancellation-safe fallback helper and run the focused fake-mpv
   suite.
4. Add deterministic concurrent config transaction tests.
5. Introduce serialized config transactions, validate before persistence, and
   move music-folder save outside the state lock/async worker.
6. Recheck secret-storage permutations and on-disk permissions.
7. Add and fix automatic-advance versus manual-Next coverage.
8. Convert the remaining custom Subsonic methods to common HTTP status handling.
9. Address the lower-priority Quick Play, resume-at-zero, log-mode, password UX,
   and MPRIS state items in focused commits.
10. Run targeted tests throughout, then the complete verification set from
    `AGENTS.md`. Do not update snapshots without inspecting every change.

Keep the changes small and separable. In particular, avoid combining playback
transition serialization with config persistence changes in one commit: they
have different risk profiles and verification strategies.
