# Keybindings Reference

Ferrosonic is fully keyboard-driven. Vim-style `j`/`k` navigation is available alongside arrow keys throughout the application.

## Global

| Key | Action |
|---|---|
| `q` | Quit |
| `p` / `Space` | Toggle play/pause |
| `l` | Next track |
| `h` | Previous track |
| `Ctrl+R` | Refresh data from server |
| `t` | Cycle to next theme |
| `F1` | Songs page |
| `F2` | Artists page |
| `F3` | Queue page |
| `F4` | Playlists page |
| `F5` | Server configuration page |
| `F6` | Settings page |

## Songs Page (F1)

| Key | Action |
|---|---|
| `Tab` | Switch focus between song options and song list |
| `Up` / `k` | Move selection up |
| `Down` / `j` | Move selection down |
| `Enter` | Play selected song (queues all visible songs and starts from selection) |

The Songs page has two modes selectable from the options pane: **Starred** (your favourited songs) and **Random** (a random selection from the server).

## Artists Page (F2)

| Key | Action |
|---|---|
| `/` | Filter artists by name |
| `Esc` | Clear filter |
| `Up` / `k` | Move selection up |
| `Down` / `j` | Move selection down |
| `Left` / `Right` | Switch focus between tree and song list |
| `Enter` | Expand/collapse artist, or play album/song |
| `Backspace` | Return to tree from song list |
| `e` | Add selected item to end of queue |
| `n` | Add selected item as next in queue |
| `s` | Shuffle play all songs by the selected artist or album |

## Queue Page (F3)

| Key | Action |
|---|---|
| `Up` / `k` | Move selection up |
| `Down` / `j` | Move selection down |
| `Enter` | Play selected song |
| `d` | Remove selected song from queue |
| `J` (Shift+J) | Move selected song down |
| `K` (Shift+K) | Move selected song up |
| `s` | Shuffle queue (current song moves to the front) |
| `c` | Clear played history (remove songs before current) |

## Playlists Page (F4)

| Key | Action |
|---|---|
| `Tab` / `Left` / `Right` | Switch focus between playlists and songs |
| `Up` / `k` | Move selection up |
| `Down` / `j` | Move selection down |
| `Enter` | Load playlist songs or play selected song |
| `e` | Add selected item to end of queue |
| `n` | Add selected song as next in queue |
| `s` | Shuffle play all songs in selected playlist |

## Server Page (F5)

| Key | Action |
|---|---|
| `Tab` | Move between fields |
| `Enter` | Test connection or Save configuration |
| `Backspace` | Delete character in text field |

## Settings Page (F6)

| Key | Action |
|---|---|
| `Up` / `Down` | Move between settings |
| `Left` | Previous option |
| `Right` / `Enter` | Next option |

Settings include theme selection and cava visualizer toggle. Changes are saved automatically.
