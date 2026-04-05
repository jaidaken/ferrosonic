#!/bin/sh
set -e

REPO="https://github.com/jaidaken/ferrosonic"
INSTALL_DIR="/usr/local/bin"

echo "Ferrosonic installer"
echo "===================="

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
    x86_64) ASSET_REGEX='ferrosonic-[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*-linux-x86_64' ;;
    *)
        echo "No precompiled binary for $ARCH. Please build from source."
        echo "See: $REPO#manual-build"
        exit 1
        ;;
esac

# Detect package manager and install runtime dependencies
if command -v pacman >/dev/null 2>&1; then
    echo "Detected Arch Linux"
    sudo pacman -S --needed --noconfirm mpv pipewire wireplumber dbus
elif command -v dnf >/dev/null 2>&1; then
    echo "Detected Fedora"
    sudo dnf install -y mpv pipewire wireplumber dbus
elif command -v apt >/dev/null 2>&1; then
    echo "Detected Debian/Ubuntu"
    sudo apt update
    sudo apt install -y mpv pipewire wireplumber libdbus-1-3
else
    echo "Unknown package manager. Please install manually: mpv, pipewire, wireplumber, dbus"
    echo "Then re-run this script."
    exit 1
fi

# Optional cava install
echo ""
echo "Optional: cava is an audio visualizer that ferrosonic can display"
echo "alongside the now-playing bar. It is not required but adds a nice"
echo "visual element that changes color with your selected theme."
echo ""
printf "Install cava? [y/N] "
read -r answer </dev/tty
if [ "$answer" = "y" ] || [ "$answer" = "Y" ]; then
    if command -v pacman >/dev/null 2>&1; then
        sudo pacman -S --needed --noconfirm cava
    elif command -v dnf >/dev/null 2>&1; then
        sudo dnf install -y cava
    elif command -v apt >/dev/null 2>&1; then
        sudo apt install -y cava
    else
        echo "Could not install cava automatically. Install it manually from: https://github.com/karlstav/cava"
    fi
    echo "cava installed. Enable it in ferrosonic under Settings (F5)."
else
    echo "Skipping cava. You can install it later and enable it in Settings (F5)."
fi

# Download latest release binary
echo "Downloading ferrosonic..."
API_LATEST="https://api.github.com/repos/jaidaken/ferrosonic/releases/latest"
if ! RELEASE_JSON=$(curl -fsSL "$API_LATEST"); then
    echo "Failed to query latest release metadata from GitHub."
    exit 1
fi

DOWNLOAD_URL=$(printf '%s\n' "$RELEASE_JSON" \
    | grep '"browser_download_url"' \
    | sed -n "s#.*\"\(https://[^\"]*/$ASSET_REGEX\)\".*#\1#p" \
    | head -n1 \
)

if [ -z "$DOWNLOAD_URL" ]; then
    echo "No release asset matching pattern '$ASSET_REGEX' was found."
    exit 1
fi

LATEST=$(printf '%s\n' "$DOWNLOAD_URL" \
    | sed -n 's#.*/ferrosonic-\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\)-linux-x86_64$#\1#p')

TMPFILE=$(mktemp)
if ! curl -fsSL "$DOWNLOAD_URL" -o "$TMPFILE"; then
    echo "Failed to download binary from: $DOWNLOAD_URL"
    rm -f "$TMPFILE"
    exit 1
fi

if [ ! -s "$TMPFILE" ]; then
    echo "Download failed: no binary file was downloaded."
    rm -f "$TMPFILE"
    exit 1
fi

chmod +x "$TMPFILE"

# Install
sudo mv "$TMPFILE" "$INSTALL_DIR/ferrosonic"

echo ""
echo "Ferrosonic $LATEST installed to $INSTALL_DIR/ferrosonic"
echo "Run 'ferrosonic' to start."
