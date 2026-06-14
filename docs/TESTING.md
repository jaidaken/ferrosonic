---
description: Per-file test-coverage audit, gaps, and the gap-fill plan + progress ledger for ferrosonic. Read before adding test coverage or judging where coverage is thin.
tags: [testing, mutation, audit, coverage, plan]
date: 2026-06-14
---

# TESTING AUDIT + PLAN

scope: every `src/*.rs` file. goal = **92% mutation kill floor** (raised from M75 by user 2026-06-14) + A1 (1+ strong assert/test) THROUGHOUT, plus style-aware coverage for UI render. supersedes the missing `TESTING-PLAN.md` referenced by [mutation baseline](MUTATION-BASELINE.md).

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

### remaining (priority order)

- P1: confirm UI 5 files >=92 (run #3); then re-measure io_util, frame, queue_ops, secret (stale baseline) + fill.
- P2: T0 unmeasured (daemon core/playback_ops/playback_tick/library_ops/settings_ops/loaders/persistence/polling/run; ipc server/client/socket_client/path; subsonic client/auth/models; config/paths; error; proc_util).
- P3: T1 unmeasured (app input*/mouse*/event_pump/cava_pipe/mod/lifecycle/state/page_state/spawn_daemon; mpris/server).
- P4: T2 UI unmeasured (server/settings pages; widget_now_playing/footer/header/layout/theme/cover_art/chafa/widget_cava/quit_prompt).
- P5: update MUTATION-BASELINE.md all files; ui/ in nightly mutants matrix.
