# Mutation baseline (2026-05-15)

Phase E of TESTING-PLAN.md. Establishes a cargo-mutants kill-rate
baseline on the critical modules. Full-workspace mutation is the
CI nightly responsibility per
`~/.claude/rules/common/testing.md` MUTATION section (estimated
8 to 30 hours one-shot, infeasible for an interactive session).

## Invocation

```
cargo mutants \
  --file <path>... \
  --test-tool=nextest \
  --baseline=skip \
  --no-shuffle \
  --timeout 120 \
  --jobs 2 \
  --cargo-test-arg '-E' \
  --cargo-test-arg 'not binary(stress_tests)'
```

Justification for the `stress_tests` exclusion: that binary holds
`proptest_arbitrary_play_queue_sequences_dont_panic` (~44s baseline)
plus other randomized stress tests whose per-run output is
non-deterministic against a fixed mutated source. They give very
weak per-mutant signal at the cost of a 44s tax per mutant. They
remain in the standard CI test suite and stay relevant for fuzz
gating; they are skipped only when measuring mutation kill rate.

`--baseline=skip` is safe because every run was preceded by a clean
`cargo nextest run` against the unmutated tree (1233 pass / 0 fail).

`--no-shuffle` makes the per-file ordering deterministic.

`--jobs 2` fits two concurrent build dirs (~30 GB each) on the
shared `/tmp` (140 GB free at run start). Raising to 4 exhausted
the volume on the first attempt.

## Per-file kill rates

Kill rate = `caught / (caught + missed)`.
Unviable mutants (compiler-rejected) are excluded. Timeouts are
listed separately and treated as non-kills.

| File                       | Kill rate | Caught | Missed | Unviable | Timeout |
| -------------------------- | --------: | -----: | -----: | -------: | ------: |
| `src/app/input_server.rs`  |      100% |     40 |      0 |        0 |       0 |
| `src/io_util.rs`           |       60% |      3 |      2 |        0 |       0 |
| `src/secret.rs`            |     88.9% |     24 |      3 |        2 |       0 |
| `src/audio/pipewire.rs`    |     93.5% |     29 |      2 |        3 |       1 |
| `src/daemon/library.rs`    |      100% |     12 |      0 |        1 |       0 |
| `src/ipc/protocol.rs`      |         - |      - |      - |        - |       - |

`src/app/input_server.rs` row is the prior A1.c result (commit
`8c90f99`) preserved here for the consolidated picture.

`src/ipc/protocol.rs` row is dash-only: cargo-mutants generated
zero viable mutants from that file. Its public surface is enum
plumbing and `#[derive]`-driven serialization which has no
behavioural site for the standard FnValue / BinaryOp mutation
operators to attack.

## Below the M75 floor

`src/io_util.rs` (60%) is the only listed file under the 75% M75
floor. The two surviving mutants are:

1. `fsync_parent_dir` body replaced with `()` (no test detects
   missing durability flush; the function returns `()` and the
   fsync syscall has no observable side effect from user-space).
2. `&&` to `||` in `atomic_write_bytes` (the discriminator that
   catches absent vs partial temp-cleanup paths).

R21 in the rust ruleset names parent-dir fsync as the closing
half of the atomic-write pattern. Strengthening this kill rate
calls for a test that asserts durability under a simulated
ungraceful shutdown (loopback dm-flakey or an injected fault).
That is heavier than the present test inventory and is queued
for a follow-up triage prompt rather than addressed here.

## Survivors above floor (informational)

`src/secret.rs` (88.9%): three survivors are in `Drop::drop` /
`PartialEq::eq` / `from_bytes`. The first two are well-known weak
spots for mutation detection (Drop's only visible effect is via
the zeroize crate's internal state which the public API does not
expose); the third is a constructor whose only behavioural use is
inside other public methods that the suite already exercises
against fixed inputs. Documented here, not deferred to triage.

`src/audio/pipewire.rs` (93.5%): two survivors plus one timeout
all sit in the `PipeWireController` `Drop` body. The drop logic
spawns a detached cleanup thread whose `JoinHandle` lifetime is
hard to assert against from a unit test. Acceptable per
SubsonicClient pattern. The single timeout (`&&` to `||` in the
drop body) is the same path; the nextest run blocked beyond the
120s --timeout, signal absent.

## Deferred (no baseline captured this run)

Per-file budget of about 30 minutes wall is required for files
with >25 mutants under the current build time (about 90 s build
+ 45 to 70 s test per mutant on this host). The interactive
session budget was about 90 minutes total; these files are
queued for follow-up:

| File                       | Mutant count | Notes                                |
| -------------------------- | -----------: | ------------------------------------ |
| `src/daemon/state.rs`      |           27 | Lock-protected state struct.         |
| `src/daemon/queue_ops.rs`  |           37 | Queue mutators, hot path.            |
| `src/ipc/frame.rs`         |           25 | Length-prefix framing, security hot. |
| `src/audio/mpv.rs`         |          ~80 | Largest critical file (595 LOC).     |
| `src/config/mod.rs`        |         ~90  | Config + RepeatMode (714 LOC).       |
| `src/daemon/playback_tick.rs` |       ~70 | Playback state machine (495 LOC).    |

Counts marked with `~` are estimates from `cargo mutants --list`
on the first invocation; exact totals will be confirmed by the
next baseline run.

## Re-running this baseline

```
rm -rf mutants.out
cargo mutants \
  --file src/io_util.rs \
  --file src/secret.rs \
  --file src/audio/pipewire.rs \
  --file src/daemon/library.rs \
  --test-tool=nextest \
  --baseline=skip \
  --no-shuffle \
  --timeout 120 \
  --jobs 2 \
  --cargo-test-arg '-E' \
  --cargo-test-arg 'not binary(stress_tests)'
```

Expected wall time: about 70 to 80 minutes on a 24-core host with
140 GB free /tmp.

## Verification

- `cargo nextest run` post-baseline: 1233 pass / 0 fail (unchanged).
- HEAD at run time: `2b0d94b` (D.5).
