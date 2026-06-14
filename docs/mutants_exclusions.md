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

## ui

- `queue.rs` (`< pos` -> `<= pos`) in queue render `is_played`. differs only at `i == pos`, which is the current track; `is_current` is checked FIRST in the style chain, so `is_played` is never consulted at `i == pos`.
