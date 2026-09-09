# Custom feature reimplementation status

Implemented on branch `personal-features`, based on upstream
`39abd072167e585c0016747a770b32337c9782ab`, beginning 2026-09-05. The branch
contains the feature commit `06a2a44` plus six focused correctness commits
through `06fa94a`; upstream dependencies and metadata are retained.

- [x] Song ratings: API, playing-song keys, cached copies, UI suffixes, rollback,
  events, and MPRIS metadata.
- [x] Playback filters: configuration, Settings, queue insertion/replacement,
  original-occurrence index preservation, shuffle and auto-continue filtering.
- [x] Random Album Quick Play: keyboard/mouse selection, folder-scoped API,
  separate cache, snapshots, star/rating synchronization, stale-empty protection.
- [x] Configurable global keys: defaults and overrides, persistence, conflict
  notifications, reserved keys, modal/Settings priority, page-edit cleanup.
- [x] In-app excluded-genre/artist editors: staged add/remove, validation,
  atomic persistence, cancellation, and narrow-terminal rendering.
- [x] In-app global-keybinding editor: chord capture, conflict rejection,
  one/all reset, daemon IPC persistence, immediate application, and effective
  footer hints.
- [x] Responsive footer: measured height and complete wrapped shortcut pairs on
  narrow/tall terminals, retaining notifications and sample-rate status.
- [x] Responsive layout pass: measured wrapping for all header tabs and
  transport controls, vertically stacked two-pane pages at narrow widths,
  shared render/mouse rectangles, and bounded cava/cover-art space.
- [x] Lyrics overlay: OpenSubsonic structured/synchronized lyrics, classic
  fallback, client-side caching, exact synchronized follow, proportional
  untimed follow, manual scrolling, source selection, and responsive
  loading/empty/error views.
- [x] ReplayGain: configuration, Settings, startup and live mpv properties,
  preamp clamping, clipping inversion, latest values retained on restart.

The MPRIS getter and pushed updates now share their metadata builder. `Shift+t`
and `T` parse identically. Unreachable reserved-key overrides are reported.

Follow-up correctness review:

- Rating RPCs are serialized independently of playback, preserving successful
  writes and event order when earlier requests are slow or fail. Results from
  an old server generation cannot roll back or update the new server's caches.
- Rating events update MPRIS while paused, using the event value even when the
  TUI's state mirror has not yet processed the event.
- Non-finite ReplayGain values are rejected during config parsing, validation,
  persistence, and live updates. Direct controller startup calls retain the
  last valid preamp with a warning if given a non-finite value.
- Regression tests exercise concurrent failure/success ordering, server
  changes during a request, invalid gains, and complete client-cache/Settings
  synchronization. Rating and invalid-gain defects were reproduced with
  failing tests before their fixes.
- Server base URLs retain an existing path prefix, rating failures reach the
  user, `Alt+1`-`Alt+5` rate the highlighted song, Library search rows update
  after star/rating events, and queue copies of ratings/stars persist across a
  daemon restart.

Random Album refreshes when switching into that option from another option.
Returning to F3 or clicking the selected option keeps the current album; this
is deliberate and documented in README. The original analysis checklist
remains a historical description, not a runtime verification report.

Tests cover the restored features plus socket requests/reconnect, small Settings
layouts, duplicate caches, stale responses, MPRIS parity, and real-mpv
ReplayGain startup/live/restart properties. See the workspace-root HANDOFF.md
for the detailed verification record.

Verification (2026-09-07, stages 1 through 4 checkpoint):

- `cargo fmt --all -- --check`: pass.
- `cargo clippy --all-targets --all-features`: passes with warnings.
- `cargo clippy --lib --bins --all-features -D clippy::unwrap_used
  -D clippy::expect_used`: passes with 34 production warnings. They are
  warning-level wildcard, lock-scope, and async-trait findings under this
  toolchain; do not broaden this feature work into a lint cleanup.
- `cargo test --doc`: 37 passed.
- `cargo nextest run --profile ci --all-targets --test-threads 1`: 1,704/1,704
  passed in 215.411 seconds. The two known host-load-sensitive concurrency
  checks passed on their third attempts and also passed together in a focused
  single-thread run.
- `cargo build --release`: pass; `target/release/ferrosonic` rebuilt at 19:45
  (13,400,248 bytes; SHA-256
  `873be2ad1713822eced6dbd00f2c7c3d0387051a338dbda98171c240e0e186f7`).
- `cargo deny check --all-features` was not run locally because `cargo-deny`
  is not installed. No dependency files changed; the configured hosted CI job
  remains the final check for this gate.

Earlier runs of the same suite failed
`lock_order::pause_resume_under_replace_storm_stays_consistent` on all three
retries and marked `state_invariant::r1_toggle_pause_state_stays_consistent_under_replace`
flaky. Both are pre-existing upstream stress tests, byte-identical to the
baseline copies, and the cause was host load on a busy desktop rather than a
change in this tree:

- Alternating ten unloaded A/B runs of the storm test measured 0.74-0.84 s on
  both trees, with no separation between them.
- The untouched baseline shows the same spread across whole-suite runs
  (0.278 s in one, 18.157 s in another) and this tree passes the test in
  0.283 s in isolation.
- A back-to-back pair of full suites finished 1,659/1,659 (55.1 s) here and
  1,558/1,558 (54.0 s) on the baseline during the initial feature review. The
  earlier post-fix suite was independently green at 1,676/1,676.

These tests remain sensitive to extreme desktop load. The post-removal suite
passed both on retry, and its focused rerun passed in 0.278 and 0.201 seconds.
Run the suite with a raised descriptor limit and modest concurrency, and
re-check future timing failures on an idle host.

Live verification against Navidrome is complete for all five feature areas.
The final rating-persistence fix was exercised with a real `setRating` request:
the isolated queue stored the latest value (5), a forced daemon restart restored
all 15 queue items, and the restored song retained rating 5.

Playback smoke testing covered daemon-backed and standalone modes, TUI
reconnection to a playing daemon, pause/resume, live ReplayGain changes, two
natural gapless advances, and PipeWire switching among 44.1, 88.2, and 96 kHz.
The listener confirmed clean audio, seamless transitions, and expected gain
behavior. Test profiles were isolated; the normal config and installed binary
were unchanged.

Roadmap stages 1 through 4 are implemented and live-verified. The user confirmed
the editors, responsive layouts, and lyrics retrieval/rendering and following.
The previously verified audiobook integration was subsequently removed at the
user's request. Audiobook and direct-RSS podcast work has moved to the separate,
portable Podsonic planning package at the workspace root. This work is recorded
on the `personal-features` branch and has not been pushed or deployed; the
installed binary and personal configuration are unchanged.
