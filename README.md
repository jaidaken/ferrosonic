# Ferrosonic
![Release](https://github.com/jaidaken/ferrosonic/actions/workflows/release.yml/badge.svg)
![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)

A terminal-based Subsonic music client written in Rust, featuring bit-perfect audio playback, gapless transitions, and full desktop integration.

Ferrosonic is a continuation of the original [ferrosonic](https://github.com/jaidaken/ferrosonic) by jaidaken, which is no longer actively maintained. Originally a ground-up rewrite of [Termsonic](https://git.sixfoisneuf.fr/termsonic/about/) in Rust, it features PipeWire sample rate switching for bit-perfect audio, MPRIS2 media controls, multiple color themes, and mouse support.

## Features

- **Bit-perfect audio** — Automatic PipeWire sample rate switching to match source material (44.1kHz, 48kHz, 96kHz, 192kHz, etc.), with the original rate restored on exit
- **Gapless playback** — Next track is pre-loaded into mpv's internal playlist for seamless transitions
- **MPRIS2 integration** — Full desktop media controls (play, pause, stop, next, previous, seek)
- **Artist/album browser** — Tree-based navigation with expandable artists, album listings, and artist filtering
- **Songs page** — Browse starred and random songs from your server
- **Playlists & queue management** — Browse server playlists, add/remove/reorder/shuffle queue, clear history
- **Audio quality display** — Real-time sample rate, bit depth, codec, and channel layout
- **Audio visualizer** — Integrated [cava](https://github.com/karlstav/cava) visualizer with theme-matched gradient colors
- **13 built-in themes + custom themes** — Monokai, Dracula, Nord, Catppuccin, Tokyo Night, and more. Create your own as TOML files in `~/.config/ferrosonic/themes/`. See the [themes documentation](docs/themes.md)
- **Mouse support** — Clickable tabs, playback controls, list items, and progress bar seeking
- **Keyboard-driven** — Vim-style navigation (j/k) alongside arrow keys. See the [full keybindings reference](docs/keybindings.md)
- **Multi-disc album support** — Proper disc and track number display

## Screenshots

![Ferrosonic](docs/screenshots/ferrosonic.png)

## Installation

### Dependencies

Ferrosonic requires the following at runtime:

| Dependency | Purpose | Required |
|---|---|---|
| **mpv** | Audio playback engine (via JSON IPC) | Yes |
| **PipeWire** | Automatic sample rate switching for bit-perfect audio | Recommended |
| **WirePlumber** | PipeWire session manager | Recommended |
| **D-Bus** | MPRIS2 desktop media controls | Recommended |
| **cava** | Audio visualizer | Optional |

### Quick Install

Supports Arch, Fedora, and Debian/Ubuntu. Installs runtime dependencies, downloads the latest precompiled binary, and installs to `/usr/local/bin/`:

```bash
curl -sSf https://raw.githubusercontent.com/jaidaken/ferrosonic/master/install.sh | sh
```

### Install via Cargo

```
cargo install ferrosonic
```

### Build from Source

If you prefer to build from source, you'll also need: Rust toolchain, pkg-config, OpenSSL dev headers, and D-Bus dev headers. Then:

```bash
git clone https://github.com/jaidaken/ferrosonic.git
cd ferrosonic
cargo build --release
sudo cp target/release/ferrosonic /usr/local/bin/
```

## Usage

```bash
# Run with default config (~/.config/ferrosonic/config.toml)
ferrosonic

# Run with a custom config file
ferrosonic -c /path/to/config.toml

# Enable verbose/debug logging
ferrosonic -v
```

## Configuration

Configuration is stored at `~/.config/ferrosonic/config.toml`. You can edit it manually or configure the server connection through the application's Server page (F5).

```toml
BaseURL = "https://your-subsonic-server.com"
Username = "your-username"
Password = "your-password"
Theme = "Default"
```

| Field | Description |
|---|---|
| `BaseURL` | URL of your Subsonic-compatible server (Navidrome, Airsonic, Gonic, etc.) |
| `Username` | Your server username |
| `Password` | Your server password |
| `Theme` | Color theme name (e.g. `Default`, `Catppuccin`, `Tokyo Night`) |

Logs are written to `~/.config/ferrosonic/ferrosonic.log`.

## Themes
Ferrosonic ships multiple built-in themes, as well as support for custom themes. Here are two examples:
<!-- A file in docs/ should be added with every built-in theme to show them off fully, these are just examples -->

| Nord | Gruvbox |
|---|---|
| <img src="docs/screenshots/nord_theme.avif" alt="Nord theme" width="640" height="327" /> | <img src="docs/screenshots/gruvbox_theme.avif" alt="Gruvbox theme" width="640" height="327" /> |

To know more about themes, **visit the [themes documentation](docs/themes.md)**.

## Compatible Servers

Ferrosonic works with any server implementing the Subsonic API, including:

- [Navidrome](https://www.navidrome.org/)
- [Airsonic](https://airsonic.github.io/)
- [Airsonic-Advanced](https://github.com/airsonic-advanced/airsonic-advanced)
- [Gonic](https://github.com/sentriz/gonic)
- [Supysonic](https://github.com/spl0k/supysonic)

## Contributing
 
Contributions are welcome! Feel free to open an issue or submit a pull request.
 
## License
 
This project is licensed under the [MIT License](LICENSE).

## Acknowledgements

Ferrosonic is inspired by [Termsonic](https://git.sixfoisneuf.fr/termsonic/about/) by SixFoisNeuf, a terminal Subsonic client written in Go. Ferrosonic builds on that concept with a Rust implementation, bit-perfect audio via PipeWire, and additional features.
