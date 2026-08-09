#!/bin/sh
# install-voice-hyprwhspr.sh — free local STT for OmegaG / DS4CC on Linux
#
# ============================================================================
# HOMAGE — this stack is not ours to invent. Please keep this header.
# ============================================================================
# Free dictation is based on:
#   • hyprwhspr      — https://github.com/goodroot/hyprwhspr
#                     original creator: goodroot
#   • hyprwhspr-rs   — https://github.com/better-slop/hyprwhspr-rs
#                     Rust reimplementation: better-slop
#   • OpenAI Whisper — medium model by default (via whisper.cpp)
#
# OmegaG only wires discovery, tray launch, and this installer. See CREDITS.md.
# ============================================================================
#
# What it does:
#   1. Ensures cargo / rustc available (for cargo install)
#   2. cargo install hyprwhspr-rs --no-default-features
#   3. Installs whisper-cpp (pacman) or documents apt fallback
#   4. Downloads ggml-medium.bin into hyprwhspr-rs models dir
#   5. Writes ~/.config/hyprwhspr-rs/config.jsonc (whisper medium)
#   6. Seeds ~/.config/ds4cc/config.toml [voice] if missing
#   7. Optional Hyprland bind snippet printed (not auto-merged)
#
# Usage:
#   ./packaging/linux/install-voice-hyprwhspr.sh
#   WHISPER_MODEL=small ./packaging/linux/install-voice-hyprwhspr.sh
#
# POSIX sh, set -eu.
set -eu

MODEL_NAME="${WHISPER_MODEL:-medium}"
# Official ggml Whisper weights (OpenAI Whisper architecture; ggml-org packaging)
MODEL_FILE="ggml-${MODEL_NAME}.bin"
MODEL_URL="${WHISPER_MODEL_URL:-https://huggingface.co/ggerganov/whisper.cpp/resolve/main/${MODEL_FILE}}"

DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/hyprwhspr-rs"
MODEL_DIR="$DATA_DIR/models"
CFG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/hyprwhspr-rs"
DS4CC_CFG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/ds4cc"
CARGO_BIN="${CARGO_HOME:-$HOME/.cargo}/bin"
export PATH="$CARGO_BIN:$HOME/.local/bin:$PATH"

echo "==> OmegaG free voice installer"
echo "    Homage: hyprwhspr (goodroot) → hyprwhspr-rs (better-slop) + Whisper ${MODEL_NAME}"
echo "    See CREDITS.md in the OmegaG repo."
echo

# ── 1. cargo ────────────────────────────────────────────────────────────
if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not found. Install Rust from https://rustup.rs and re-run." >&2
    exit 1
fi

# ── 2. hyprwhspr-rs ─────────────────────────────────────────────────────
if command -v hyprwhspr-rs >/dev/null 2>&1; then
    echo "==> hyprwhspr-rs already on PATH: $(command -v hyprwhspr-rs)"
else
    echo "==> cargo install hyprwhspr-rs --no-default-features (whisper path)"
    cargo install hyprwhspr-rs --no-default-features
fi

HYPR_BIN="$(command -v hyprwhspr-rs)"
echo "    binary: $HYPR_BIN"

# ── 3. whisper-cpp / whisper-cli ────────────────────────────────────────
if command -v whisper-cli >/dev/null 2>&1; then
    echo "==> whisper-cli present: $(command -v whisper-cli)"
elif command -v pacman >/dev/null 2>&1; then
    echo "==> Installing whisper-cpp via pacman (needs sudo)"
    if command -v sudo >/dev/null 2>&1; then
        sudo pacman -S --needed --noconfirm whisper-cpp || {
            echo "warn: pacman whisper-cpp failed — install whisper.cpp so whisper-cli is on PATH" >&2
        }
    else
        echo "warn: no sudo; install package 'whisper-cpp' manually" >&2
    fi
elif command -v apt-get >/dev/null 2>&1; then
    echo "warn: install whisper.cpp / whisper-cli for your distro (no default apt package name)." >&2
    echo "      See https://github.com/ggml-org/whisper.cpp" >&2
else
    echo "warn: install whisper-cli from whisper.cpp" >&2
fi

# ── 4. Whisper model (medium by default) ────────────────────────────────
mkdir -p "$MODEL_DIR"
MODEL_PATH="$MODEL_DIR/$MODEL_FILE"
if [ -f "$MODEL_PATH" ]; then
    echo "==> Model already present: $MODEL_PATH"
else
    echo "==> Downloading OpenAI Whisper ggml model: $MODEL_FILE"
    echo "    $MODEL_URL"
    if command -v curl >/dev/null 2>&1; then
        curl -fL --progress-bar -o "$MODEL_PATH.partial" "$MODEL_URL"
        mv "$MODEL_PATH.partial" "$MODEL_PATH"
    elif command -v wget >/dev/null 2>&1; then
        wget -O "$MODEL_PATH.partial" "$MODEL_URL"
        mv "$MODEL_PATH.partial" "$MODEL_PATH"
    else
        echo "error: need curl or wget to download the Whisper model" >&2
        exit 1
    fi
    echo "    saved: $MODEL_PATH"
fi

# ── 5. hyprwhspr-rs config (whisper medium) ─────────────────────────────
mkdir -p "$CFG_DIR"
CFG_FILE="$CFG_DIR/config.jsonc"
if [ -f "$CFG_FILE" ] && [ "${FORCE_CONFIG:-0}" != "1" ]; then
    echo "==> Keeping existing $CFG_FILE (set FORCE_CONFIG=1 to overwrite)"
else
    echo "==> Writing $CFG_FILE (provider=whisper_cpp, model=${MODEL_NAME})"
    cat >"$CFG_FILE" <<EOF
{
  // OmegaG free STT — homage:
  //   original: https://github.com/goodroot/hyprwhspr (goodroot)
  //   rust:     https://github.com/better-slop/hyprwhspr-rs (better-slop)
  //   models:   OpenAI Whisper via whisper.cpp (ggml-${MODEL_NAME})
  "\$schema": "https://raw.githubusercontent.com/better-slop/hyprwhspr-rs/main/config/schema.json",
  "audio_feedback": true,
  "auto_copy_clipboard": true,
  "transcription": {
    "provider": "whisper_cpp",
    "request_timeout_secs": 60,
    "max_retries": 2,
    "whisper_cpp": {
      "prompt": "Transcribe clearly with proper capitalization and technical terms. Do not invent words.",
      "model": "${MODEL_NAME}",
      "threads": 4,
      "gpu_layers": 999,
      "fallback_cli": true,
      "models_dirs": ["${MODEL_DIR}"]
    }
  }
}
EOF
fi

# ── 6. Seed ds4cc [voice] ───────────────────────────────────────────────
mkdir -p "$DS4CC_CFG_DIR"
DS4CC_CFG="$DS4CC_CFG_DIR/config.toml"
if [ ! -f "$DS4CC_CFG" ]; then
    echo "==> Creating $DS4CC_CFG with free hyprwhspr voice section"
    cat >"$DS4CC_CFG" <<EOF
# OmegaG / DS4CC config — free local STT via hyprwhspr-rs
# Credits: goodroot/hyprwhspr · better-slop/hyprwhspr-rs · OpenAI Whisper

[voice]
# Empty app_command + auto_discover finds hyprwhspr-rs on PATH / cargo bin.
app_command = ""
backend = "hyprwhspr-rs"
whisper_model = "${MODEL_NAME}"
auto_discover = true
# tray_label = "Open hyprwhspr (free Whisper ${MODEL_NAME})"
EOF
elif ! grep -q '^\[voice\]' "$DS4CC_CFG" 2>/dev/null; then
    echo "==> Appending [voice] free-stack defaults to $DS4CC_CFG"
    cat >>"$DS4CC_CFG" <<EOF

# Free local STT — homage: goodroot/hyprwhspr · better-slop/hyprwhspr-rs
[voice]
app_command = ""
backend = "hyprwhspr-rs"
whisper_model = "${MODEL_NAME}"
auto_discover = true
EOF
else
    echo "==> $DS4CC_CFG already has [voice] — leave as-is"
fi

# ── 7. Optional service install ─────────────────────────────────────────
if [ "${HYPRWHSPR_INSTALL_SERVICE:-0}" = "1" ]; then
    echo "==> hyprwhspr-rs install --service"
    hyprwhspr-rs install --service || true
fi

# ── Done ────────────────────────────────────────────────────────────────
echo
echo "Done. Free STT stack ready (Whisper ${MODEL_NAME})."
echo
echo "Homage again:"
echo "  • goodroot — https://github.com/goodroot/hyprwhspr"
echo "  • better-slop — https://github.com/better-slop/hyprwhspr-rs"
echo
echo "Next steps:"
echo "  1. Start daemon:  hyprwhspr-rs"
echo "     or:            systemctl --user enable --now hyprwhspr-rs  # after 'hyprwhspr-rs install'"
echo "  2. Hyprland binds (example — add to ~/.config/hypr/bindings.conf):"
echo "       bind  = ALT, GRAVE, exec, hyprwhspr-rs record start"
echo "       bindr = ALT, GRAVE, exec, hyprwhspr-rs record stop"
echo "       bind  = ALT, SPACE, exec, hyprwhspr-rs record toggle"
echo "  3. OmegaG tray item: Open hyprwhspr (free Whisper ${MODEL_NAME})"
echo "  4. Full credits: CREDITS.md"
echo
