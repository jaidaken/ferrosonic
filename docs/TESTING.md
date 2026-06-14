---
description: Per-file test-coverage audit, gaps, and the gap-fill plan + progress ledger for ferrosonic. Read before adding test coverage or judging where coverage is thin.
tags: [testing, mutation, audit, coverage, plan]
date: 2026-06-14
---

# TESTING AUDIT + PLAN

scope: every `src/*.rs` file. goal = **92% mutation kill floor** (raised from M75 by user 2026-06-14) + A1 (1+ strong assert/test) THROUGHOUT, plus style-aware coverage for UI render. supersedes the missing `TESTING-PLAN.md` referenced by [mutation baseline](MUTATION-BASELINE.md).

## MUTATION RUNNER GOTCHAS (learned 2026-06-14)

- ALWAYS `rm -rf /dev/shm/cargo-mutants-*` after killing a run. `pkill -9` orphans 3.3G/worker scratch dirs; they accumulate, fill the RAM tmpfs, and every build thrashes (a 115-mutant job took 13h instead of ~20min). check `df -BG /dev/shm` before launching.
- SCOPE THE TEST PHASE: `MUTANTS_EXCLUDE='binary(x) | binary(y) | ...'` (positive filter of the file's relevant test binaries) cuts the per-mutant test phase from ~20s to ~4s. the unmutated baseline must pass under that scope. build time (~30-70s/mutant) then dominates; ~115 mutants / 4 workers ~= 20min.
- kill flaky baseline tests via the exclude (e.g. `test(mpris_handler_off_tokio_runtime_does_not_panic)`); a red baseline aborts the whole run.
- SCOPE TOO NARROW = FALSE SURVIVORS: if a mutant's killing test is NOT in the scoped set, it reports MISSED spuriously (core.rs:818 release_pipewire_rate showed missed because `pipewire_pin_lifecycle` was excluded). central files (core.rs) are exercised by many subsystems' tests; run them with the DEFAULT exclude (full suite) for accuracy, accept the slower test phase. only scope narrow for leaf files with few exercising binaries.

## METHOD

- ORACLE = `cargo mutants --file <f>` kill rate. **FLOOR = 92%** (every file at or above; provably-equivalent survivors documented + excluded, never silenced). coverage % = triage only (async per-monomorphization counting misleads; see testing rules).
- A1 = every test fn has 1+ strong assert (value/state/error-kind/snapshot), smoke exception only for `_renders_without_panic` w/ no observable effect.
- UI render = STYLE-AWARE (cell fg/bg/modifier), not text-only. text-only harness hid the song-pane focus leak (fixed `2127efd`); style harness added `088f8a8` (`tests/common/render.rs` `StyledScreen`).
- per-file mutation run: background via `scripts/mutants.sh --file <f>`; triage survivors -> add asserts -> re-run -> record %.

## STATUS LEGEND

`OK` >=92% measured. `LOW` <92% measured. `?` unmeasured. `style?` render file, style-coverage unknown.

## BELOW 92 FLOOR (measured), P1 targets

`io_util.rs` 60, `ipc/frame.rs` 72.7, `queue_ops.rs` 80.6, `secret.rs` 88.9, UI render 5 files ~61. `daemon/state.rs` 92.3 = borderline (recheck). all need raising to >=92.

## MUTATION DATA (measured)

from [MUTATION-BASELINE.md](MUTATION-BASELINE.md) (2026-05-15) + UI run (2026-06-14):

```
input_server.rs   100%    config/mod.rs    94.4%   audio/mpv.rs     94.8%
audio/pipewire.rs 93.5%   daemon/state.rs  92.3%   daemon/library   100%
secret.rs         88.9%   queue_ops.rs     80.6%   daemon/library   100%
ipc/protocol.rs   n/a(0 viable mutants)            ipc/frame.rs     72.7% LOW
io_util.rs        60%  LOW
ui/pages (library,playlists,songs,queue) + styled_lines  61% aggregate LOW (24 survivors)
```

## PER-FILE MATRIX

tier: T0 = data/state/protocol/sync/security (fresh-eyes + dangerous-prop test required). T1 = app/orchestration logic. T2 = presentation/render.

### T0 core (daemon / ipc / audio / config / secret / io)

| file | loc | mut | test files (primary) | gap/action |
|---|--:|---|---|---|
| daemon/core.rs | 825 | ? | daemon_actions, daemon_play_queue, lock_order, state_invariant | measure; likely partial |
| daemon/playback_ops.rs | 408 | ? | pause_resume_flow, playback_resume, next_prev_flow, stop_keeps_queue | measure |
| daemon/playback_tick.rs | 655 | ? | playback_tick_regression, playback_poll, daemon_polling | measure |
| daemon/queue_ops.rs | 157 | 80.6 OK | queue_mutations, enqueue_songs, remove_from_queue | 7 survivors -> raise |
| daemon/library_ops.rs | 307 | ? | library_cache, refresh_flows, star_toggle | measure |
| daemon/settings_ops.rs | 177 | ? | settings_setters, daemon_setters | measure |
| daemon/loaders.rs | 138 | ? | daemon_with_real_subsonic, refresh_flows | measure |
| daemon/library.rs | 176 | 100 OK | library_cache, lru_cache_proptest | hold |
| daemon/state.rs | 136 | 92.3 OK | state_invariant, apply_event_* | hold |
| daemon/persistence.rs | 63 | ? | queue_persistence | measure |
| daemon/polling.rs | 71 | ? | daemon_polling, playback_poll | measure |
| daemon/run.rs | 89 | ? | ferrosonicd_boot, sigterm_graceful_exit, ipc_shutdown_exits | measure |
| ipc/frame.rs | 334 | 72.7 LOW | ipc_frame_full, ipc_frame_boundary, fuzz_ipc_frame | 6 survivors -> raise |
| ipc/protocol.rs | 278 | n/a | ipc_roundtrip, ipc_frame_proptest | no viable mutants; hold |
| ipc/server.rs | 285 | ? | ipc_server_edge, ipc_server_misbehaved_clients, lock_order | measure |
| ipc/client.rs | 383 | ? | ipc_roundtrip, daemon_actions (in-process), ipc_shutdown_exits | measure |
| ipc/socket_client.rs | 145 | ? | ipc_socket_client_full, ipc_socket_client_unknown_responses | measure |
| ipc/path.rs | 71 | ? | ipc_path_helpers, ipc_path_branches | measure |
| audio/mpv.rs | 648 | 94.8 OK | mpv_controller_full, mpv_scripted_failures, mpv_failure_modes | hold; loadfile_at new |
| audio/pipewire.rs | 217 | 93.5 OK | pipewire_logic, pipewire_pin_lifecycle, pipewire_with_runner | hold |
| config/mod.rs | 736 | 94.4 OK | config_validate_and_save, config_proptest, fuzz_config_toml | hold |
| config/paths.rs | 60 | ? | config_paths_full | measure |
| subsonic/client.rs | 431 | ? | subsonic_client_endpoints, subsonic_errors, daemon_with_real_subsonic | measure |
| subsonic/auth.rs | 76 | ? | (doctest), subsonic_client_endpoints | measure |
| subsonic/models.rs | 351 | ? | fuzz_subsonic_response | measure (mostly derives) |
| secret.rs | 266 | 88.9 OK | password_redaction, config_password_resolution | 3 survivors -> opt |
| io_util.rs | 444 | 60 LOW | queue_persistence, tmp_sweep | fsync + atomic_write survivors |
| error.rs | 163 | ? | (indirect) | measure (mostly Display) |
| proc_util.rs | 27 | ? | process_death_signal | measure |

### T1 app / orchestration / mpris

| file | loc | mut | test files (primary) | gap/action |
|---|--:|---|---|---|
| app/input.rs | 239 | ? | input_dispatch, input_top_level_full, input_quit_confirm, handle_event_full | measure (8 UI survivors seen) |
| app/input_library.rs | 734 | ? | input_library_* (12 files), library_playing_album | measure |
| app/input_playlists.rs | 214 | ? | input_playlists_* | measure |
| app/input_queue.rs | 170 | ? | input_queue_deep, input_queue_full | measure |
| app/input_server.rs | 191 | 100 OK | input_server_* (6 files) | hold |
| app/input_settings.rs | 238 | ? | input_settings_deep, input_settings_full | measure |
| app/input_songs.rs | 127 | ? | input_songs_actions, input_songs_full | measure |
| app/mouse.rs | 411 | ? | mouse_dispatch, mouse_scroll_full, mouse_* | measure |
| app/mouse_library.rs | 221 | ? | mouse_library_* (4 files) | measure |
| app/mouse_playlists.rs | 104 | ? | mouse_playlists_full | measure |
| app/event_pump.rs | 283 | ? | apply_event_*, event_loop_*, seed_cover_art_and_pump | measure |
| app/event_source.rs | 56 | ? | event_source_full, event_loop_with_source | measure |
| app/cava_pipe.rs | 407 | ? | cava_drain, cava_helpers, fuzz_cava_vt100 | measure |
| app/mod.rs | 492 | ? | app_lifecycle, run_setup_seams, ferrosonic_tui_end_to_end | measure |
| app/lifecycle.rs | 90 | ? | terminal_guard, quit_listener, signal_quit | measure |
| app/state.rs | 190 | ? | state_invariant (indirect) | measure |
| app/client_state.rs | 76 | ? | (indirect) | measure (mostly fields) |
| app/page_state.rs | 204 | ? | (indirect via input/render) | measure |
| app/spawn_daemon.rs | 64 | ? | ipc_spawn_full, ipc_spawn_real_binary | measure |
| app/models.rs | 12 | ? | (trivial enum) | low priority |
| mpris/server.rs | 426 | ? | mpris_dispatch, mpris_remaining, mpris_property_snapshot, mpris_cover_art_url | measure |

### T2 ui / render

| file | loc | mut | test files (primary) | gap/action |
|---|--:|---|---|---|
| ui/pages/library.rs | 323 | 61 LOW | ui_pane_focus, ui_indicator_render, library_search_render | survivors: tree filter, disc, selection == |
| ui/pages/playlists.rs | 198 | 61 LOW | playlists_render, ui_pane_focus | survivors: duration math, selection == |
| ui/pages/songs.rs | 128 | 61 LOW | ui_pane_focus | survivors: selection ==, is_playing == |
| ui/pages/queue.rs | 126 | 61 LOW | ui_queue_render_states, ui_indicator_render | survivors: is_current ==, < |
| ui/pages/server.rs | 173 | ? | ui_server_page_render, form_pages_dont_blank | measure (style-aware?) |
| ui/pages/settings.rs | 192 | ? | input_settings_*, form_pages_dont_blank | measure (style-aware?) |
| ui/styled_lines.rs | 104 | 61 LOW | (via page renders) | survivor: track-number match arm |
| ui/widget_now_playing.rs | 280 | ? | now_playing_widget_full, ui_populated | measure (style-aware?) |
| ui/footer.rs | 190 | ? | ui_smoke, ui_populated | measure |
| ui/header.rs | 167 | ? | ui_smoke | measure |
| ui/layout.rs | 149 | ? | ui_layout_branches, ui_smoke | measure |
| ui/theme.rs | 284 | ? | theme_loading | measure |
| ui/theme_builtins.rs | 292 | ? | theme_loading | measure (mostly data) |
| ui/cover_art.rs | 295 | ? | cover_art* (6 files) | measure |
| ui/chafa_ext.rs | 256 | ? | chafa_encoding, cover_art_chafa_* | measure |
| ui/widget_cava.rs | 63 | ? | cava_widget_full | measure |
| ui/quit_prompt.rs | 58 | ? | (via layout) | measure (new, smoke only) |

## PLAN (phases, highest-risk-first)

P1 BELOW-FLOOR FIXES (known survivors): io_util.rs (60), ipc/frame.rs (72.7), queue_ops.rs (80.6), secret.rs (88.9), UI render 5 files (61), daemon/state.rs (92.3 recheck). raise each to >=92.
P2 T0 UNMEASURED: daemon/core, playback_ops, playback_tick, library_ops, settings_ops, loaders, persistence, polling, run; ipc/server, client, socket_client, path; subsonic/client, auth, models; config/paths; error; proc_util. measure -> fill -> >=75.
P3 T1 UNMEASURED: app/input* (non-server), mouse*, event_pump, event_source, cava_pipe, mod, lifecycle, state, page_state, spawn_daemon; mpris/server. measure -> fill.
P4 T2 UI UNMEASURED: server/settings pages, widget_now_playing, footer, header, layout, theme, cover_art, chafa, widget_cava, quit_prompt. style-aware where render. measure -> fill.
P5 LOCK-IN: update MUTATION-BASELINE.md w/ all files; add ui/ to nightly mutants matrix; record final per-file %.

## LEDGER (per-file completion)

append `file | date | before% -> after% | commit` as each file reaches the floor.

### work log (2026-06-14)

- UI render foundation: style-aware harness + focus/indicator/border invariants `088f8a8`.
- UI focus-gate tighten (mutation found differential too loose) `17c7172`.
- UI selection-row + content/format coverage `aa678d5`.
- UI library search-result format + search title `(committed)`.
- playback state-machine property test (control-sequence invariants) `(committed)`.
- UI targeted mutant kills (glyph/bold/accent/disc discriminators; theme colour collisions noted) `(committed)`.
- P1 UI files (library/playlists/songs/queue/styled_lines): 61% -> 83.6% (run #3) -> ~96% expected (run #4 confirming). 2 provably-equivalent mutants remain (queue `<`->`<=` at the current index; library 193 `&&`->`||` unreachable filter state).

### done

- **playback_tick.rs: 16 survivors -> 11 killed + 1 equiv + 4 known-open** (55 mutants). killed: decide boundaries 140/141/142/145 (exact-value inline tests in `playback_tick_tests`), tr arithmetic 83 + has_next 86 (AdvanceEarly-vs-Preload queue_position discriminator) + tick_fetch 302 (`tests/daemon_playback_tick.rs`). equiv: 291 backfill-0.0 (exclusions doc). KNOWN-OPEN (seam-required, final depth pass): 111 just_loaded 1500ms boundary (std::time::Instant, tokio fake-time does not reach it; needs a clock-injection seam), 216/221 bump_preload_due debounce (5s suppression unobservable because the first preload changes playlist_count off the Preload path; needs a playlist-count-pinned harness).
- **playback_ops.rs: 100% of killable** (44 mutants, was 9 missed; 5 killed + 4 equivalent). `c61884e` core.rs cheap kills, `4a4c3d5` playback_ops. kills: prev 3s boundary (155), resume offset commit (223 ==/<), seek (384), seek_relative (397). equiv: 223 >=, 260 log-only. `tests/daemon_seek_resume.rs`.
- **core.rs cheap real kills: 378/391/235** (9/9 scoped verify). rest = seam-required known-open (see CURRENT). `tests/daemon_startup_sweep.rs` + `daemon_core_effects.rs`.
- UI list render (library/playlists/songs/queue/styled_lines): **98.4%** (60/61, run #4 + 121 fix). 1 provably-equivalent mutant (queue `<`->`<=`).
- playback state-machine property test.
- daemon core.rs partial kills (PREPPED, verify in overnight core run): broadcast_now_playing, emit_config_changed, bump_library_version (`tests/daemon_core_effects.rs`), dispatch_play `>` boundary (`tests/playback_resume.rs`).
- flaky `mpris_handler_off_tokio_runtime` excluded from mutation baseline (passes solo, fails under parallel load; CLAUDE.md known-flaky). FIX-LATER: real flakiness.

### CURRENT (resume here, updated)

- **DECISION RESOLVED (user: "whatever is the most professional, complete way forward"):** kill core.rs's cheap real survivors now, log the seam-required real gaps as known-open (NOT as equivalents, they change behaviour), then sweep the ~55 unmeasured files breadth-first, returning for the expensive core.rs seams as a final depth pass.
- core.rs CHEAP REAL KILLS LANDED (scoped verify `/tmp/core-kill-verify.log`: 9/9 caught): 378 bump_library_version value (assert emitted == 1, `tests/daemon_core_effects.rs`), 391 extend_with_random_and_play empty-guard (auto-continue error notification, same file), 235 sweep_orphan_prebuffer_files (`tests/daemon_startup_sweep.rs` backdated orphan fixture).
- core.rs KNOWN-OPEN (REAL gaps, seam-required, for final depth pass, do NOT exclude as equivalent):
  - RAII guards 43/49/70/76/98/104 (LoadingFlagOwner/PrebufferGate/CancelSlotCleaner): protect a track-switch cancel race; outer owner.disarm prevents clobbering a newer task's loading flag. needs a concurrent rapid-switch cancel-race harness (loom or staged Buffered plays).
  - event listener 278 (`reason != "eof"`) / 291 (`count >= 2`): mpv EOF to auto-advance gapless gating. needs FakeMpv unsolicited-event injection seam (push `{"event":"end-file","reason":...}` to the connected client; MpvController reader broadcasts it).
  - prebuffer streaming 606/619/680/693/705/707: detached HTTP-streaming task; threshold/trigger mutants change buffering latency but load the same song. needs a >512KB FakeSubsonic byte-stream + chunk-timing control. low correctness value (perf-timing, not song selection).
  - start_mpv 252 (`-> Ok(())`): spawns the real mpv process; test harness pre-connects via connect_to_existing. real-process e2e only (mpv binary present).
  - config_gen_changed 366 (`-> false`): stale-refresh gate in library_ops; needs a config_gen-bump-mid-refresh race seam.
  - dispatch_play 514 (`&&`->`||`): best-effort stop before reload; mutant sends an extra harmless stop when idle (same end state). killable by asserting the command stream; low value.
  - config_gen_for_test 371: `#[doc(hidden)]` test-only accessor, no production caller, add `#[mutants::skip]` + exclusions entry.
- NEXT: playback_ops.rs (heavily tested already, measure, expect strong), playback_tick.rs, then ipc/subsonic/audio re-verify/app/ui-remaining per priority list below.

### OLD CURRENT

- daemon small-files TESTS WRITTEN (committed): daemon_star_sync (apply_star_to_cached/sync_starred_songs + song.id==id), daemon_queue_ops_more (move-position adjust, shuffle_library body+guard, shuffle_queue), daemon_core_effects (broadcast_now_playing, emit_config_changed, refresh playlists/starred/random/artists events). playback_resume +zero-offset boundary.
- RUNNING scoped verification `/tmp/daemon-small3.log` (test phase scoped to daemon binaries, ~20min). expect most survivors killed; known-equivalent: persistence:24 + run:85 (NotFound guard = log-only, same return), queue move 69/71 + shuffle_queue 146 (boundary guards for states the queue-position invariant prevents).
- DEFERRED: settings_ops:24 (password_file filter; update_server_config blocks ~10s on connection probe, fiddly).
- NEXT: big-daemon run = core.rs (~30 survivors in `/tmp/daemon-mutants.log`, 4 pre-killed), playback_ops.rs, playback_tick.rs. USE scoped test phase + clean scratch.

### daemon T0 batch (RUNNING `/tmp/daemon-mutants.log`, 285 mutants ~3h)

core.rs shows a high early miss rate: integration tests exercise it (coverage) but assert weakly. survivor categories + how to kill:

- VOID-FN SIDE EFFECTS (`fn -> ()`): broadcast_now_playing[done], emit_config_changed[done via set_cava_enabled], bump_library_version (needs FakeSubsonic artists; extend refresh_flows), quit_mpv (assert FakeMpv got "quit"; needs mpv started), broadcast/emit/sweep. KILL = subscribe + assert the event/command. `tests/daemon_core_effects.rs` is the pattern.
- RAII GUARDS (LoadingFlagOwner/PrebufferGate/CancelSlotCleaner disarm+drop, core.rs:43-104): cleanup w/ no observable user-space effect. likely PROVABLY-EQUIVALENT or need a concurrency/cancel test. investigate; exclude w/ doc if equivalent.
- ARITH/CMP BOUNDARIES: dispatch_play 479 (`>`->`>=` start_at=0 boundary; KILL via play_queue_position_at(_,_,0.0) asserts plain loadfile no start=), 514 (&&->||), prebuffer 606/619 math, bump 378 (+->*). KILL = boundary-value tests.
- MPV LIFECYCLE: start_mpv, spawn_mpv_event_listener 278/291 (needs started-mpv harness).

approach: let batch finish -> full survivor list -> group by category -> write effect/boundary tests -> re-run daemon files to verify -> document equivalents. then P2 ipc/subsonic, P3 app, P4 ui-remaining.

### remaining (priority order)

- P2 daemon: triage full survivor set (above) to >=92.
- P2 ipc/subsonic/misc batch: ipc server/client/socket_client/path/frame, subsonic client/auth/models, config/paths, secret, io_util, error, proc_util.
- P3 T1 app: input*/mouse*/event_pump/cava_pipe/mod/lifecycle/state/page_state/spawn_daemon; mpris/server.
- P4 T2 ui-remaining: server/settings pages, widget_now_playing/footer/header/layout/theme/cover_art/chafa/widget_cava/quit_prompt.
- P1: re-measure io_util, frame, secret (stale baseline) + fill.
- P5: update MUTATION-BASELINE.md; ui/ + daemon in nightly mutants matrix.
- P2: T0 unmeasured (daemon core/playback_ops/playback_tick/library_ops/settings_ops/loaders/persistence/polling/run; ipc server/client/socket_client/path; subsonic client/auth/models; config/paths; error; proc_util).
- P3: T1 unmeasured (app input*/mouse*/event_pump/cava_pipe/mod/lifecycle/state/page_state/spawn_daemon; mpris/server).
- P4: T2 UI unmeasured (server/settings pages; widget_now_playing/footer/header/layout/theme/cover_art/chafa/widget_cava/quit_prompt).
- P5: update MUTATION-BASELINE.md all files; ui/ in nightly mutants matrix.
