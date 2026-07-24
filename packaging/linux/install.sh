#!/bin/sh
# ds4cc Linux installer — Ubuntu 22.04+ / Arch.
#
# What it does:
#   1. Installs runtime dependencies (apt or pacman)
#   2. Creates the `uinput` group and adds you to `uinput` + `input`
#   3. Installs the udev rule (hidraw controller access + uinput node)
#   4. Installs the ds4cc binary to ~/.local/bin
#   5. Installs + enables the systemd --user unit (XDG autostart fallback)
#
# Usage:
#   ./install.sh                 # find the binary automatically
#   DS4CC_BIN=/path/to/ds4cc ./install.sh
#
# POSIX sh, no bashisms.
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

SUDO=""
if [ "$(id -u)" -ne 0 ]; then
    if ! command -v sudo >/dev/null 2>&1; then
        echo "error: need root or sudo to install udev rules and groups" >&2
        exit 1
    fi
    SUDO="sudo"
fi

# ── 1. Runtime dependencies ─────────────────────────────────────────────

if command -v apt-get >/dev/null 2>&1; then
    echo "==> Installing runtime dependencies (apt)"
    $SUDO apt-get update -qq
    # libudev1: hidapi backend; tar: self-update extraction; tmux: optional,
    # enables keybind auto-detection from your tmux server.
    $SUDO apt-get install -y libudev1 tar tmux
elif command -v pacman >/dev/null 2>&1; then
    echo "==> Installing runtime dependencies (pacman)"
    # systemd-libs provides libudev; tmux is optional (keybind detection).
    $SUDO pacman -S --needed --noconfirm systemd-libs tar tmux
else
    echo "error: unsupported distro — need apt-get (Ubuntu/Debian) or pacman (Arch)" >&2
    exit 1
fi

# ── 2. Groups + uinput module ───────────────────────────────────────────

echo "==> Creating uinput group and adding $USER to uinput,input"
$SUDO groupadd -f uinput
$SUDO usermod -aG uinput,input "$USER"

echo "==> Loading uinput module (now + at boot)"
$SUDO modprobe uinput || true
echo uinput | $SUDO tee /etc/modules-load.d/ds4cc-uinput.conf >/dev/null

# ── 3. udev rule ────────────────────────────────────────────────────────

echo "==> Installing udev rule /etc/udev/rules.d/99-ds4cc.rules"
$SUDO install -m 0644 "$SCRIPT_DIR/99-ds4cc.rules" /etc/udev/rules.d/99-ds4cc.rules
$SUDO udevadm control --reload-rules
$SUDO udevadm trigger || true

# ── 4. Binary → ~/.local/bin ────────────────────────────────────────────

if [ -n "${DS4CC_BIN:-}" ]; then
    BIN="$DS4CC_BIN"
elif [ -f "$SCRIPT_DIR/../../target/release/ds4cc" ]; then
    BIN="$SCRIPT_DIR/../../target/release/ds4cc"
elif [ -f "$SCRIPT_DIR/ds4cc" ]; then
    BIN="$SCRIPT_DIR/ds4cc"
else
    echo "error: ds4cc binary not found." >&2
    echo "       Build it with 'cargo build --release' or set DS4CC_BIN=/path/to/ds4cc" >&2
    exit 1
fi

echo "==> Installing $BIN to ~/.local/bin/ds4cc"
mkdir -p "$HOME/.local/bin"
install -m 0755 "$BIN" "$HOME/.local/bin/ds4cc"

# ── 5. Autostart: systemd --user unit (XDG autostart fallback) ──────────

if command -v systemctl >/dev/null 2>&1 && systemctl --user show-environment >/dev/null 2>&1; then
    echo "==> Installing systemd user unit and enabling it"
    mkdir -p "$HOME/.config/systemd/user"
    install -m 0644 "$SCRIPT_DIR/ds4cc.service" "$HOME/.config/systemd/user/ds4cc.service"
    systemctl --user daemon-reload
    systemctl --user enable --now ds4cc.service
else
    echo "==> systemctl --user unavailable — falling back to XDG autostart"
    mkdir -p "$HOME/.config/autostart"
    install -m 0644 "$SCRIPT_DIR/ds4cc.desktop" "$HOME/.config/autostart/ds4cc.desktop"
fi

# ── Done ────────────────────────────────────────────────────────────────

cat <<'EOF'

==> ds4cc installed.

NEXT STEPS:
  * Log out and back in — group membership (uinput, input) and the udev
    rule only take effect for new sessions.
  * Make sure ~/.local/bin is on your PATH (default on Ubuntu 22.04+).

BLUETOOTH PAIRING (DualSense / DS4):
  1. Hold PS + Share until the light bar flashes rapidly (pairing mode).
  2. Pair "Wireless Controller" from your desktop's Bluetooth settings.
  USB works out of the box and takes priority; BT is the automatic fallback.

TRAY ICON:
  * KDE Plasma: works natively.
  * GNOME: install the "AppIndicator and KStatusNotifierItem Support"
    extension, otherwise no tray icon is shown (the daemon still runs).

CONFIG:
  ~/.config/ds4cc/config.toml   (all settings optional)
EOF
