# Themes

Ferrosonic ships with 13 themes. On first run, the built-in themes are written as TOML files to `~/.config/ferrosonic/themes/`.

| Theme | Description |
|---|---|
| **Default** | Cyan/yellow on dark background (hardcoded) |
| **Monokai** | Classic Monokai syntax highlighting palette |
| **Dracula** | Purple/pink Dracula color scheme |
| **Nord** | Arctic blue Nord palette |
| **Gruvbox** | Warm retro Gruvbox colors |
| **Catppuccin** | Soothing pastel Catppuccin Mocha palette |
| **Solarized** | Ethan Schoonover's Solarized Dark |
| **Tokyo Night** | Dark Tokyo Night color scheme |
| **Rosé Pine** | Soho vibes Rosé Pine palette |
| **Everforest** | Comfortable green Everforest Dark |
| **Kanagawa** | Dark Kanagawa wave palette |
| **One Dark** | Atom One Dark color scheme |
| **Ayu Dark** | Ayu Dark color scheme |

Change themes with `t` from any page, from the Settings page (F6), or by editing the `Theme` field in `config.toml`.

### Custom Themes

Create a `.toml` file in `~/.config/ferrosonic/themes/` and it will appear in the theme list. The filename becomes the display name (e.g. `my-theme.toml` becomes "My Theme").

```toml
[colors]
primary = "#89b4fa"
secondary = "#585b70"
accent = "#f9e2af"
artist = "#a6e3a1"
album = "#f5c2e7"
song = "#94e2d5"
muted = "#6c7086"
highlight_bg = "#45475a"
highlight_fg = "#cdd6f4"
success = "#a6e3a1"
error = "#f38ba8"
playing = "#f9e2af"
played = "#6c7086"
border_focused = "#89b4fa"
border_unfocused = "#45475a"

[cava]
gradient = ["#a6e3a1", "#94e2d5", "#89dceb", "#74c7ec", "#cba6f7", "#f5c2e7", "#f38ba8", "#f38ba8"]
horizontal_gradient = ["#f38ba8", "#eba0ac", "#fab387", "#f9e2af", "#a6e3a1", "#94e2d5", "#89b4fa", "#cba6f7"]
```

You can also edit the built-in theme files to customize them. They will not be overwritten unless deleted.