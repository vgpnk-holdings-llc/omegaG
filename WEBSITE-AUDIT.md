# WEBSITE-AUDIT — omegaG product site

**Date:** 2026-08-01  
**Live URL:** https://ds4cc-proto.kimi.page/  
**In-repo site:** `website/`  
**Mirror snapshot:** `~/Projects/omegag-site/` (`index.fetched.html`, CSS/JS/assets)  
**Repo truth:** `README.md`, `SPEC.md`, `HIGHLIGHTS.md`, `src/` (package name `ds4cc`)  
**Checks:** `node website/checks.mjs` → **pass** (2026-08-01)

---

## 1. What the site is

A single-page dark marketing site for **omegaG** (fork / rebrand of **DS4CC**): DualSense / DualShock 4 → terminal-first shortcut mapper (tmux, Claude Code keybindings, mouse/scroll/mic), with an optional **Windows-only Codex controller runtime** (PS modifier layer, six chat slots, selected-slot lightbar status).

Structure (both live and `website/`):

| Block | Role |
|--------|------|
| Nav | Brand omegaG, anchors Lightbar / Modifier layer / Specs, GitHub, “formerly DS4CC” |
| Hero | Tagline + GitHub CTA |
| Lightbar | DualSense SVG demo + state chips (illustration) |
| Modifier layer | [01]–[03] PS exclusive semantics |
| Device | `masterpiece.png` + etch lines |
| Specs | Platform / controllers / feedback / mapping / detection / Codex / config / stack + default maps |
| Footer | Fork attribution, MIT, upstream link |

Stack: static HTML + CSS + small vanilla JS. No build step required for the page itself. Hosted today on **kimi.page** (Cloudflare edge; injects `https://www.kimi.com/sdk-seed.js` on the live document).

---

## 2. Live vs `website/` vs code (summary)

| Source | Verdict |
|--------|---------|
| **Live `ds4cc-proto.kimi.page`** | **Stale marketing draft.** Overstates Codex/lightbar as general product features; Windows-centric hero; incomplete config/detection copy; uses `hero-journey.png`; footer “Releases” → upstream DS4CC (ambiguous branding). Matches `~/Projects/omegag-site/index.fetched.html`. |
| **In-repo `website/`** | **Accuracy-corrected source of truth for the product page.** Codex/lightbar marked Windows-only; demo labeled illustration; Linux XDG config path; DualSense vs DS4 touchpad nuance; detection WSL vs native; Share/Options unmapped defaults; guarded by `checks.mjs`. **Already committed on `master`** (`website/` tracked). |
| **README / SPEC / src** | Align with **`website/`**, not live. Package/binary/config dir remain **`ds4cc`**. Codex runtime + status→lightbar are **Windows-only** (`config.rs` logs and ignores `enabled` on Linux; SPEC golden rule). Six slots, 350 ms activate/PTT latch, 500 ms pulse, brightness, 180 s sleep in `codex_micro` / README. |

**Primary gap:** live host has **not been redeployed** from `website/`. Fixing accuracy for end users is a **deploy action**, not more HTML churn (unless further product messaging is desired).

---

## 3. Claim accuracy matrix

Legend: **OK** accurate · **LIVE-BAD** wrong/overclaim on live only · **SOFT** imprecise but not false · **GAP** true in code but missing from site

| Claim | Live | `website/` | Code / docs |
|-------|------|------------|-------------|
| Product name omegaG, formerly DS4CC | OK | OK | OK (fork of VeigaPunk/DS4CC) |
| Package / binary / config path `ds4cc` preserved | SOFT (“same config path”) | OK (Windows path + XDG) | OK `config_dir()` → `%APPDATA%\ds4cc` / `$XDG_CONFIG_HOME/ds4cc` \| `~/.config/ds4cc` |
| DualSense + DS4, USB + BT | OK | OK | OK |
| Windows tray + Linux Ubuntu 22.04+ / Arch, evdev/uinput | Partial (hero “Windows tray daemon”) | OK (Linux and Windows hero) | OK SPEC / tray_linux |
| Shortcut mapper: keys, tmux, Claude Code, mouse, scroll, mic | OK | OK | OK |
| Six slots + PS modifier + 350 ms double-press | LIVE-BAD as always-on product | OK as Windows Codex runtime | OK `codex_micro` Windows path |
| Lightbar status projection (idle/thinking/…/unassigned), 500 ms pulse, brightness, 180 s sleep | LIVE-BAD as universal | OK Windows-only + illustration disclaimer | OK status RGB/pulse Windows; static `[lightbar]` RGB also on mapper |
| Static lightbar color / mic LED (mapper) | Buried under status projection claim | OK in Feedback row | OK |
| L2 always PTT / latch | LIVE-BAD | OK “when a voice command is configured” | Generic map: L2 = Ctrl+Win; Codex L2 PTT when runtime + voice config |
| Detection: WSL only | LIVE-BAD | OK WSL Windows + native Linux | OK `detect.rs` / README |
| Codex optional, disabled by default, `codex app-server --stdio` | Partial (no OS gate) | OK Windows-only + Linux ignore | OK `config.rs` Linux warning |
| Default maps (Cross enter, Circle esc, … Share/Options unmapped) | Partial (missing Share/Options / touchpad-disabled) | OK | OK README |
| DS4 no player/mute LEDs; selected-slot only lightbar | OK | OK | OK README |
| Stack Rust 2024, tokio, hidapi; 5 ms in / 100 ms out; MIT | OK | OK | OK (package edition 2024) |
| Launcher named actions (`launcher:…`, godspeed, wtype/xdotool) | GAP | GAP | HIGHLIGHTS.md — not marketed on site |
| Install: upstream `DS4CC-Setup.exe` | Footer “Releases” only | “Upstream DS4CC releases” (clearer) | README Quick Start Windows → upstream |
| OmegaG-owned installers / linux tarball on this site | GAP | GAP | `update.rs` expects `ds4cc-linux-*.tar.gz` style assets; site doesn’t deep-link omegaG releases |

---

## 4. Links (broken / stale / ambiguous)

Checked 2026-08-01 via HTTP:

| Link | Status |
|------|--------|
| https://github.com/vgpnk-holdings-llc/omegaG | **200** |
| https://github.com/VeigaPunk/DS4CC | Present in footer (fork story) |
| https://github.com/VeigaPunk/DS4CC/releases/latest | **302** → `…/releases/tag/v3.1.0` (live) |
| In-page anchors `#top` `#lightbar` `#layer` `#specs` `#device` `#footer` | Present |
| Local assets `website/`: `style.css`, `main.js`, `assets/*` | Exist; `checks.mjs` validates all local `src`/`href` |
| Live `assets/hero-journey.png` | Present on live / mirror; **removed** from `website/` (by design; checks forbid it) |
| Live inject `https://www.kimi.com/sdk-seed.js` | Present on live HTML only (host platform), not in `website/index.html` |

**Ambiguous (not broken):**

- Live footer label **“Releases”** → DS4CC upstream, not omegaG releases (users may think omegaG ships there).
- No CTA for **Linux install** (`cargo build` / `packaging/linux/install.sh`) on either version.
- No link to **omegaG GitHub Releases** if/when they differ from upstream.

**Relative `og:image`:** both use path-style `assets/…`. Social crawlers need an **absolute** URL; previews will be weak until fixed after a stable public origin is chosen.

---

## 5. Brand consistency (omegaG vs DS4CC)

| Surface | Practice | Assessment |
|---------|----------|------------|
| Title / H1 / nav wordmark | **omegaG** primary | Good |
| Tag “formerly DS4CC” | Nav + hero eyebrow + etch | Good — rename story clear |
| Config / package | Still **`ds4cc`** paths and binary | Correct for compatibility; site should keep saying so (repo does) |
| GitHub org | `vgpnk-holdings-llc/omegaG` | Primary product repo |
| Upstream | `VeigaPunk/DS4CC` + releases | Attribution + Windows installer path |
| Host subdomain | **`ds4cc-proto.kimi.page`** | Legacy codename; confuses brand (see deploy notes) |
| Live hero art `hero-journey.png` | Older branded journey art | Dropped in `website/` for neutral controller mark |
| Etch / voice lines | Live: “Voice dictates… No keyboard required.” / Repo: “Configured voice… Keyboard optional when configured.” | Repo safer (matches optional voice) |

**Policy recommendation:** User-facing name **omegaG**; technical identifiers **`ds4cc`**; always say “formerly DS4CC” once near the top; never imply Codex layer is required for the mapper.

---

## 6. kimi.page deploy notes

Observations from live response headers + HTML:

- **HTTP/2 200**, `server: cloudflare`, `cf-cache-status: DYNAMIC`, `cache-control: public,max-age=60,must-revalidate`.
- Document includes **Kimi SDK** script (`sdk-seed.js`) injected by the host, not by repo `website/index.html`.
- Live still serves **old HTML** (hero-journey, overclaim lightbar/Codex, Google Fonts preconnects).
- Repo `website/style.css` uses **system Inter/SFMono stacks** without Google Fonts — intentional offline-friendly; live still pulls Google Fonts.

**Operational implications:**

1. **Source of truth for content is git `website/`**, not the kimi editor draft. Redeploy = replace host files with `website/index.html`, `style.css`, `main.js`, `assets/*` (do **not** upload `website/tools/` or `node_modules`).
2. **Host will re-inject** Kimi scripts; do not bake third-party analytics into the repo unless product wants it.
3. Subdomain **`ds4cc-proto`** signals prototype + old name → plan rename to something like `omegag.*` or project Pages on GitHub/Cloudflare when leaving proto.
4. After deploy, re-run: open live URL, click chips, verify lightbar SVG color; `curl -sL` and grep for `Windows-only` / absence of `hero-journey`.
5. Optional: pin `og:url` / absolute `og:image` to the final public origin.

There is **no in-repo CI/Pages workflow** for this site in the paths reviewed; deploy is currently manual (or Kimi project publish).

---

## 7. Prioritized fix list

### P0 — correctness / user harm (do first)

| ID | Item | Owner |
|----|------|--------|
| P0.1 | **Redeploy live from `website/`** so Codex runtime, lightbar status projection, detection, config paths, and L2/voice caveats match code. | Host / publisher (not pure git) |
| P0.2 | Keep live claims from re-diverging: treat `website/checks.mjs` as gate before any publish. | Process |
| P0.3 | Footer / downloads: never label upstream DS4CC as plain “Releases” without “upstream” (repo already fixed; live still wrong). | Deploy of `website/` |

*In-repo `website/` content: **no remaining P0 accuracy bugs** vs README/SPEC/src as of this audit. `checks.mjs` passes.*

### P1 — important polish / product clarity

| ID | Item |
|----|------|
| P1.1 | Absolute Open Graph / Twitter image + canonical URL once public origin is stable. |
| P1.2 | Hero/footer CTAs: **Windows** → upstream DS4CC installer; **Linux** → clone/build + `packaging/linux/install.sh` (or omegaG release assets when published). |
| P1.3 | Rename host off `ds4cc-proto` when out of prototype. |
| P1.4 | Mention binary name **`ds4cc`** once in Specs (users install/run `ds4cc`, not `omegaG`). |
| P1.5 | Optional one-line note: static `[lightbar]` color exists on both OSes; **status projection** is Windows Codex-only (already split; keep sharp). |
| P1.6 | Ensure `website/tools/` (local node tooling + `node_modules`) stays **untracked / gitignored** and never ships to kimi.page. |

### P2 — nice-to-have

| ID | Item |
|----|------|
| P2.1 | Market **launcher** actions (`launcher:godspeed`, Unicode inject) — HIGHLIGHTS.md feature currently absent from site. |
| P2.2 | Self-host Inter / JetBrains Mono or keep system stack; document choice. |
| P2.3 | Skip-link / a11y already improved in `website/`; keep chips `type="button"` + `aria-pressed` (live chips weaker). |
| P2.4 | Compress `masterpiece.png` (~1.4 MB repo / larger on live) for LCP. |
| P2.5 | Privacy note if Kimi SDK remains on production host. |
| P2.6 | Link SPEC/README for deep technical readers (optional docs section). |

---

## 8. Diff highlights (live snapshot vs `website/`)

Material copy changes already present in repo (and **missing on live**):

- Hero sub: Linux **and** Windows; drops implying Codex is default stack.
- Lightbar eyebrow: **“Windows-only lightbar feedback”**; lede says optional Codex + **illustration, not HID**.
- Layer eyebrow: **“Optional Windows-only Codex controller runtime”**.
- L2 / Share / Options / touchpad defaults tightened to match README.
- Specs: Feedback / Detection / Codex / Config rewritten for dual-platform truth.
- Footer: **“Upstream DS4CC releases”**; Windows config path preserved wording.
- Assets: drop `hero-journey.png`; smaller `controller-mark.png`; `og:image` → controller mark.
- A11y: skip link, `main`, chip types, live region, slot `aria-current`.

Live-only (host / old draft):

- Google Fonts + Kimi `sdk-seed.js`
- Stronger “no keyboard required” etch (overclaim vs optional voice)

---

## 9. Actions taken this session

| Action | Status |
|--------|--------|
| Read `website/*`, mirror `index.fetched.html`, live URL text | Done |
| Cross-check README, SPEC, HIGHLIGHTS, `src/config.rs`, codex/lightbar paths | Done |
| `node website/checks.mjs` | **pass** |
| External links HEAD | omegaG 200; DS4CC latest release 302→v3.1.0 |
| P0 HTML fixes under `website/` | **None required** (already accurate on `master`) |
| This file `WEBSITE-AUDIT.md` | Written |
| Branch `website-audit` + local commit | See git status after commit |
| Push / force-push | Push only if remote allows; **no force-push** |

---

## 10. Recommended next command (human)

```bash
# After review of this audit on branch website-audit:
# 1) Merge audit (and any tiny site tweaks) to master if desired
# 2) Upload website/{index.html,style.css,main.js,assets/*} to kimi.page project
# 3) Hard-refresh live URL and confirm "Windows-only" appears in lightbar section
```

**Bottom line:** The marketing **live site is behind** the accuracy work already in **`website/`**. Treat redeploy as the single highest-value fix; keep `checks.mjs` green before every publish.
