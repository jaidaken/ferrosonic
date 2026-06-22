---
description: Provably-equivalent cargo-mutants survivors that no test can kill, with the reason each is equivalent. Read before treating a surviving mutant as a coverage gap.
tags: [testing, mutation, exclusions]
date: 2026-06-14
---

# MUTANTS EXCLUSIONS (provably equivalent)

survivors here change NO observable behaviour: same return value + same side
effects for every reachable input. they are NOT weak spots; a test cannot
distinguish them. listed by `file:line`, with the equivalence proof. all other
survivors are real gaps and get a test (see [TESTING](TESTING.md)).

## daemon

- `queue_ops.rs:69` (`from < cur` -> `<=`) and `:71` (`from > cur` -> `>=`) in `move_queue_item`. the `cur == from` case is handled by the FIRST match arm (`if cur == from`), so these comparisons are only reached when `from != cur`; `<` vs `<=` (and `>` vs `>=`) differ only at `from == cur`, which is unreachable here.
- `queue_ops.rs:146` (`cur < state.queue.len()` -> `<=`) in `shuffle_queue`. `queue_position` is always a valid index (`< len`) by the queue invariant; `cur == len` is unreachable, so `<` and `<=` agree on every reachable state.
- `persistence.rs:24` (NotFound match guard -> true/false, `==` -> `!=`) in `QueueSnapshot::load`. both guard arms `return None`; the guard only selects whether a warning is logged. result is identical (`None`) for every error kind.
- `run.rs:85` (`e.kind() != NotFound` -> `==`) in `shutdown`. socket removal: both branches leave the socket removed; the guard only selects whether a warning logs. no behavioural difference.
- `playback_ops.rs:223` (`start_at > 0.0` -> `>= 0.0`) in `play_queue_position_at`. at `start_at == 0.0` the mutant writes `now_playing.position = 0.0`, which equals the `0.0` already set by `commit_play_state_in_lock` earlier in the same call; for `start_at > 0.0` both commit. same value for every input. (the `== 0.0` and `< 0.0` mutants at this site ARE killed: they skip the commit for `start_at > 0`, see `tests/daemon_seek_resume.rs`.)
- `playback_ops.rs:260` (`count < 2` -> `==` / `>` / `<=`) in `preload_next_track`. the comparison only selects `warn!` vs `debug!` after the preload append; no state, queue, or mpv-command difference. log-only.
- `config/mod.rs:491:43` (`!parent.is_empty() && !parent.exists()` -> `||`) in `write_password_file_atomic`. the guard only decides whether to call `create_dir_all`. with `||`, an existing parent additionally gets an idempotent `create_dir_all` (a no-op success); a missing parent is created under both. no observable difference for any reachable path.
- `audio/pipewire.rs:195` (`rate > 0` -> `rate >= 0`) in `Drop`. `rate` is `u32`, so `>= 0` only adds the `rate == 0` case, where the mutant's `rate.to_string()` produces `"0"` and the original `else` branch also produces `"0"`. identical for every value.
- `audio/pipewire.rs:213` (`!handle.is_finished()` -> `handle.is_finished()`) in `Drop`. the branch body is a single `error!` log of the timeout; inverting it only changes whether that line logs. no state, thread, or pw-metadata difference.
- `ipc/server.rs:126` (`LOCK_EX | LOCK_NB` -> `LOCK_EX ^ LOCK_NB`) in `acquire_socket_lock`. the two flag constants are disjoint bits (`LOCK_EX = 2`, `LOCK_NB = 4`), so `|` and `^` both yield `6`. identical for these operands.
- `playback_tick.rs:291` (`dur > 0.0` -> `>= 0.0`) in `tick_backfill_duration`. the filter only differs at `dur == 0.0`, where the mutant keeps `0.0` and the body writes `now_playing.duration = 0.0` under the `<= 0.0` guard, which equals the `0.0` already present. duration is never negative, so the write is a no-op for every reachable state.
- `core.rs` `prebuffer_and_load` cancel-path `gate.disarm()` / `slot_cleaner.disarm()` (3 sites: loop-top, pre-loadfile, post-loadfile). removing any is equivalent. each Buffered task owns a PER-TASK loading `Arc` and cancel `Arc`. a superseding `dispatch_play` flips this task's cancel and replaces both slots in ONE critical section with both the `prebuffer_cancel` and `prebuffer_loading` locks held. so `cancel == true` implies this task's loading `Arc` is already detached from the `prebuffer_loading` slot and `prebuffer_cancel` holds the newer task's cancel. `PrebufferGate::drop`'s lock-free `flag.store(false)` hits the detached `Arc` (the sole reader, `playback_tick`, takes the loading lock and reads the CURRENT slot Arc, so it never sees the detached one). `CancelSlotCleaner`'s `Arc::ptr_eq` is false vs the newer cancel, so it skips. disarm only skips the redundant Drop work; it is an optimization, not a correctness gate. verified: removing the loop-top pair leaves `tests/buffered_playback.rs` + `core::guard_tests` green. the guard LOGIC mutants (disarm methods, Drop `armed` check, `Arc::ptr_eq`) ARE killed by `core::guard_tests`.

## ui

- `queue.rs` (`< pos` -> `<= pos`) in queue render `is_played`. differs only at `i == pos`, which is the current track; `is_current` is checked FIRST in the style chain, so `is_played` is never consulted at `i == pos`.
