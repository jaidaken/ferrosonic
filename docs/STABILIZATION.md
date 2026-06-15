# Ferrosonic Stabilization Plan

Audit consolidation produced 2026-05-14 against HEAD `ab8d2e1`. Inputs:

1. `docs/AUDIT-2026-05-13.md` (every line item marked `[x]` fixed).
2. `git log --oneline -40` (audit-driven fix history from `1d16ee5` to
   `ab8d2e1`).
3. Twelve parallel `/rust-audit` Explore agents, one per file group,
   each scanning for R1 to R15 plus the category tags PASSWORD,
   IPC_PROTOCOL, LOCK_ORDER, STATE_INVARIANT, RESOURCE_LEAK,
   INPUT_VALIDATION, ERROR_PATH, TEST_QUALITY, BUILD_HYGIENE, PERF,
   COSMETIC.

Output rules: deduplicated, no fixes written, plan only. Drives prompts
2 through 10 of the stabilization sprint.

---

## STATUS RE-BASELINE 2026-06-15 (HEAD `3896a62`)

Verified prompts 4-10 against current code + git ancestry (grep + commit
check, not a fresh audit). Prompts 5 and 6 over-delivered via the
content-driven testing phase; several prompt-7 structural fixes landed
under a DIFFERENT mechanism than this plan proposed. Sections 1-6 below
are retained as the original audit reference; THIS section is the
authoritative current status.

### Prompt status

- **P2 LOCK_ORDER** DONE. `docs/LOCK-ORDER.md` + `tests/lock_order.rs`.
- **P2.5 STATE_INVARIANT** DONE. `tests/state_invariant.rs`.
- **P3 PASSWORD** DONE. `src/secret.rs`, 4 highest-traffic sites migrated.
- **P4 IPC_PROTOCOL** PARTIAL.
  - done: frame caps (`MAX_FRAME_BYTES` 16MiB + tighter `MAX_REQUEST_FRAME_BYTES`, `frame.rs:11-13`); unknown-variant leniency (version skew no longer severs the connection); frame-boundary + fuzz tests (`tests/ipc_frame_boundary.rs`, `tests/fuzz_ipc_frame.rs`).
  - open: `Hello`/`protocol_version` handshake; per-connection idle timeout; `CancelRequest`; per-frame version tag; `Resync`-on-`Lagged` event. None present in `src/ipc/`.
- **P5 PROPERTY + INTEGRATION TESTS** DONE, over-delivered. 1377 tests: queue/playback/stress proptest, `state_invariant`, `lock_order`, `password_redaction`, plus security + daemon integration tests. Exceeds original scope.
- **P6 FUZZ** DONE via bolero (NOT cargo-fuzz; `CLAUDE.md` rule 8 ratified the switch). Targets: cava vt100, config TOML, IPC frame, subsonic response, mpv reply.
- **P7 HIGH CATCH-ALL** PARTIAL.
  - done, alt mechanism: task-outlives-shutdown leak solved by `shutdown: AtomicBool` + `shutdown_signal()` checked every spawn loop (`core.rs:173,574`; `CLAUDE.md` rule 3), NOT the `CancellationToken` this plan named. Subprocess orphans (mpv/cava) solved by `PR_SET_PDEATHSIG(SIGKILL)` + `Drop` kill (`1c88f0a`, `d38a75a`).
  - open/low-value: cava raw-FD RAII guard (`cava_pipe.rs` still `from_raw_fd` without a guard); mpv reader single-line framing (works in practice: mpv emits one JSON per line; parser is fuzz-guarded); `queue.json` 0o600 (now in the config dir not `/tmp`, so low severity; song ids are not secrets).
  - false-positive: mpv `send_command` multi-lock on `pending` is safe; request ids are unique (`AtomicU64`), so no wrong-oneshot demux. No fix.
- **P8 MEDIUM/LOW TRIAGE** NOT done as a formal pass. No `KNOWN-ISSUES.md`. Residue = the 847-warning pedantic/nursery clippy backlog.
- **P9 CI GATES** PARTIAL.
  - done: `test.yml` + `release.yml` exist; `deny.toml` (cargo-deny); nightly cron.
  - open: CI triggers are `workflow_dispatch` + cron only, NOT push/PR (`test.yml:6-8`); clippy `unwrap_used`/`expect_used` still `warn` not `deny` (`Cargo.toml:147-148`; `CLAUDE.md` rule 2 treats as deny manually + a CI grep backstop).
- **P10 RELEASE** not started.

### New finding (not in the original audit)

- **TEST-FIXTURE /tmp LEAK.** Tests call `tempfile::tempdir()` (135 call sites) producing `/tmp/.tmpXXXXXX/`. `TempDir`'s `Drop` is skipped on SIGKILL (nextest timeout, cargo-mutants group-kill, `panic=abort`), so the dirs survive. 9,594 present 2026-06-15, accumulating. The production mpv socket no longer leaks (fixed path, `paths.rs:27`). Fix = design choice (relocate test roots under a swept prefix vs janitor pass).

### Worth doing, value-ranked

1. **CI on push/PR** (P9). Nothing auto-gates regressions today. Low effort (edit `test.yml` triggers).
2. **Test-fixture /tmp leak.** Active, accumulating. Fix needs a call (135 sites).
3. **IPC per-connection idle timeout** (P4). Real: a hung client holds a writer task forever.
4. **clippy `unwrap`/`expect` to deny** (P9). Closes the gap between `CLAUDE.md` rule 2 (manual) and machine enforcement. Blocked on triaging the 847 pedantic backlog OR scoping deny to just those two lints.
5. **Release cut** (P10) once the above settle.

### Low-value / defer (localhost single-user IPC; defense-in-depth)

- P4 `Hello` handshake, per-frame version tag, `CancelRequest`, `Resync`-on-`Lagged`.
- P7 cava `RawFdGuard`, mpv length-prefixed framing, `queue.json` 0o600.
- P8 formal MEDIUM/LOW triage + `KNOWN-ISSUES.md`.

---

## 1. Open findings (deduplicated)

Severity tiers: HIGH (real bug observable today), MEDIUM (race / leak /
divergence reachable under load), LOW (cosmetic, defense-in-depth, or
fragile-but-not-broken).

### 1.1 daemon/core.rs (2149 lines, hottest file)

| file:line | sev | category | description | fix sketch |
|---|---|---|---|---|
| core.rs:111-119 | HIGH | RESOURCE_LEAK | `CancelSlotCleaner::drop` spawns task holding `Arc<Self>`; blocks shutdown | Synchronous slot clear in Drop, no spawn |
| core.rs:322-327 | HIGH | STATE_INVARIANT | `broadcast::Lagged` triggers idle-probe instead of an explicit Resync event to the client (R15) | Push `DaemonEvent::Resync` carrying snapshot |
| core.rs:984 | HIGH | STATE_INVARIANT | `commit_play_state_in_lock` sets `now_playing.state = Playing` before `dispatch_play` issues `mpv.loadfile` (R2) | Set Playing only after loadfile success, or gate observers on a transitioning flag |
| core.rs:1553-1596 | HIGH | LOCK_ORDER | gapless advance reads queue/repeat under one lock, writes under another, then reads mpv (R1) | One write critical section spanning queue read, resolve next, set state, mpv probe |
| core.rs:778-800 | MEDIUM | STATE_INVARIANT | `extend_with_random_and_play` reads len, writes queue, plays; concurrent `advance_auto` can shift queue (R1) | One write critical section read-validate-extend-play |
| core.rs:261-268 | MEDIUM | STATE_INVARIANT | `restore_queue_blocking` uses `try_write` then warns on failure; queue restore silently skipped (R1) | Block IPC consumers behind Notify until restore completes, or use sync write |
| core.rs:1020-1024 | MEDIUM | LOCK_ORDER | `prebuffer_cancel.lock()` split read/write with atomic ops between (R1) | One critical section: read old, flip old, set new |
| core.rs:1099-1104 | MEDIUM | LOCK_ORDER | `prebuffer_and_load` takes `prebuffer_cancel.lock()` twice with async gap (R1) | Consolidate to one lock acquisition |
| core.rs:639-661 | MEDIUM | STATE_INVARIANT | `toggle_pause` reads queue_pos outside lock, then sets Paused/Playing before mpv ops (R1, R2) | Re-check queue_pos under write lock; defer state until mpv ack |
| core.rs:673-676 | MEDIUM | LOCK_ORDER | `pause_playback` reads state, early-returns, then takes write lock (R1) | Acquire write lock upfront and re-check |
| core.rs:1442-1445 | MEDIUM | ERROR_PATH | `.ok().flatten()` on audio property queries discards errors (R9) | Log at warn, then drop |
| core.rs:1887-1906 | MEDIUM | PASSWORD | `update_server_config` writes password file, clears inline, saves config, then stores plaintext in `state.config.password`; window where memory plaintext outlives disk redirect | Mirror the password indirection in memory: store only file pointer or zeroized secret |
| core.rs:1893 | MEDIUM | PASSWORD | `state.config.password` is plaintext `String` after disk redirect | Type-level masking via `Secret` newtype (see Prompt 3) |
| core.rs:1902-1906 | MEDIUM | STATE_INVARIANT | `*self.subsonic.write().await = Some(new_client)` then `config_gen.fetch_add()`; old-gen tasks completing in the window discard valid results (R4) | Bump gen first, then install client |
| core.rs:588 | MEDIUM | STATE_INVARIANT | `apply_star_to_cached` mutates after star RPC but before `refresh_starred` query; observers see mid-transition (R2) | Wrap mutation and refresh in one state write |
| core.rs:2061-2108 | MEDIUM | STATE_INVARIANT | `song_is_starred` scans every cache with no state lock | Move scan under state read lock, or atomic per-song flag |
| core.rs:251 | LOW | LOCK_ORDER | `last_loadfile.lock()` without a documented module-level lock order (R12) | Document order: state RwLock, subsonic RwLock, mpv Mutex, pipewire Mutex, prebuffer_*, last_loadfile, last_preload_attempt |
| core.rs:1528 | LOW | LOCK_ORDER | `last_preload_attempt.lock()` undocumented (R12) | Same doc comment |
| core.rs:1622-1624 | LOW | ERROR_PATH | `last_loadfile.lock().unwrap()` panics on poison | `expect("last_loadfile lock poisoned")` with reason |
| core.rs:1003,1028 | LOW | ERROR_PATH | `is_paused().unwrap_or(false)` silently returns false on query failure | Log warn on Err |
| core.rs:1013 | LOW | ERROR_PATH | `dispatch_play` returns `Ok(())` on `mpv.loadfile` error | Propagate Err so caller rolls back state |
| core.rs:1511-1551 | LOW | ERROR_PATH | `.ok()` on playlist_count / playlist_pos without log (R9) | Log at debug |
| core.rs:1877-1895 | LOW | INPUT_VALIDATION | `password_file` path not validated to be within config dir | Restrict to `Config::config_dir()` subtree or error |
| core.rs:292-296 | LOW | PERF | `spawn_queue_persistence` clones full queue under read lock | Lock to snapshot, then clone outside |
| core.rs:291-300 | LOW | PERF | 500ms sleep before first persistence drain | Zero-delay first send |

### 1.2 audio/mpv.rs

| file:line | sev | category | description | fix sketch |
|---|---|---|---|---|
| mpv.rs:254-296 | HIGH | LOCK_ORDER | `pending` Mutex acquired four times in `send_command`; concurrent callers can demux to wrong oneshot (R1) | Single critical section around register-write-await-cleanup |
| mpv.rs:254-296 | HIGH | RESOURCE_LEAK | `reader_loop` captures `Arc<pending>` clone; `tear_down_connection` drain races registrations (R4) | Generation counter on the pending map, or epoch-keyed (gen, id) |
| mpv.rs:216-252 | HIGH | LOCK_ORDER | `is_running` mutates `reader_handle` / `writer` / `process` with no sync vs `start` (R12) | Wrap connection slots in a single Mutex |
| mpv.rs:516-556 | HIGH | IPC_PROTOCOL | response demux assumes single-line atomic frames; multi-line JSON misaligns reader (R6) | Length-prefixed or delimiter-framed protocol |
| mpv.rs:254-296 | MEDIUM | STATE_INVARIANT | writer is `None`-checked, then unwrapped; can race with `tear_down_connection` (R2) | Move check inside the lock with the write |
| mpv.rs:162-172 | MEDIUM | STATE_INVARIANT | `start` busy-waits for socket then connects; mpv may still be initializing (R14) | Gate command dispatch on an explicit `ready` flag the reader sets |
| mpv.rs:135-185 | MEDIUM | STATE_INVARIANT | `reader_loop` spawned via `tokio::spawn` not awaited; early commands race the loop (R3) | Notify on first reader iteration, await before returning |
| mpv.rs:113-130 | MEDIUM | RESOURCE_LEAK | `reader_handle.abort()` is non-cooperative (R11) | CancellationToken; `select!` against it in reader |
| mpv.rs:299-304 | MEDIUM | IPC_PROTOCOL | `loadfile` path stuffed into JSON with no escaping; newline or quote breaks framing | JSON-encode the path argument |
| mpv.rs:474-479 | LOW | PERF | `is_idle()` round-trips mpv every poll, no TTL (R8) | Cache with 50ms TTL, invalidated on loadfile |
| mpv.rs:216-252 | LOW | RESOURCE_LEAK | `reader_handle.take()` called twice on cleanup paths | Idempotent guard; document why double-take is safe |
| mpv.rs:162-168 | LOW | PERF | 50 retries x 100ms busy-wait for socket | inotify or Notify from child |

### 1.3 audio/pipewire.rs

| file:line | sev | category | description | fix sketch |
|---|---|---|---|---|
| pipewire.rs:55-68 | HIGH | STATE_INVARIANT | `with_runner` calls `block_in_place` during construction (R3) | Defer first rate probe to first set/clear call |
| pipewire.rs:84-120 | MEDIUM | STATE_INVARIANT | `set_rate` / `clear_forced_rate` cache mutated after async run; external mutation diverges (R8) | Re-query `pw-metadata` post-mutation, or drop cache |
| pipewire.rs:146-176 | MEDIUM | RESOURCE_LEAK | `Drop` spawns thread; thread can outlive process (R11) | Pass a `Drop` cancel flag the runner checks |
| pipewire.rs:70-74 | MEDIUM | INPUT_VALIDATION | `parse_force_rate_from_output` returns 0 on parse failure with no log | Return `Result`; log the malformed line |
| pipewire.rs:146-176 | LOW | PERF | `Drop` poll loop 20ms x 150 | Use Condvar or `join_timeout` |
| audio/mod.rs | LOW | BUILD_HYGIENE | sub-modules `pub mod`, no aggregate | `pub(crate)` or single `AudioSystem` facade |

### 1.4 ipc/server.rs, protocol.rs, frame.rs, path.rs, mod.rs

| file:line | sev | category | description | fix sketch |
|---|---|---|---|---|
| protocol.rs:56-65 | HIGH | PASSWORD | `UpdateServerConfig` and `TestServerConnection` carry plaintext password on the wire and in `Serialize`/`Debug` | `Secret` newtype on the wire + redacted Debug |
| server.rs:26-50 | HIGH | STATE_INVARIANT | `broadcast::Lagged` flagged but no explicit Resync frame sent (R15) | Emit `DaemonEvent::Resync { snapshot }` |
| server.rs:160-286 | HIGH | IPC_PROTOCOL | No per-connection idle timeout; hung client holds writer task forever | Wrap read loop in `timeout(IDLE_BUDGET, ...)` |
| server.rs:170-178 | MEDIUM | RESOURCE_LEAK | writer task spawn captures handles with no CancellationToken (R11) | CancellationToken signalled on shutdown |
| server.rs:182-226 | MEDIUM | RESOURCE_LEAK | event task spawn capture (R11) | Same |
| server.rs:84-116 | MEDIUM | IPC_PROTOCOL | No protocol version handshake | `Hello { protocol_version: u32 }` frame |
| server.rs:228-279 | MEDIUM | IPC_PROTOCOL | No CancelRequest path for long-running operations (R6 adjacent) | Add `CancelRequest { id }` |
| server.rs:168 | MEDIUM | IPC_PROTOCOL | `EVENT_FORWARD_CAPACITY = 256` documented nowhere; resync path now requires deeper ring | Raise to 1024, document rationale |
| frame.rs:83-124 | MEDIUM | IPC_PROTOCOL | No frame-level version tag; forward-compat vs corruption indistinguishable | Per-frame version byte |
| frame.rs:145-151 | MEDIUM | INPUT_VALIDATION | `read_frame_body_with_cap` eager-allocates up to 16 MiB | Tighter per-message caps; CoverArt 4 MiB cap |

### 1.5 ipc/client.rs, socket_client.rs, spawn.rs

| file:line | sev | category | description | fix sketch |
|---|---|---|---|---|
| socket_client.rs:34-112 | HIGH | RESOURCE_LEAK | reader and writer tasks spawn with no cancellation; outlive socket close (R11) | CancellationToken; both tasks check it |
| socket_client.rs:49-57 | HIGH | IPC_PROTOCOL | writer task silently breaks on send failure; pending oneshots never notified | Drain `pending` on writer exit, send `Disconnected` to each |
| socket_client.rs:119 | HIGH | IPC_PROTOCOL | request id `AtomicU64::fetch_add` reused across reconnect (R4) | (gen, id) tuple keyed pending map |
| socket_client.rs:118-139 | MEDIUM | IPC_PROTOCOL | No overall timeout on the request-response cycle (R5) | `timeout(REQUEST_BUDGET, rx)` |
| socket_client.rs:59-110 | MEDIUM | STATE_INVARIANT | client broadcast lag silently dropped (R15) | Resubscribe and resync on Lagged |
| socket_client.rs:65-72 | MEDIUM | IPC_PROTOCOL | id collisions silently drop responses (R6) | Counter metric; epoch-prefix id |
| spawn.rs:70-86 | MEDIUM | RESOURCE_LEAK | `spawn_daemon` calls `forget(child)`; parent loses exit-code visibility | `waitpid(WNOHANG)` until socket ready; bail on early exit |
| client.rs:211-221 | LOW | PASSWORD | plaintext password fields in `DaemonRequest` (covered by Prompt 3) | `Secret` newtype |
| socket_client.rs:81-88 | LOW | ERROR_PATH | `UnknownResponse` loses payload error | Structured error frame |

### 1.6 config/mod.rs, config/paths.rs

| file:line | sev | category | description | fix sketch |
|---|---|---|---|---|
| config/mod.rs:319-345 | MEDIUM | PASSWORD | `resolve_password` clears password on file-read failure but leaves `password_file` set; observers cannot tell file read failed (R10) | Return Err on file-read failure; do not silently fall through |
| config/mod.rs:291 | MEDIUM | INPUT_VALIDATION | unknown TOML fields warned post-parse, not rejected | `serde(deny_unknown_fields)` |
| config/mod.rs:35 | MEDIUM | PASSWORD | redundant Debug + serialize_with redaction; two paths to keep aligned | Collapse into `Secret` newtype with one redact path |
| config/mod.rs:389-399 | LOW | INPUT_VALIDATION | `validate()` not called by `load_from_file` | Call validate after parse |
| config/mod.rs:455-564 | LOW | TEST_QUALITY | tests store plaintext passwords; no redaction round-trip test | Add test that Debug and serialize both redact |

### 1.7 subsonic/client.rs, auth.rs, models.rs

| file:line | sev | category | description | fix sketch |
|---|---|---|---|---|
| subsonic/client.rs:18-19 | HIGH | PASSWORD | plaintext password stored, no zeroize on Drop | `Secret` newtype + zeroize crate |
| subsonic/client.rs:14 | HIGH | PASSWORD | `SubsonicClient` derives Clone; duplicates secret | `Arc<Secret>` shared, manual Clone |
| subsonic/client.rs:111 | MEDIUM | IPC_PROTOCOL | no overall http timeout (R5) | `timeout(REQUEST_BUDGET, ...)` |
| subsonic/models.rs:16 | MEDIUM | INPUT_VALIDATION | `serde(default)` masks missing fields | Validate post-deserialize |
| subsonic/auth.rs:25 | MEDIUM | PASSWORD | `generate_token` concatenates plaintext into String, never zeroized | Hash via zeroizing buffer |
| subsonic/client.rs:43,330,354 | MEDIUM | PASSWORD | `generate_auth_params` callers expose plaintext on early return | Take `&Secret` |
| subsonic/client.rs:351-365 | MEDIUM | IPC_PROTOCOL | `get_stream_url` sync, no caller timeout pattern | Document or async |
| subsonic/client.rs:132-135 | MEDIUM | ERROR_PATH | `request<T>` returns Parse error without endpoint context | Wrap error with endpoint label |
| subsonic/models.rs:5 | LOW | PASSWORD | `SubsonicResponse` derives Debug, can expose secret via debug! | Filter logging or impl manually |
| subsonic/client.rs:58-70 | LOW | ERROR_PATH | `request_action` discards body | Preserve raw text in error context |

### 1.8 app/cava.rs, event_source.rs, client_state.rs, state.rs

| file:line | sev | category | description | fix sketch |
|---|---|---|---|---|
| app/state.rs:190 | HIGH | PASSWORD | `ServerState.password: String` plaintext field | `Secret` newtype |
| app/state.rs:350 | HIGH | PASSWORD | password copied from config to client state without zeroize | Drop-zeroize via Secret |
| app/cava.rs:120-143 | HIGH | STATE_INVARIANT | EOF path nulls `cava_pty_master` before confirming child exit (R2) | Confirm exit first; gate read with `cava_alive` flag |
| app/cava.rs:8-106 | HIGH | RESOURCE_LEAK | double-start race exposes PTY master FD (R3) | RAII `RawFdGuard`; serialize start via Mutex |
| app/cava.rs:44-49 | MEDIUM | RESOURCE_LEAK | `dup()` + `from_raw_fd` panic exposes FD | RAII guard |
| app/cava.rs:127-142 | MEDIUM | LOCK_ORDER | `cava_pty_master` read then written with parser activity between (R1) | Wrap pty_master + parser in one Mutex |
| app/state.rs:185-207 | MEDIUM | PASSWORD | `ServerState` derives Debug/Serialize; redaction is hand-rolled | Use `Secret` |
| app/client_state.rs:10-24 | MEDIUM | PASSWORD | `ClientState` holds `ServerState` with password | Same |
| app/cava.rs:157-185 | LOW | ERROR_PATH | `drain_into_parser` does not distinguish transient from hard IO error | Add transient variant; log hard error context |
| app/state.rs:346-360 | LOW | PASSWORD | no zeroize of config password after copy | Drop-zeroize via Secret |

### 1.9 app/mod.rs

| file:line | sev | category | description | fix sketch |
|---|---|---|---|---|
| app/mod.rs:597-644 | HIGH | LOCK_ORDER | NowPlayingChanged: cover_art Mutex acquired before and after async fetch with daemon_state lock between (R1) | One critical section: acquire cover_art lock once, gen-counter the fetch |
| app/mod.rs:770-809 | HIGH | LOCK_ORDER | ConfigChanged cover_art same split critical section pattern (R1) | Same fix |
| app/mod.rs:599-615 | MEDIUM | STATE_INVARIANT | back-to-back NowPlayingChanged races the in-flight cover fetch (R4) | Generation counter on cover fetch tasks |
| app/mod.rs:562-572 | MEDIUM | STATE_INVARIANT | broadcast lag handled by implicit resubscribe, no Resync event surfaced to UI (R15) | Push Resync after resubscribe |
| app/mod.rs:408-431 | MEDIUM | RESOURCE_LEAK | `bootstrap_and_pump` and `spawn_mpris_pump` spawn with no CancellationToken (R11) | Token signalled on quit |
| app/mod.rs:651-689 | MEDIUM | PERF | `SongStarChanged` rebuilds `starred_ids` and rescans every cache under state write lock | Compute outside lock, swap in |
| app/mod.rs:220 | LOW | RESOURCE_LEAK | `_poll_task` JoinHandle dropped; cancellation semantics implicit | Hold + cancel on shutdown |
| app/mod.rs:125-164 | LOW | COSMETIC | duplicated `CoverArtState` init in two constructors | Extract helper |
| app/mod.rs:598-604 | MEDIUM | STATE_INVARIANT | `now_playing` assigned before cover fetch starts | Stage as pending under cover_art lock |

### 1.10 daemon/library.rs, persistence.rs, state.rs, mod.rs

| file:line | sev | category | description | fix sketch |
|---|---|---|---|---|
| persistence.rs:47-61 | HIGH | RESOURCE_LEAK | queue tmp file written world-readable; auth-bearing song ids exposed in `/tmp` | `OpenOptions::mode(0o600)` before write, then rename |
| daemon/state.rs:10-17 | HIGH | PASSWORD | `DaemonState` derives Serialize; password redaction is ad-hoc per-call (R10) | `Secret` newtype removes the ad-hoc pattern |
| library.rs:30-62 | MEDIUM | STATE_INVARIANT | LruCache `get` / `insert` order vs map desync paths | Single source of truth for order; invariant test |
| persistence.rs:26-44 | LOW | ERROR_PATH | corrupt snapshot renamed to `.bad` with no auto-restore prompt | Surface via daemon log; expose via IPC for UI prompt |

### 1.11 mpris, bins, ui/cover_art

| file:line | sev | category | description | fix sketch |
|---|---|---|---|---|
| mpris/server.rs:27 | HIGH | PASSWORD | `build_cover_art_url` embeds token+salt in URL; URL can land in debug logs | Redact URL before any tracing call |
| mpris/server.rs:131-135 | HIGH | RESOURCE_LEAK | `fire()` spawns task from sync fn, no cancel (R3, R11) | Pre-built client clone with cancellable token |
| bin/ferrosonicd.rs:67-73 | MEDIUM | RESOURCE_LEAK | panic hook does not cancel spawned tasks | Token signalled in hook |
| bin/ferrosonicd.rs:115-124 | MEDIUM | RESOURCE_LEAK | `_poll`, `_mpv_events` discarded JoinHandles (R11) | Hold tokens; signal on shutdown |
| bin/ferrosonic.rs:72-84 | MEDIUM | ERROR_PATH | panic hook restores terminal then calls prev hook; nested-panic risk | Call prev first; swallow terminal errors |
| ui/cover_art.rs:165-202 | MEDIUM | STATE_INVARIANT | async fetch can land bytes for stale id (R2/R4) | Generation counter and pending flag |
| ui/cover_art.rs:245-250 | MEDIUM | STATE_INVARIANT | poisoned-Mutex recovery in render path leaves chafa cache stale | Re-check current_id after blit |
| bin/ferrosonicd.rs:138-144 | LOW | RESOURCE_LEAK | signal handlers leaked on early exit | Explicit drop scope |

### 1.12 tests, build

| file:line | sev | category | description | fix sketch |
|---|---|---|---|---|
| tests/queue_proptest.rs:49-52 | HIGH | TEST_QUALITY | proptest split read/write on state TOCTOU; tests its own race-prone harness | Hoist into one write critical section |
| tests/stress_tests.rs:76-94 | HIGH | TEST_QUALITY | no test exercises broadcast lag and resync recovery | Add Lagged-injection test |
| Cargo.toml:26 | MEDIUM | BUILD_HYGIENE | ratatui 0.28 vs 0.29 duplicate (tui-tree-widget transitive) | Bump tui-tree-widget or vendor; deny.toml `multiple-versions = "deny"` |
| tests/ (missing) | MEDIUM | TEST_QUALITY | no lock-ordering stress test | New `tests/lock_order.rs` |
| tests/ (missing) | MEDIUM | TEST_QUALITY | no password redaction round-trip test | New `tests/password_redaction.rs` |
| tests/ (missing) | MEDIUM | TEST_QUALITY | no atomic config save crash test | New `tests/config_atomic_save.rs` |
| tests/buffered_playback.rs:40 | MEDIUM | TEST_QUALITY | hardcoded `/tmp/` path check | `std::env::temp_dir()` |
| tests/ipc_socket_client_full.rs:49-65 | MEDIUM | TEST_QUALITY | broadcast subscriber test does not cover Lagged | Extend |
| Cargo.toml:117-131 | LOW | BUILD_HYGIENE | `unwrap_used` / `expect_used` / `panic` at warn, not deny | Promote to deny |
| fuzz/ (missing) | LOW | TEST_QUALITY | no cargo-fuzz targets | Establish via Prompt 6 |
| tests/ (missing) | LOW | TEST_QUALITY | no mpv-respawn-on-crash test | Kill mpv, assert respawn |
| .github/workflows | LOW | BUILD_HYGIENE | CI runs on workflow_dispatch only; cargo-audit / cargo-deny absent | Add jobs (Prompt 9) |

---

## 2. Structural patterns (3+ instances)

These are the targets for prompts 2 through 6. Kill the pattern, not the
individual instances.

### 2.1 PASSWORD (17 instances)

Plaintext `String` for passwords lives in `Config`, `DaemonState`,
`ServerState`, `ClientState`, `SubsonicClient`, `DaemonRequest` wire
structs, mpris URL builder, auth token builder, and assorted in-flight
clones. Current defense is ad-hoc: a custom `serialize_with`, a custom
`Debug`, manual scrubbing inside `DaemonCore::snapshot`. Every new
callsite is a regression risk. **Fix structurally with a `Secret`
newtype** that owns `Box<[u8]>`, masks Debug and Serialize by default,
zeroizes on Drop, and exposes `reveal()` only to legitimate callers.

### 2.2 LOCK_ORDER and STATE_INVARIANT (combined; 22 instances)

Split read/write critical sections (R1), state-before-IO (R2),
Arc-clone-outlives-slot (R4), and undocumented lock acquisition order
(R12) dominate the audit. Sites: `core.rs` gapless advance,
`extend_with_random_and_play`, `restore_queue_blocking`,
`prebuffer_*`, `toggle_pause`, `pause_playback`, `update_server_config`,
`apply_star_to_cached`, `song_is_starred`, `mpv.rs` `send_command` and
`is_running`, `pipewire.rs` `set_rate` cache, `cava.rs` PTY teardown,
`app/mod.rs` cover_art NowPlayingChanged / ConfigChanged,
`ui/cover_art.rs` fetch race. **Fix structurally with a module-level
lock-order doc comment plus a per-site refactor pass.**

### 2.3 IPC_PROTOCOL (12 instances)

Missing: protocol version negotiation, per-connection idle timeout,
CancelRequest, frame-level version tag, per-message size caps, bulk
event resync, request-id epoch tagging, writer-task failure
notification, overall request timeout, mpv JSON path escaping.
**Fix structurally with a single IPC hardening pass (Prompt 4).**

### 2.4 RESOURCE_LEAK (14 instances)

`tokio::spawn` capturing `Arc<Self>` with no CancellationToken in
`core.rs` (3 sites), `ipc/server.rs` (2 sites), `socket_client.rs`
(2 sites), `bin/ferrosonicd.rs` (3 sites), `app/mod.rs` (2 sites),
`mpris/server.rs` (1 site), `pipewire.rs` Drop (1 site). Plus FD-leak
paths in `cava.rs` (2 sites) and `persistence.rs` tmp permissions.
**Fix structurally by adopting a single `CancellationToken` field per
long-lived component and an RAII `RawFdGuard` helper. Falls inside
Prompt 7's HIGH catch-all.**

### 2.5 TEST_QUALITY (11 instances)

Property tests cover queue / config / frame but miss broadcast-lag
recovery, lock-order stress, password redaction round-trip, atomic
config save crash, mpv respawn. No fuzz targets. **Fix via Prompt 5
(property + integration) and Prompt 6 (fuzz).**

### 2.6 INPUT_VALIDATION (5 instances)

Unknown TOML fields warn rather than reject. `validate()` is not called
on load. `password_file` path is not bounded to the config directory.
`SubsonicResponse` accepts `serde(default)` everywhere. PipeWire
`parse_force_rate_from_output` silently returns 0 on parse failure.
**Fix mostly inside Prompt 4 (frame caps) plus Prompt 8 (config
validation).**

### 2.7 ERROR_PATH (8 instances)

`.ok()` and `.ok().flatten()` drop important Results in `core.rs`,
`subsonic/client.rs`, `app/cava.rs`, `socket_client.rs`,
`ferrosonic.rs` panic hook, `dispatch_play`. **Cleanup pass during
Prompt 7 / 8.**

### 2.8 BUILD_HYGIENE (4 instances)

Duplicate ratatui versions, clippy at warn not deny, no cargo-audit /
cargo-deny in CI, CI is `workflow_dispatch`-only. **All folded into
Prompt 9.**

### 2.9 PERF (5 instances)

mpv `is_idle` no TTL, `spawn_queue_persistence` lock-then-clone,
`SongStarChanged` cache rescan under write lock, busy-waits in
`mpv.rs:162-168` and `pipewire.rs Drop`. **Triaged in Prompt 8.**

---

## 3. Single-instance bugs

Bugs that do not fit a structural pattern; addressed in Prompt 7's HIGH
catch-all or Prompt 8's MEDIUM/LOW triage.

- `core.rs:984` HIGH state-before-IO on Playing (R2). Caught by lock-
  order pass and Secret refactor; if not, fix in Prompt 7.
- `core.rs:322-327` HIGH idle-probe on Lagged. Subsumed by Prompt 4
  (Resync event variant).
- `mpv.rs:516-556` HIGH single-line response demux. Subsumed by mpv
  reader refactor inside Prompt 7 (Resource lifecycle).
- `app/cava.rs:120-143` HIGH cava EOF before child confirmed. Prompt 7
  (HIGH catch-all).
- `mpris/server.rs:27` HIGH cover URL leak. Prompt 3 (Secret stops the
  URL from carrying plaintext; URL builder also needs a redact-before-
  log pass).
- `persistence.rs:47-61` HIGH tmp file world-readable. Prompt 7 (HIGH
  catch-all; trivial fix, separate commit).
- `core.rs:111-119` HIGH CancelSlotCleaner spawn in Drop. Prompt 7.
- `daemon/state.rs:10-17` HIGH DaemonState Serialize redaction
  fragility. Subsumed by Prompt 3 (Secret).
- `pipewire.rs:55-68` HIGH block_in_place in construction. Prompt 7.
- `mpv.rs:299-304` MEDIUM mpv path JSON injection. Prompt 7.
- `core.rs:1442-1445` MEDIUM ERROR_PATH .ok().flatten on audio props.
  Prompt 8.
- Remaining LOW items per file. Prompt 8.

---

## 4. Proposed execution order

One structural pattern per prompt. Each prompt opens a fresh session,
re-reads the rules file, fixes the pattern wholesale, and exits with
green build, green clippy, green tests, and no new audit findings.

| # | Pattern | Why this slot |
|---|---|---|
| 2 | LOCK_ORDER (DONE 2026-05-14) | Sealed per `docs/LOCK-ORDER.md` and `tests/lock_order.rs`. Four `core.rs` sites consolidated (prebuffer cancel split fixed; module + last_loadfile + last_preload_attempt order documented). Concurrent IPC fuzz passes 100 consecutive runs. |
| 2.5 | STATE_INVARIANT (R2 + R4) | Split out of the original LOCK_ORDER+STATE_INVARIANT combined slot. R2 (state-before-IO) and R4 (stale-Arc-after-slot-swap) are a real category that needs the same treatment while the codebase is fresh in session memory. Deferring to prompt 7's catch-all would re-bloat that slot with 8+ state-invariant fixes plus all the unrelated HIGHs. |
| 3 | PASSWORD via `Secret` newtype | Plaintext password sites touch every layer. Adding the type now means every subsequent refactor (IPC handshake, fuzz targets, test fixtures) automatically inherits redaction. Defers no other prompt; unblocks Prompt 4's clean wire format. |
| 4 | IPC_PROTOCOL hardening | With Secret in place, the wire format can rev cleanly: Hello handshake, idle timeout, CancelRequest, version tags, per-message caps, `LibraryDomainChanged` version events, Resync. This kills the IPC_PROTOCOL category in one pass and removes blockers for Prompt 5's integration tests. |
| 5 | Property + integration tests | Builds on stable lock invariants (Prompt 2), Secret type (Prompt 3), and the new protocol (Prompt 4). queue / lock-order / broadcast property tests, plus integration tests for shutdown, crash recovery, concurrent IPC. Highest payoff for catching future regressions. |
| 6 | Fuzz infrastructure | `cargo-fuzz` targets for frame parser, mpv response parser, subsonic JSON, URL parser. Crash corpus committed and replayed in `cargo test --release` for regression. Catches the long tail of malformed-input bugs property tests miss. |
| 7 | Remaining HIGH catch-all (RESOURCE_LEAK, single-instance HIGHs) | All non-structural HIGHs: CancelSlotCleaner Drop, persistence tmp 0o600, cava EOF race, mpv response demux refactor, pipewire `block_in_place`, mpv JSON injection, RAII FD guard, CancellationToken adoption per long-lived component. Each item: write a failing regression test, fix, confirm green. |
| 8 | MEDIUM and LOW triage | Per-item FIX_NOW vs KNOWN_ISSUE vs DROP decision. Captures the remaining ERROR_PATH, INPUT_VALIDATION, PERF, COSMETIC items. Updates `docs/STABILIZATION.md` and creates `docs/KNOWN-ISSUES.md`. |
| 9 | BUILD_HYGIENE / CI gates | CI runs on push and PR. cargo fmt, clippy `-D warnings`, test `--release`, cargo-audit, cargo-deny with ratatui de-duplication. Pre-push hook. Clippy lints promoted to deny. Ensures no regression of prompts 2 through 8. |
| 10 | Release cut | Final `/rust-audit` pass; require zero CRITICAL and zero HIGH. Three test runs with different proptest seeds. CHANGELOG. README. Version bump. Tag. Post-mortem at `docs/POST-STABILIZATION.md`. |

Two-sentence defense per category:

- LOCK_ORDER first because every other category sits on top of these
  invariants. A `Secret` refactor on top of a racey lock pattern would
  relocate the race rather than fix it.
- STATE_INVARIANT (slot 2.5) immediately after LOCK_ORDER because the
  R2 / R4 hazards live in the same critical sections we just touched
  and prompt 7's catch-all would otherwise carry 8+ state-invariant
  fixes plus the unrelated HIGHs.
- PASSWORD second because the newtype is a fresh-eyes module and the
  largest single category by site count; deferring it forces every
  subsequent prompt to repeat the manual redaction dance.
- IPC_PROTOCOL third because the wire format is the public-facing
  surface; locking it in early lets the test suite (Prompt 5) and the
  fuzzers (Prompt 6) target a stable artefact rather than a moving one.
- Property + integration tests fourth because they require all three
  prior structural fixes to be in place to assert meaningful invariants.
- Fuzz fifth because it exercises the new frame parser and message
  caps the moment they exist, catching the long-tail malformed-input
  bugs hand-written tests miss.
- HIGH catch-all sixth because once the structural categories are
  closed, what remains are individual bugs each needing a regression
  test and a one-shot fix.
- MEDIUM and LOW seventh because triage cost dominates fix cost; doing
  it after the HIGHs ensures we are not deciding KNOWN_ISSUE on items
  the HIGH pass would have folded into a wider fix.
- BUILD_HYGIENE eighth because CI gates are pointless until the
  artefact under test is stable. Adding them now forces every prior
  fix to remain green.
- Release tenth because Prompt 9 is the last code-touching step; the
  cut is a checklist, not new work.

---

## 5. Prompt 2.5 in-scope checklist

R2 (state-before-IO) and R4 (stale-Arc-after-slot-swap) items lifted
out of section 1 for the prompt 2.5 fresh session. For each item:
read the finding row in section 1, write a regression test in
`tests/state_invariant.rs` that fails on current HEAD, fix the bug,
confirm the test passes. Run `/rust-audit` after each commit and
require zero new STATE_INVARIANT findings.

Status (DONE 2026-05-14):

- [x] `core.rs:984` R2: `commit_play_state_in_lock` stamps
  `last_loadfile` under the state write lock; the existing 1.5s
  idle-advance gate in `update_playback_info` now covers the
  commit-to-loadfile transitioning window. Commit `8129336`.
- [x] `core.rs:1902-1906` R4: `update_server_config` now takes
  `subsonic.write()`, bumps `config_gen` first, then installs the
  client, all under one critical section. Refreshes acquire
  `subsonic.read()` and serialize behind. Commit `0d6143f`.
- [x] `core.rs:588` R2: `toggle_star_song` consolidates the
  currently-starred read + optimistic cache mutation into one
  state write; pre-fetches the fresh starred list outside the
  lock; then commits cache + starred_songs replacement + index
  rebuild atomically. RPC failure path rolls the optimistic
  mutation back under another write. Commit `6f82b61`.
- [DROP] `core.rs:2061-2108` `song_is_starred` scan: function
  signature `fn song_is_starred(daemon: &DaemonState, song_id)`
  already requires the caller to hold a state lock (the `&` borrow
  is only obtainable while holding the read or write guard). The
  sole caller in `toggle_star_song` takes the lock before calling
  in. The fallback chain over multiple caches is an intentional
  defense for caches that may carry the `starred` marker without
  having had `starred_ids` rebuilt. No code change required.
- [x] `core.rs:261-268` R1: snapshot load is hoisted into
  `new_shared_daemon_state` so it runs on a fresh `DaemonState`
  before the `Arc<RwLock>` wraps it. `restore_queue_blocking` is
  removed entirely. The try_write silent-skip path no longer
  exists. Commit `22f338a`.
- [x] `core.rs:639-661` R1 + R2: `toggle_pause` now re-checks the
  pre-commit state under the final write lock; only commits
  Paused/Playing if state was still Playing or Paused. Concurrent
  Stop wins. Commit `ef10a09`.
- [x] `core.rs:673-676` R1: `pause_playback` takes the write lock
  upfront for the Playing check, releases for the mpv.pause()
  call, then re-acquires the write lock and re-checks before
  committing Paused. Commit `c9da966`.
- [x] `core.rs:778-800` R1: `extend_with_random_and_play` already
  reads queue.len(), extends, and commits play state under a
  single state write (prompt 2 LOCK_ORDER pass ratified). A
  positive regression test pins the contract. Commit `0f801a4`.

Out of scope (NOT touched in prompt 2.5):

- `mpv.rs` / `pipewire.rs` / `cava.rs` / `app/mod.rs` /
  `ui/cover_art.rs` STATE_INVARIANT items: these have their own
  locks and are prompt 7 territory.
- PASSWORD, IPC_PROTOCOL, RESOURCE_LEAK categories.
- Any LOW item not in the STATE_INVARIANT category.

Done criteria status:

- Every in-scope finding fixed or marked DROP with reasoning: YES.
- `tests/state_invariant.rs` exists with one regression test per
  fix (7 tests + DROP item) and passes: YES.
- `cargo check` + clippy + test green: YES.
- `/rust-audit` reports no new STATE_INVARIANT findings: per-commit
  audits returned 0 blocking, 0 investigate, 0 note for each.

---

## 6. Prompt 3 PASSWORD follow-up checklist

Items deferred from the prompt 3 in-scope set. Each tagged DEFER (to a
later prompt) or DROP (with reasoning). Status as of 2026-05-14 after
the 4 highest-traffic sites adopted `Secret`.

Done in prompt 3:

- [x] `Secret` newtype with drop-zeroize and masked Debug+Serialize.
  `src/secret.rs`, registered in `lib.rs`. Commit `cea7abf`.
- [x] `Config.password: Secret` with default-masked Serialize and
  Debug, `serialize_revealed_opt` for on-disk path. `resolve_password`
  zeroizes the intermediate `String` after trimming.
- [x] `SubsonicClient.password: Arc<Secret>` so Clone is cheap and
  the secret is shared, not duplicated. `generate_auth_params(&Secret)`
  hashes through a zeroizing buffer.
- [x] `ServerState.password: Secret` (drops manual Debug masking
  in favour of Secret's masked impl).
- [x] `DaemonRequest::{UpdateServerConfig, TestServerConnection}`
  wire types: `Secret` field with explicit
  `#[serde(serialize_with = "serialize_revealed",
  deserialize_with = "deserialize_secret")]` so the IPC frame on the
  wire carries plaintext but in-memory `Debug` masks. Commit `551e3d1`.
- [x] `tests/password_redaction.rs` regression suite: Debug + Serialize
  masking, wire reveal helpers, IPC round-trip, Config/ServerState
  debug masking, clone correctness. 100/100 green.

DEFER (move to a later prompt; not behavior-improving in isolation):

- [DEFER prompt 7] `DaemonState.password` field via Secret newtype
  + remove ad-hoc Serialize redaction (`src/daemon/state.rs:10-17`).
  Subsumed: `DaemonState` already routes via `Config.password`, so
  the type-level masking we just landed in `Config` covers DaemonState
  too. The audit row (1.10) was filed before the Config refactor.
  Verify in prompt 7's HIGH catch-all that no separate path leaks.
- [DEFER prompt 7] `mpris/server.rs:27` cover-art URL redaction.
  `build_cover_art_url` builds a URL containing the auth token and
  salt that the password produced. The token is one-way-hashed so
  the URL itself is not the password, but it still authenticates.
  Audit a redact-before-log pass over the URL builder; this is
  category PASSWORD-adjacent and belongs in prompt 7 alongside the
  other tracing / log audits.
- [DEFER prompt 7] `core.rs:1887-1906` PASSWORD window between
  `password_file` write and `state.config.password` re-set. The
  current code (in `update_server_config`) is still:
  1. write `password_file` atomically, 2. clear `state.config.password`,
  3. save config to disk, 4. restore `state.config.password` to the
  full Secret. This window is now under a single state write lock
  so concurrent readers see one of the two consistent states. The
  ideal fix (don't store plaintext in memory at all when
  `password_file` is set, instead lazy-read on demand) is a behavior
  change deferred to prompt 7's HIGH catch-all. Marked acceptable
  given Secret already drop-zeroizes.

DROP (not worth landing as part of the stabilization sprint):

- [DROP] `tests/config_password_resolution.rs` and other tests that
  store plaintext passwords. The prompt 3 brief listed this as a
  candidate for `Secret::from_string("...".into())`. The 4 in-scope
  sites already forced these test fixtures to migrate (now use
  `&"p".into()` or `c.password_str()` for assertions). No further
  cleanup needed. Status: covered by the in-scope work, no DROP cost.
- [DROP] `subsonic/models.rs:5` `SubsonicResponse` derives Debug
  audit row 1.7. With `Secret` adopted at the four boundaries, no
  password ever flows through `SubsonicResponse`. The audit row was
  defense-in-depth against a future leak path that does not exist
  today. Reasoning: would cost a manual Debug impl plus tests for a
  type that does not carry secrets. No structural improvement.
- [DROP] `app/state.rs:346-360` "no zeroize of config password after
  copy" audit row 1.8. The copy is now Secret-to-Secret. When
  `client.server_state.password` is later overwritten, the previous
  Secret drops and zeroizes. The original "copy" was a `String`
  duplication that lingered until the ClientState dropped; with
  Secret, the lifecycle is bounded and zeroized. No follow-up needed.

Done criteria status:

- `Secret` newtype lives at `src/secret.rs`, exported from `lib.rs`:
  YES.
- All 4 highest-traffic sites adopted: YES (Config.password,
  ServerState.password, SubsonicClient.password Arc-shared, IPC
  request types via serialize_revealed).
- Regression tests prove redaction: YES
  (`tests/password_redaction.rs`, 14 tests, 100/100 consecutive).
- `cargo check` + clippy + test all green: YES.
- `/rust-audit` reports no new PASSWORD findings on the affected
  files: per-commit audits returned 0 blocking, 0 investigate, 0
  note.
