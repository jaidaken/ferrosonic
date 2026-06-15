# Subprocess Leak Bug (found 2026-05-15)

## UPDATE 2026-06-15: daemon-orphan path fixed

The **daemon-never-dies** half is fixed:

- **Idle self-exit** (`DaemonCore::spawn_idle_exit_monitor`, wired in `run.rs`): the
  daemon shuts down after ~30s with no connected IPC clients and playback Stopped.
  `active_clients` is counted via `ClientGuard` in `handle_connection`. Playing/Paused
  keeps it alive so audio continues after the TUI closes.
- **Bounded shutdown** (`run.rs::shutdown`): `quit_mpv` is wrapped in a 3s timeout and a
  5s hard `process::exit` backstop, so a wedged mpv lock can't make the daemon ignore
  SIGTERM (the cause of orphans that needed SIGKILL).
- **Test reaping** (`spawn_daemon_exe`): under a test runner (`NEXTEST` /
  `FERROSONIC_TEST_REAP_DAEMON`) the daemon skips `setsid` (stays in the test's process
  group for a timeout group-kill) and arms `PR_SET_PDEATHSIG(SIGKILL)`.

STILL OPEN: the **mpv/cava child reaping** below (a dying daemon still orphans its mpv
child; the TUI still orphans cava). Those `Child` handles need Drop-based kill + IPC quit.

## Summary

Both `ferrosonicd` and the `ferrosonic` TUI leak child processes. After ~3 days of normal use plus 22 debug daemons left running, the system accumulated:

- 22 `target/debug/ferrosonicd` long-lived daemons (the daemon itself is the leak target here, not its children).
- 30+ `cava` instances dating back days (`-p /tmp/ferrosonic-cava-*.conf` and `/tmp/ferrosonic-cava.conf`).
- 6+ `mpv --idle --input-ipc-server=...` instances orphaned after their parent daemon died. Two distinct socket-path patterns observed:
  - `/tmp/.tmpXXXXXX/ferrosonic-mpv.sock`
  - `/tmp/.tmpXXXXXX/real-mpv.sock`
- 2 zombies attached to the TUI when the daemon died: `cava <defunct>` and `mpv <defunct>`.

Killing the daemon does not clean up its mpv child. Killing the TUI does not clean up its cava child. Children become orphans (reparented to PID 1) and the OS never reclaims them.

## Symptoms

- PID slot pressure (1778 procs at peak).
- Swap usage 23 GiB despite 37 GiB free RAM (long-lived RSS that paged out and never got freed).
- Audio backends still holding `/tmp` sockets after daemon shutdown, blocking re-spawn on next launch.

## Root causes to investigate

1. **mpv shutdown path.** `ferrosonicd` does not send `{"command":["quit"]}` to its mpv IPC socket on shutdown, and does not `SIGTERM` the child as a fallback. Likely the `mpv::Mpv` (or whichever wrapper) drops its `Child` handle without killing it. Two socket-path templates suggest two separate spawn sites with the same bug.
2. **cava lifecycle.** The TUI appears to spawn one `cava` per session (or per visualizer reconfigure) without killing the previous one. Each gets its own `/tmp/ferrosonic-cava-XXXXXX.conf`. The `XXXXXX` suffix implies `mkstemp`-style naming, so configs are not being reused either (separate cleanup issue: stale `/tmp/ferrosonic-cava-*.conf` files likely accumulating).
3. **Child reaping.** When children die before parent, parent never calls `waitpid()`, producing zombies. Either:
   - Install a `SIGCHLD` handler that reaps in a loop, or
   - Set `PR_SET_CHILD_SUBREAPER` + a dedicated reaper task, or
   - Make children their own session leaders with `setsid()` so init reaps them.

## Files worth grepping

```
rg -n 'Command::|Stdio::|process::Child|spawn\(\)' src/
rg -n 'cava|mpv' src/
```

Check whether the `Child` handles are stored, dropped, or awaited. Rust's `std::process::Child` does NOT kill on drop by default. `tokio::process::Child` only kills on drop if you call `kill_on_drop(true)` at spawn time.

## Suggested fixes

- Wrap mpv spawn in a struct with `Drop` that sends `quit` IPC and then `SIGTERM` as a backstop.
- Same pattern for cava (just `SIGTERM`, no IPC).
- Add a daemon-shutdown hook that walks all known children and terminates them before exiting.
- Tests: spawn daemon, kill it, assert no `mpv`/`cava` survive in the process tree of the test harness.

## Reproduction

1. `cargo build` (or run any debug daemon).
2. Run the TUI, let cava attach.
3. `kill <ferrosonicd-pid>`.
4. `pgrep -af 'mpv --idle'` shows the orphan still there.
5. `kill <ferrosonic-tui-pid>` and cava becomes zombie (parent reaps via session teardown but only if you also kill the terminal).

## Cleanup commands used today

```
pkill -KILL -x cava
pkill -KILL -f 'real-mpv.sock'
pkill -KILL -f 'ferrosonic-mpv.sock'
```

Also worth a `find /tmp -maxdepth 2 -name 'ferrosonic-cava-*.conf' -mtime +1 -delete` and `find /tmp -maxdepth 2 -type d -name '.tmp*' -empty -delete` sweep.

## Temp file / directory leaks (related)

Cleanup sweep on 2026-05-15 found:

- **67,391 `/tmp/.tmpXXXXXX/` directories** accumulated over 4 days. Each ferrosonic launch creates one via `tempfile::TempDir` (or equivalent) to hold the mpv IPC socket (`ferrosonic-mpv.sock` / `real-mpv.sock`). The TempDir Drop never fires because the daemon is killed or crashes before normal shutdown, so these survive forever. Most are empty (the socket file is unlinked when mpv exits) but the parent directory remains. This is the same lifecycle bug as the subprocess leak, separate manifestation.
- **2 stale `/tmp/ferrosonic-cava*.conf` files** generated by the cava spawn path. No cleanup hook removes them on visualizer teardown.
- **5 stale install-script-downloaded release binaries in `/tmp`** (~38 MB total): `ferrosonic-0.2.2`, `ferrosonic-0.4.0`, `ferrosonicd-0.4.0`, `ferrosonic-0.4.1`, `ferrosonicd-0.4.1`. The `install.sh` script downloads to `/tmp/` directly instead of a `mktemp -d` work dir, and never removes the artifact after copying into place.

### Suggested fixes

- mpv tempdir: hold the `TempDir` handle inside the same Drop-implementing wrapper that owns the mpv `Child`. Wrapper Drop = quit mpv + drop TempDir (which removes it).
- cava: same pattern. Stash the config-file path in the cava wrapper, delete it in Drop.
- Backstop on daemon start: sweep `/tmp/.tmp*/ferrosonic-mpv.sock` parents older than 1 hour at startup. The daemon already owns `/tmp/ferrosonic-{uid}/`; widen the janitor pass.
- `install.sh`: change `curl -o /tmp/ferrosonic-X.Y.Z-...` to `tmpdir=$(mktemp -d); trap 'rm -rf "$tmpdir"' EXIT; curl -o "$tmpdir/..."`.

### Cleanup commands used today

```
find /tmp -maxdepth 2 -type d -name '.tmp*' -empty -delete
find /tmp -maxdepth 2 -name 'ferrosonic-cava*.conf' -delete
rm -f /tmp/ferrosonic-0.2.2-linux-x86_64 /tmp/ferrosonic-0.4.0-linux-x86_64 /tmp/ferrosonicd-0.4.0-linux-x86_64 /tmp/ferrosonic-0.4.1-linux-x86_64 /tmp/ferrosonicd-0.4.1-linux-x86_64
```

### Test fixture leak (separate, found same sweep)

After the empty-dir sweep above, **14,948 non-empty `/tmp/.tmpXXXXXX/` dirs remained**. Each holds `config.toml` + `queue.json` with synthetic test data (`"song-3"`, `"Test Artist"`, `"Test Album"`). The newest was created at 19:21 the same day the leak was found, so tests are actively producing them at roughly **3,700 per day**.

Mechanism: `TestDaemon::new_with_config_dir(TempDir)` (declared as a test seam in `CLAUDE.md`) calls `tempfile::TempDir::new()`, which creates `/tmp/.tmpXXXXXX/`. `TempDir`'s `Drop` impl deletes the directory, but `Drop` only runs on normal scope exit. If the test panics with `panic = "abort"`, hits a `cargo nextest run` timeout (default 60s, harness sends SIGKILL), or the test process is killed any other way, the dir survives.

Aggravating factors:
- Property tests / proptest cases that panic-and-shrink leak one per panic during shrinking.
- Tests that spawn the real daemon, then kill it abruptly, will have the daemon's own `TempDir` (for mpv socket) leak too (this is the production-side leak above, manifested in tests).
- Test count is high (`cargo nextest run` runs hundreds), so even a few percent leak rate fills /tmp fast.

### Fix options

1. **Move test temp roots to a well-known location.** `tempfile::Builder::new().prefix("ferrosonic-test-").tempdir_in("/tmp/ferrosonic-test-tmp")`. A `cargo nextest run` pre-hook or `build.rs` ensures the parent dir exists, and CI / a pre-commit hook can `rm -rf /tmp/ferrosonic-test-tmp` between runs. Production tempdirs stay where they are.
2. **Add a test-side panic hook** in a `#[ctor]` or `#[test_main]`-style init that registers tempdir paths and removes them on `SIGTERM` / `SIGINT`. Will not help on `SIGKILL`.
3. **Use `tempfile::Builder::disable_cleanup(false)` explicitly + a process-wide `atexit`** that walks a `Mutex<Vec<TempDir>>` registry. Same `SIGKILL` caveat.
4. **Accept the leak, add a janitor.** `tests/setup_teardown.rs` or a `cargo xtask clean-tmp` that runs `find /tmp -maxdepth 2 -name '.tmp*' -name 'ferrosonic*' -mtime +0 -exec rm -rf {} +` before / after the test suite.

Option 1 is the cleanest. The prefix change also makes future sweeps trivial (`rm -rf /tmp/ferrosonic-test-tmp/*` no glob).

### Cleanup command for the test fixture leak

```
find /tmp -maxdepth 2 -type d -name '.tmp*' \
  \( -path '*/config.toml' -o -path '*/queue.json' -prune \) \
  -exec sh -c 'ls "$1" | grep -qE "^(config.toml|queue.json)$" && rm -rf "$1"' _ {} \;
```

Or simpler if confident the only `.tmpXXXXXX` dirs in `/tmp` are ferrosonic test ones:

```
find /tmp -maxdepth 2 -type d -name '.tmp*' -exec rm -rf {} +
```
