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
- `ipc/server.rs:126` (`LOCK_EX | LOCK_NB` -> `LOCK_EX ^ LOCK_NB`) in `acquire_socket_lock`. the two flag constants are disjoint bits (`LOCK_EX = 2`, `LOCK_NB = 4`), so `|` and `^` both yield `6`. identical for these operands.
- `playback_tick.rs:291` (`dur > 0.0` -> `>= 0.0`) in `tick_backfill_duration`. the filter only differs at `dur == 0.0`, where the mutant keeps `0.0` and the body writes `now_playing.duration = 0.0` under the `<= 0.0` guard, which equals the `0.0` already present. duration is never negative, so the write is a no-op for every reachable state.

## ui

- `queue.rs` (`< pos` -> `<= pos`) in queue render `is_played`. differs only at `i == pos`, which is the current track; `is_current` is checked FIRST in the style chain, so `is_played` is never consulted at `i == pos`.
