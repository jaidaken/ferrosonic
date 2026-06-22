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

clippy pedantic + nursery at the crate root (`#![warn(pedantic, nursery, missing_docs)]`). 2026-06-22 cleanup pass took lib+bins 897 -> ~131. gating clippy job compiles (no `-D`); `unwrap_used`/`expect_used` denied on lib+bins (`unwrap_check` CI job).

NO GLOBAL SILENCES. `src/lib.rs` carries only `#![warn(...)]` (zero crate `#![allow]`); `Cargo.toml [lints.clippy]` has no `allow`; `clippy.toml` has no `too-many-lines-threshold` override (default 100). Every suppression is a real fix or a per-site inline `#[allow]` with a one-line rationale at the marker. A future regression of any of these lints surfaces again instead of passing silently.

FIXED (not suppressed): all 114 `missing_errors_doc`; `uninlined_format_args` (446); input handlers -> `&self` (12 fns); `assigning_clones` -> `clone_from` (24); 10 unused imports; `manual_let_else`. 2026-06-22 de-silencing pass added: `drop_non_drop` (72) -> `let _ = state` borrow-release (no-op `drop()` removed); int->int narrowing casts -> saturating `try_from` helpers in `src/num.rs` (`u16_sat`/`u32_sat`/`usize_sat`/`i32_sat`/`u8_sat`); list-index math -> `usize::checked_add_signed`; `option_if_let_else` (9 of 17) -> `map_or`/`map_or_else`.

SUPPRESSED per-site (inline `#[allow]` + rationale at each marker, no global allow):
- float->int casts (`as` is saturating since Rust 1.45): scrobble/state durations, mpris volume/position/seek, footer/widget kHz + progress. each a per-site allow.
- `frame.rs` length prefix: exact value, guarded `<= MAX_FRAME_BYTES` directly above; per-site allow.
- `chafa_ext::argb_to_color`: const fn, bytes masked `& 0xff`, `try_from` not const-stable; per-fn allow.
- `option_if_let_else` (8 of 17): side-effecting arms, nested `map_or_else`, parser/iterator-state, or label-fallback where the explicit `None` arm reads clearer; per-site allows.
- `struct_excessive_bools` (5): `Config`/`ConfigOnDisk`/`ClientState`/`SettingsState`/`PlaybackTickInputs` -- independent orthogonal flags, not state machines; per-struct allows.
- `match_same_arms` (4 fn-scoped allows): arms intentionally explicit / structurally unmergeable.
- `too_many_lines` (16 fns >100 lines): cohesive key/mouse dispatchers + page renders; per-fn allows. the 4 >250 (`handle_library_key` 685, `handle_playlists_key` 349, ipc client `dispatch` 320, `apply_event` 258) remain split-candidates, allowed-with-reason rather than silently under a raised threshold.

DEFERRED (per-case review queued, still warning - NOT silenced):
- `significant_drop_tightening` (79): nursery lock-guard tightening; mix of genuine contention wins and false-positives where the guard must live; needs a per-site pass, not a bulk change.
- minor pedantic tail (~50: `first_doc_paragraph_too_long`, `large_enum_variant`, `needless_pass_by_value`, float `==`, `similar_names`, `format_push_string`, etc.): low value, fix opportunistically. NOT noise to bulk-silence per `CLAUDE.md` rule 0; quiet selectively only when a suggestion is genuinely wrong.

## CI carve-outs

- coverage job = report-only, best-effort: nextest `coverage` profile (`.config/nextest.toml`) drops the 11 real-binary e2e test files (they exec a separate process -> no in-process coverage, and the instrumented child races profile-write vs signal-exit); collect step is `continue-on-error`. coverage is not a gate.
- subprocess/PTY tests flaky under parallel CI: the gating nextest job runs the `ci` profile (`retries=2`) so a known-flaky timing test retries instead of reddening the gate. a real break still fails all attempts.

## mutation known-open seams (deferred depth pass)

real (behaviour-changing) survivors that need a test seam not yet built; NOT provably-equivalent (those live in [mutants_exclusions](mutants_exclusions.md)). detail in [TESTING](TESTING.md) CURRENT section.

- `core.rs` RAII guards (LoadingFlagOwner/PrebufferGate/CancelSlotCleaner disarm+drop): track-switch cancel race; needs a concurrent rapid-switch harness (loom or staged Buffered plays).
- `core.rs` mpv EOF event listener (`reason != "eof"`, `count >= 2`): gapless auto-advance gating; needs a FakeMpv unsolicited-event injection seam.
- `core.rs` prebuffer streaming thresholds: perf-timing, loads the same song; low correctness value.
- `playback_tick.rs` 1500ms / 5s debounce boundaries: `std::time::Instant`, tokio fake-time can't reach; needs a clock-injection seam.
