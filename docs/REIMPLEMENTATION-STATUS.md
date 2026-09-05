# Custom feature reimplementation status

Implemented on upstream `39abd072167e585c0016747a770b32337c9782ab`, 2026-09-05.
Recovered the custom implementation from local commits `daad264` and `7d20e96`
as an uncommitted patch, retaining upstream dependencies and metadata.

- [x] Song ratings: API, playing-song keys, cached copies, UI suffixes, rollback,
  events, and MPRIS metadata.
- [x] Playback filters: configuration, Settings, queue insertion/replacement,
  original-occurrence index preservation, shuffle and auto-continue filtering.
- [x] Random Album Quick Play: keyboard/mouse selection, folder-scoped API,
  separate cache, snapshots, star/rating synchronization, stale-empty protection.
- [x] Configurable global keys: defaults and overrides, persistence, conflict
  notifications, reserved keys, modal/Settings priority, page-edit cleanup.
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

Random Album refreshes when switching into that option from another option.
Returning to F3 or clicking the selected option keeps the current album; this
is deliberate and documented in README. The original analysis checklist
remains a historical description, not a runtime verification report.

Tests cover the restored features plus socket requests/reconnect, small Settings
layouts, duplicate caches, stale responses, MPRIS parity, and real-mpv
ReplayGain startup/live/restart properties. See the workspace-root HANDOFF.md
for logs and manual verification limitations.

Verification (2026-09-05, final run):

- `cargo fmt --all -- --check` and `git diff --check`: pass.
- `cargo clippy --all-targets --all-features`: passes with warnings.
- `cargo clippy --lib --bins --all-features -D clippy::unwrap_used
  -D clippy::expect_used`: passes with 34 production warnings. The untouched
  upstream baseline produces the same 34, so this work adds none. They concern
  pre-existing wildcard and async-trait code under this toolchain; do not
  clean them up as part of this feature work.
- `cargo test --doc`: 37 passed.
- `cargo nextest run --profile ci --all-targets --test-threads 4`:
  **1,659 of 1,659 passed**.

An earlier run of the same suite failed
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
  1,558/1,558 (54.0 s) on the baseline.

Treat these two tests as load-sensitive on a desktop machine. Run the suite
with a raised descriptor limit and modest concurrency, and re-check a failure
in isolation before treating it as a regression.

Live listening and interactive UI smoke checks were not performed.
