# DaemonCore Lock Order

Authoritative declaration of the global lock acquisition order for
every `Mutex` and `RwLock` field on `DaemonCore` (R12). A site that
already holds a later lock may not back-acquire an earlier one
without first releasing the later one. Sites that hold two or more
locks from the list simultaneously must document the reason at the
call site.

## The order

| # | Lock | Type | Field |
|---|------|------|-------|
| 1 | `state` | `RwLock<DaemonState>` | shared state machine, queue, library, now-playing |
| 2 | `subsonic` | `RwLock<Option<SubsonicClient>>` | active Subsonic client (replaced on `update_server_config`) |
| 3 | `mpv` | `Mutex<MpvController>` | mpv IPC controller |
| 4 | `pipewire` | `Mutex<PipeWireController>` | PipeWire sample-rate switcher |
| 5 | `prebuffer_cancel` | `Mutex<Option<Arc<AtomicBool>>>` | cancel flag for the in-flight prebuffer task |
| 6 | `prebuffer_loading` | `Mutex<Option<Arc<AtomicBool>>>` | gates idle-advance during prebuffer-to-loadfile gap |
| 7 | `prebuffer_files` | `Mutex<Vec<Arc<NamedTempFile>>>` | keep-alive ring for prebuffer temp files |
| 8 | `last_loadfile` | `std::sync::Mutex<Option<Instant>>` | timestamp of the most recent `loadfile` |
| 9 | `last_preload_attempt` | `std::sync::Mutex<Option<Instant>>` | retry-throttle for failed gapless preloads |
| 10 | `cover_art_cache` | `RwLock<LruCache<Vec<u8>>>` | bounded LRU of cover-art bytes |
| 11 | `scrobble_state` | `Mutex<ScrobbleState>` | per-play scrobble tracking; never held with any other lock |

## Standard idioms

- `let client = self.subsonic.read().await.clone();` releases the
  `subsonic` lock immediately so subsequent `state.write()` can be
  taken without overlap.
- `stamp_loadfile()` may be called while holding `mpv` (last_loadfile
  is lock 8, mpv is lock 3).
- `commit_play_state_in_lock` runs under an already-held `state`
  write lock; the caller must have taken `state` first (and is
  expected to have cloned `subsonic` already so the `SubsonicClient`
  passed in is by reference).
- `dispatch_play(PlayMode::Buffered)` consolidates the prebuffer-cancel
  swap into one critical section; the new cancel slot and loading
  flag are installed before any mpv call.
- `scrobble_tick` reads `state` and releases it, then takes
  `scrobble_state` alone to decide, then releases it before spawning
  the report task (which reads `subsonic`). It never holds two locks
  at once, so lock 11 cannot invert with any other.

## What this fixes

R1 (split read/write critical sections) and R12 (lock-order
inversion). The historical worst case was prebuffer\_cancel taken
twice with an async gap between (`dispatch_play` then
`prebuffer_and_load`), and `RemoveFromQueue` reading `state` then
acquiring `mpv` then re-acquiring `state` -- both have been folded
into single critical sections per layer.

## Past bugs that prove the order matters

Three shipped bugs motivate the order above; reverting any of them
re-opens the same race window.

- **Gapless-advance TOCTOU** (`core.rs:1553-1596`, audit 2026-05-13).
  `update_playback_info` read `queue` + `repeat_mode` under one
  `state.read()`, computed the next track, then re-acquired
  `state.write()` to commit. A concurrent `RemoveFromQueue` between
  the read and the write could shift the queue so the wrong song
  was committed as the next track. Fix: resolve and commit under a
  single `state.write()`, then re-check `playlist-pos` under the
  same `mpv` lock as `playlist_remove(0)` so a stale advance never
  drops entry 0 after mpv has moved on.
- **`RemoveFromQueue` inversion** (`ipc/client.rs:86`+, audit
  2026-05-13). The handler read `state`, then acquired `mpv` to
  call `stop()`, then re-acquired `state` to set `now_playing` to
  `Stopped`. Two clients running `RemoveFromQueue` and
  `PlayQueueIndex` concurrently could interleave such that mpv was
  stopped while state still said `Playing`, or state went `Stopped`
  while mpv kept playing the removed track. Fix: collect every
  state mutation (queue mutation, sentinel `Stopped`, now_playing
  scrub) into one `state.write()` critical section, then take
  `mpv` only after `state` drops, matching the declared order
  1 -> 3 with no back-acquire.
- **Prebuffer cancel split** (`core.rs:1019-1041` + `1089-1104`,
  audit 2026-05-13). `dispatch_play(Buffered)` took
  `prebuffer_cancel` to cancel the prior task, dropped it, then
  `prebuffer_and_load` took it again to install the new cancel
  Arc. Two concurrent `Play` calls could interleave between the
  two acquisitions: caller A cancels, caller B cancels (finds
  nothing), caller B installs its Arc, caller A installs its Arc,
  and B's prebuffer task runs uncancelled. Fix: one nested
  critical section in `dispatch_play(Buffered)` covering both
  `prebuffer_cancel` and `prebuffer_loading` swaps (lock order
  5 -> 6) before any `mpv` call; `prebuffer_and_load` no longer
  touches `prebuffer_cancel` at all.

## Internal sub-locks (not part of the order)

`MpvController` owns an internal `pending` map (request-id to oneshot
sender). It is acquired only while `core.mpv` is held; its order
relative to other DaemonCore fields is therefore subsumed by `mpv`'s
position in the table above.

`AppState.cover_art` (TUI side) is `std::sync::Mutex` on the TUI's
local state, not on `DaemonCore`. Its lock order with respect to
`daemon_state` / `client_state` is documented in `src/app/mod.rs`
(`apply_event`).

`PipeWireController` runs work on a dedicated thread via
`crossbeam-channel`; the `pipewire` Mutex on `DaemonCore` is the
serialization point and never overlaps with the runner thread's own
state.
