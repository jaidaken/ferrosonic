---
description: Accepted and deferred items for ferrosonic - low-value stabilization tail, cargo-deny advisory ignores, clippy backlog, CI carve-outs, mutation known-open seams. Read before re-filing one of these as a bug.
tags: [known-issues, stabilization, deferred, security]
date: 2026-06-22
---

# KNOWN ISSUES (accepted / deferred)

NOT bugs to fix now. each = decided-acceptable w/ reason. re-opening needs a new reason, not a rediscovery. companion to [STABILIZATION](STABILIZATION.md) (status re-baseline 2026-06-15).

## IPC hardening (deferred, low-value)

scope = localhost single-user Unix socket; the threat model these guard against is remote/multi-tenant, absent here.

- `Hello`/`protocol_version` handshake: NONE. version skew already handled leniently (unknown variants -> Err, connection survives). full handshake = defer.
- per-frame version tag: NONE. forward-compat vs corruption indistinguishable; unobserved in practice.
- `CancelRequest`: NONE. only matters for long ops a client wants to abort; none today.
- `Resync`-on-`Lagged`: client resubscribes silently; no explicit resync event. rare, accepted.
- DONE this scope: frame caps (`MAX_FRAME_BYTES` 16MiB + tighter `MAX_REQUEST_FRAME_BYTES`); per-connection idle timeout (45s) + client keepalive ping (15s).

## resource / security (deferred, low-incidence)

- cava raw-FD RAII guard: `cava_pipe.rs` uses `from_raw_fd` w/o a guard; a panic between dup and ownership could leak an FD. low incidence; `set_die_with_parent` + `stop_cava` cover the lifecycle.
- mpv reader single-line framing: assumes one JSON per line. holds in practice (mpv emits line-delimited); parser is fuzz-guarded. length-prefix = defer.
- `queue.json` 0o600: written in the config dir (user-owned), not `/tmp`; song ids are not secrets. defer.

## deps / cargo-deny (accepted advisory ignores)

`deny.toml` ignores, each justified inline:

- RUSTSEC-2024-0436 `paste` unmaintained: transitive via ratatui 0.29; no fixed release exists; not a vulnerability.
- RUSTSEC-2026-0097 `rand` 0.9 unsound: dev-only (proptest); shipped binary uses rand 0.8 and no custom `rand::rng()` logger. not reachable.
- duplicate-version warnings (`base64`, `hashbrown`, `thiserror` 1+2, etc.): `multiple-versions = "warn"`, transitive, non-gating. accepted.

## build hygiene (clippy backlog)

clippy pedantic + nursery at the crate root (`#![warn(pedantic, nursery, missing_docs)]`). 2026-06-22 cleanup took lib+bins 897 -> **0**. gating clippy job compiles (no `-D`); `unwrap_used`/`expect_used` denied on lib+bins (`unwrap_check` CI job).

NO GLOBAL SILENCES. `src/lib.rs` carries only `#![warn(...)]` (zero crate `#![allow]`); `Cargo.toml [lints.clippy]` has no `allow`; `clippy.toml` has no `too-many-lines-threshold` override (default 100). Every former warning is a real fix or a per-site inline `#[allow]` with a one-line rationale at the marker. A future regression of any lint surfaces again instead of passing silently.

FIXED (not suppressed): `missing_errors_doc` (114); `uninlined_format_args` (446); input handlers -> `&self` (12 fns); `assigning_clones` -> `clone_from` (24); unused imports; `manual_let_else` (10); `drop_non_drop` (72, `drop(state)` -> `let _ = state`); int->int narrowing casts -> saturating `try_from` helpers in `src/num.rs`; list-index math -> `usize::checked_add_signed`; `option_if_let_else` (9 of 17) -> `map_or`/`map_or_else`; `large_enum_variant` (3, `NowPlaying` boxed at `DaemonEvent::NowPlayingChanged`); `format_push_string` (3, `write!`); `too_long_first_doc_paragraph` (6); `disallowed_methods` (theme seed -> `io_util::atomic_write_bytes`); `significant_drop_in_scrutinee`; `len_without_is_empty`/`new_without_default` (+impls); `ref_option_ref` (dropped `serialize_with` for a `RevealedSecret` newtype); plus `explicit_counter_loop`, `manual_clamp`, `items_after_statements`, `unreadable_literal`, `similar_names` (stale->is_stale), `match_same_arms` (1, merged `mpv is_running` duplicate arms).

`significant_drop_tightening` (79): every guard is a `tokio` lock (`await_holding_lock = deny` already covers the std mutexes; tokio guards are allowed to span an await, so the lint must be judged per site, not assumed sync-only). A per-site compile-and-await gate split them: a handful of free-standing guards whose early `drop()` both compiles and adds no cross-await risk got the drop (`stamp_loadfile`, `playback_tick`, `socket_client`, `mpris`); the rest carry a per-fn allow whose reason states why tightening is unsafe - borrow-blocked (early-drop fails to compile), spans a trailing await (a tokio early-drop there changes cross-await behavior, e.g. the `notify` cover-write serialization), or saves nothing before return. `notify::cover_uri` keeps its `cover_file` lock across the `spawn_blocking` write deliberately, with an allow.

SUPPRESSED per-site (inline `#[allow]` + rationale at each marker, no global allow):
- float->int casts (`as` saturating since Rust 1.45); `frame.rs` length prefix (guarded `<= MAX_FRAME_BYTES`); `chafa_ext::argb_to_color` (const fn, masked `& 0xff`).
- `option_if_let_else` (8 of 17): side-effecting arms / nested `map_or_else` / parser-iterator state / label fallback.
- `struct_excessive_bools` (5): orthogonal-flag structs, not state machines.
- `match_same_arms` (3 fn-scoped): arms kept explicit per enum-variant / recv-outcome / setting.
- `too_many_lines` (16 fns): cohesive dispatchers/renders; the 4 >250 are tracked split-candidates.
- `needless_pass_by_value` (by-value constructors + a `map_err` fn-item); `too_many_arguments` (freedesktop `Notify` D-Bus signature); `implicit_hasher`/`map_entry` (transitional shim); `float_cmp` (exact integer-value test); `significant_drop_tightening` (36 fns, above).

TEST TREE: 6 dead `zombie_processes` crate-allows removed; `sigterm_graceful_exit` keeps a per-fn allow (child reaped on all paths, clippy can't prove it through the loop); `cava_drain` `ReadEnd::Eof` now exercised (was dead); `tests/common` keeps a module `#![allow(dead_code, unused_imports)]` (shared-harness idiom, structural). Warn-level `unwrap`/`expect`/`panic` in tests are accepted (gate is lib+bins only).

DEFERRED: none. backlog cleared.

## CI carve-outs

- coverage job = report-only, best-effort: nextest `coverage` profile (`.config/nextest.toml`) drops the 11 real-binary e2e test files (they exec a separate process -> no in-process coverage, and the instrumented child races profile-write vs signal-exit); collect step is `continue-on-error`. coverage is not a gate.
- subprocess/PTY tests flaky under parallel CI: the gating nextest job runs the `ci` profile (`retries=2`) so a known-flaky timing test retries instead of reddening the gate. a real break still fails all attempts.

## mutation known-open seams

real (behaviour-changing) survivors; NOT provably-equivalent (those live in [mutants_exclusions](mutants_exclusions.md)). detail in [TESTING](TESTING.md) CURRENT section.

CLOSED 2026-06-22:
- `core.rs` mpv EOF event listener (`reason != "eof"`, `count >= 2`): `FakeMpv::emit_end_file` injection seam + `tests/mpv_eof_advance.rs`. Mutant-verified (the `!=`-flip and the `>= 2` boundary both fail the tests).
- `playback_tick.rs` 1500ms / 5s debounce boundaries: extracted to pure `within_just_loaded_window` / `preload_due` (parameterized on `now`); boundary tests in the `boundary_tests` module. Mutant-verified (`<`->`<=`, `>=`->`>` both fail).

OPEN:
- `core.rs` RAII guards (LoadingFlagOwner/PrebufferGate/CancelSlotCleaner disarm+drop): track-switch cancel race. The hard one: needs a `FakeSubsonic` controllable/blocking-stream seam so a Buffered prebuffer can be held mid-flight and superseded deterministically, then assert the superseded task's Drop clears only its own flag. Not built yet; deferred to avoid a flaky concurrent test.
- `core.rs` prebuffer streaming thresholds: perf-timing, loads the same song; low correctness value, left alone.
