# Credits

OmegaG / DS4CC stands on other people's open work. This file names the people
and projects our **free local voice dictation (STT)** path depends on.

## Free voice stack (Linux) — homage first

Commercial products such as [Wispr Flow](https://wisprflow.ai) popularized
hands-free dictation into any app. On Linux we deliberately **do not** ship a
closed STT engine. The free path is built **on top of** these projects:

### 1. hyprwhspr — original creator

| | |
|---|---|
| **Project** | [hyprwhspr](https://github.com/goodroot/hyprwhspr) |
| **Creator** | **[goodroot](https://github.com/goodroot)** |
| **Why we care** | Established the private, local-first, system-wide speech-to-text dictation model for Linux as a serious Wispr Flow alternative — offline models, hotkeys, paste into the focused window. |

**Thank you, goodroot.** OmegaG's free voice story starts with your project.

### 2. hyprwhspr-rs — Rust reimplementation

| | |
|---|---|
| **Project** | [hyprwhspr-rs](https://github.com/better-slop/hyprwhspr-rs) |
| **Creator / maintainers** | **[better-slop](https://github.com/better-slop)** |
| **Why we care** | Native Rust port aimed at Hyprland and Omarchy: whisper.cpp integration, multi-provider STT, systemd/Waybar hooks, and compositor-aware paste. This is the binary OmegaG discovers and launches by default on Linux. |

**Thank you, better-slop.** We integrate and document your tool; we do not claim authorship.

### 3. OpenAI Whisper + whisper.cpp

| | |
|---|---|
| **Whisper** | [openai/whisper](https://github.com/openai/whisper) — OpenAI's open speech recognition models (MIT). OmegaG's free path defaults to the **medium** model size. |
| **whisper.cpp** | [ggml-org/whisper.cpp](https://github.com/ggml-org/whisper.cpp) — portable C/C++ runtime used by hyprwhspr-rs for local inference. |

## What OmegaG adds

- Controller-first workflow (DualSense / DS4) beside free STT
- Tray discovery of `hyprwhspr-rs`, config schema (`[voice]`), and install helper
- Docs that keep **credit lines** visible in logs and packaging

OmegaG is a **consumer and integrator** of this free stack, not a fork that
erases history. Prefer linking and thanking upstream over rebranding.

## Windows note

Windows still documents the commercial [Wispr Flow](https://ref.wisprflow.ai/vgpnk)
pairing for users who want it. The free hyprwhspr-based path is the Linux /
Hyprland / Omarchy default.

## License reminders

Respect each project's license (typically MIT for the tools above). When you
redistribute install scripts that pull their binaries or models, keep notices
and do not strip attribution.
