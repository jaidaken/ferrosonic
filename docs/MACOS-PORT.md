# macOS Port — Handover

This document records the state of the macOS (Intel and Apple Silicon) port and
contains a paste-ready prompt for an OpenCode session running on a Mac.

## Summary

- There is no prebuilt macOS binary and `install.sh` is Linux-only. A native
  build from source is the only way to run on macOS.
- The codebase is close to portable already. Build-time native dependencies are
  effectively nil: TLS is `rustls`, D-Bus is pure-Rust `zbus`, cover-art `chafa`
  is `dlopen`ed at runtime, and credentials use the platform keychain API.
- `x86_64-apple-darwin` (Intel) and `aarch64-apple-darwin` (Apple Silicon) are
  both Rust Tier-1 targets.
- **Status: unverified.** The changes below were developed and checked on Linux.
  The macOS module is `#[cfg(target_os = "macos")]` and therefore is not
  type-checked by the Linux build. The Mac session must build, fix, and smoke
  test.

## What changed in this branch

| File | Change |
|---|---|
| `src/daemon/notify.rs` | Added a `#[cfg(target_os = "macos")]` `Notifier` that shells out to `osascript` (`display notification`). Widened the no-op stub gate to platforms other than Linux/macOS. The module-level doc now covers both backends. |
| `src/audio/pipewire.rs` | `PipeWireController` now records whether the construction probe could execute `pw-metadata`. When it cannot (missing binary, e.g. macOS), `set_rate` / `clear_forced_rate` are silent no-ops instead of warning on every track. Runner-injection tests are unaffected. |
| `README.md` | Marked PipeWire/WirePlumber/D-Bus as Linux-specific in the dependency table, corrected the stale "OpenSSL/D-Bus dev headers" build note, and added a macOS section (Homebrew deps, config path, feature caveats). |
| `.github/workflows/test.yml` | Added a report-only macOS build matrix for `x86_64-apple-darwin` (`macos-15-intel`) and `aarch64-apple-darwin` (`macos-15`). Nothing else in CI changed. |

## Feature behaviour on macOS

| Area | Behaviour |
|---|---|
| Core playback (mpv) | Works. mpv uses CoreAudio. Gapless/prefetch flags are platform-neutral. |
| Daemon + queue persistence | Works. IPC is a Unix domain socket; with no `XDG_RUNTIME_DIR` it falls back to `/tmp/ferrosonic-<uid>/ferrosonicd.sock`. |
| Keychain | Works. keyring v4's default `v1` feature auto-selects macOS Keychain Services. |
| Cover art | Works. `probe_chafa` already tries `libchafa.dylib` and both Homebrew prefixes (`/opt/homebrew/lib` for ARM, `/usr/local/lib` for Intel). Terminal image protocols (iTerm2/kitty/sixel) are handled by `ratatui-image`. |
| Sample-rate switching | **Unavailable.** No PipeWire. Automatically skipped (no warning spam). The decoded quality readout still works. |
| Notifications | Implemented via `osascript`, text only (no cover art). |
| MPRIS | Compiles and runs against a D-Bus session bus, but macOS has no native MPRIS consumer, so it does **not** provide media-key / Control Center integration by itself. |
| systemd unit / installer | N/A. |

## Mac-side checklist

1. Toolchain and runtime:
   ```bash
   xcode-select --install
   rustup toolchain install stable
   brew install mpv cava chafa dbus   # dbus only for MPRIS
   ```
2. Build: `cd upstream && cargo build --release`
3. Lint: `cargo clippy --all-targets --all-features`
4. Smoke test: see the prompt below.
5. Report back: exact commands, observed output, code fixes (file:line), and any
   remaining failures.

## Decision points to settle on the Mac

1. **Native media keys.** If media-key / Control Center integration is required
   (it was listed as a priority), MPRIS is not sufficient on macOS. Options:
   a native Now Playing backend (`souvlaki`, `ctrl`, or a small Objective-C
   shim around `MPNowPlayingInfoCenter` + `MPRemoteCommandCenter`), or accept
   that media keys are unavailable. This is a new subsystem, not a `cfg` fix —
   scope it separately.
2. **Notification cover art.** `osascript` cannot attach an image. If cover art
   in notifications matters, the fallback is the Homebrew `terminal-notifier`
   binary (`-contentImage`), used only when present.
3. **Flip the macOS CI job to gating** once the build is reliably green.

## Transfer

The changes are uncommitted in the `upstream/` working tree on the Linux host.
Move them to the Mac by committing/pushing a branch, or by producing a patch
(`git diff > macos-port.patch`), then apply it before starting the Mac session.

---

## Paste-ready prompt for the Mac session

```text
You are working in the Ferrosonic repo (a Rust terminal Subsonic client). The
project was developed for Linux (PipeWire sample-rate switching, freedesktop
D-Bus notifications/MPRIS). A macOS-prepared patch is applied to the working
tree. Your job is to build it natively on this Intel Mac, fix any remaining
target-specific compile errors, validate runtime behaviour, and report.

Constraints:
- Platform-specific code stays behind #[cfg(target_os = "macos")].
- Do not add dependencies without justification; prefer shelling out to system
  tools for small jobs.
- No unwrap()/expect() in production code. Run `cargo fmt` when done.
- Do not commit or push unless explicitly asked.

Setup:
  xcode-select --install
  rustup toolchain install stable
  brew install mpv cava chafa dbus   # dbus only if you test MPRIS

Build and lint:
1. cd upstream && cargo build --release
   Fix compile errors only; keep changes minimal.
2. cargo clippy --all-targets --all-features

Smoke test (record exact commands and observed results):
- Standalone playback: ./target/release/ferrosonic --standalone against a test
  Navidrome config. Verify play/pause, seek, next/prev, gapless, queue edits,
  ReplayGain, cover art (iTerm2/kitty/sixel), cava, resize, and mouse input.
- Daemon: run normally (daemon mode on). Confirm the daemon auto-spawns
  detached, the TUI reconnects, and the queue + position persist across a
  daemon restart (check queue.json). Confirm --standalone still works.
- Keychain: set the password on the Server page (F5). Confirm it lands in the
  macOS Keychain (`security find-generic-password -s ferrosonic`) and is not
  written as plaintext in the config.
- Notifications: with Notifications=true, play tracks and confirm a banner
  appears. If not, capture the log line and the osascript stderr and report.
- MPRIS: `brew services start dbus` (or `dbus-launch --sh-syntax`), then check:
    dbus-send --session --print-reply --dest=org.freedesktop.DBus \
      /org/freedesktop/DBus org.freedesktop.DBus.ListNames | grep mpris
  Note: macOS has no native MPRIS consumer, so media keys will NOT work via
  MPRIS. Report this and give a recommendation on native Now Playing.

Notes:
- Config/logs/queue live under ~/Library/Application Support/ferrosonic/.
  Override with FERROSONIC_CONFIG_DIR.
- PipeWire is absent; sample-rate switching is expected to be skipped silently.
- The daemon IPC socket falls back to /tmp/ferrosonic-<uid>/ferrosonicd.sock.

Final report: commands run, results, code fixes (file:line), skipped checks, and
remaining risks. Do not claim success just because it compiles.
```
