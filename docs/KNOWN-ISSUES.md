---
description: Accepted and deferred items for ferrosonic - low-value stabilization tail, cargo-deny advisory ignores, clippy backlog, CI carve-outs, mutation known-open seams. Read before re-filing one of these as a bug.
tags: [known-issues, stabilization, deferred, security]
date: 2026-06-15
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

## build hygiene (intentional backlog)

- clippy pedantic + nursery: ~847 warnings at the crate root (`#![warn(pedantic, nursery, missing_docs)]`). INTENTIONAL triage backlog per `CLAUDE.md` rule 0, NOT noise to bulk-silence. the gating clippy job compiles (no `-D`); `unwrap_used`/`expect_used` ARE denied on lib+bins (`unwrap_check` CI job). quiet selectively only when a suggestion is genuinely wrong.

## CI carve-outs

- coverage job = report-only, best-effort: nextest `coverage` profile (`.config/nextest.toml`) drops the 11 real-binary e2e test files (they exec a separate process -> no in-process coverage, and the instrumented child races profile-write vs signal-exit); collect step is `continue-on-error`. coverage is not a gate.
- subprocess/PTY tests flaky under parallel CI: the gating nextest job runs the `ci` profile (`retries=2`) so a known-flaky timing test retries instead of reddening the gate. a real break still fails all attempts.

## mutation known-open seams (deferred depth pass)

real (behaviour-changing) survivors that need a test seam not yet built; NOT provably-equivalent (those live in [mutants_exclusions](mutants_exclusions.md)). detail in [TESTING](TESTING.md) CURRENT section.

- `core.rs` RAII guards (LoadingFlagOwner/PrebufferGate/CancelSlotCleaner disarm+drop): track-switch cancel race; needs a concurrent rapid-switch harness (loom or staged Buffered plays).
- `core.rs` mpv EOF event listener (`reason != "eof"`, `count >= 2`): gapless auto-advance gating; needs a FakeMpv unsolicited-event injection seam.
- `core.rs` prebuffer streaming thresholds: perf-timing, loads the same song; low correctness value.
- `playback_tick.rs` 1500ms / 5s debounce boundaries: `std::time::Instant`, tokio fake-time can't reach; needs a clock-injection seam.
