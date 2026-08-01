# WEBSITE-AUDIT — omegaG product site

**Date:** 2026-08-01 (updated same day)  
**Live URL:** https://ds4cc-proto.kimi.page/  
**In-repo site:** `website/`  
**Mirror snapshot:** `~/Projects/omegag-site/` (`index.fetched.html`, CSS/JS/assets)  
**Repo truth:** `README.md`, `SPEC.md`, `HIGHLIGHTS.md`, `src/` (package name `ds4cc`)  
**Checks:** `node website/checks.mjs` → **pass**  
**Branch:** `website-audit` (includes hero rebrand from `website-hero-omegag`)

---

## 1. What the site is

A single-page dark marketing site for **omegaG** (fork / rebrand of **DS4CC**): DualSense / DualShock 4 → terminal-first shortcut mapper (tmux, Claude Code keybindings, mouse/scroll/mic), with an optional **Windows-only Codex controller runtime** (PS modifier layer, six chat slots, selected-slot lightbar status).

Structure:

| Block | Role |
|--------|------|
| Nav | Brand omegaG, anchors Lightbar / Modifier layer / Specs, GitHub, “formerly DS4CC” |
| Hero | `hero-journey.png` omegaG badge + tagline + GitHub CTA |
| Lightbar | DualSense SVG demo + state chips (**illustration**, not HID) |
| Modifier layer | [01]–[03] PS exclusive semantics (Windows Codex runtime) |
| Device | `masterpiece.png` + etch lines |
| Specs | Platform / controllers / feedback / mapping / detection / Codex / config / stack + default maps |
| Footer | Fork attribution, MIT, **Upstream DS4CC releases** |

Stack: static HTML + CSS + small vanilla JS. Optional local tooling under `website/tools/` (Puppeteer screenshots; `node_modules` gitignored). Hosted today on **kimi.page** (Cloudflare; live injects `https://www.kimi.com/sdk-seed.js`).

---

## 2. Live vs `website/` vs code (summary)

| Source | Verdict |
|--------|---------|
| **Live `ds4cc-proto.kimi.page`** | **Still stale (P0 deploy).** Overstates Codex/lightbar as general product; Windows-centric hero strip; incomplete config/detection; footer “Releases” → upstream DS4CC without “upstream”; Kimi SDK. Snapshot: `~/Projects/omegag-site/index.fetched.html`. |
| **In-repo `website/`** | **Accuracy source of truth.** Codex/lightbar Windows-only; illustration disclaimer; XDG config; DualSense vs DS4 touchpad; detection WSL vs native; Share/Options unmapped; binary name `ds4cc` called out; hero badge rebranded to omegaG (not Claude). Guarded by `checks.mjs`. |
| **README / SPEC / src** | Align with **`website/`**, not live. Package/binary/config dir remain **`ds4cc`**. Codex + status→lightbar **Windows-only**. Six slots, 350 ms activate/PTT latch, 500 ms pulse, brightness, 180 s sleep in `codex_micro` / README. |

**Primary remaining P0 for users:** redeploy live from `website/`. Content under `website/` is publish-ready.

---

## 3. Claim accuracy matrix

Legend: **OK** · **LIVE-BAD** (live only) · **SOFT** · **GAP**

| Claim | Live | `website/` | Code / docs |
|-------|------|------------|-------------|
| Product name omegaG, formerly DS4CC | OK | OK | OK |
| Package / binary `ds4cc` preserved | SOFT | **OK** (hero, stack, footer) | OK |
| Config `%APPDATA%\ds4cc` + XDG/`~/.config/ds4cc` | LIVE-BAD (Linux incomplete) | OK | OK |
| DualSense + DS4, USB + BT | OK | OK | OK |
| Windows tray + Linux Ubuntu/Arch, evdev/uinput | Partial | OK | OK |
| Shortcut mapper defaults | Partial | OK | OK |
| Six slots + PS modifier | LIVE-BAD as always-on | OK as Windows Codex | OK |
| Status lightbar projection + pulse/sleep | LIVE-BAD as universal | OK Windows-only + demo label | OK |
| Static lightbar / mic LED | Buried | OK Feedback row | OK |
| L2 always PTT | LIVE-BAD | OK when voice configured | OK |
| Detection WSL-only | LIVE-BAD | OK WSL + native Linux | OK |
| Codex optional, Windows-only, disabled default | LIVE-BAD (no OS gate) | OK | OK |
| Upstream releases labeled | LIVE-BAD (“Releases”) | OK “Upstream DS4CC releases” | README Quick Start |
| Launcher (`launcher:…`) | GAP | GAP | HIGHLIGHTS.md |
| Linux install CTA | GAP | GAP | README |

---

## 4. Links

| Link | Status (2026-08-01) |
|------|---------------------|
| https://github.com/vgpnk-holdings-llc/omegaG | 200 |
| https://github.com/VeigaPunk/DS4CC/releases/latest | 302 → v3.1.0 |
| Local `website/` assets | Present; checks validate `src`/`href` |
| Live `hero-journey.png` | Present (old badge art may still say CLAUDE on live CDN until redeploy) |
| Kimi `sdk-seed.js` | Live-only host inject |

**og:image** remains relative (`assets/hero-journey.png`) — social previews need absolute URL after stable origin (P1).

---

## 5. Brand (omegaG vs DS4CC)

| Surface | Practice | Assessment |
|---------|----------|------------|
| Title / H1 / nav | **omegaG** | Good |
| “formerly DS4CC” | Nav, hero, etch | Good |
| Package/binary | **`ds4cc`** named on page | Good (post-fix) |
| Host | **`ds4cc-proto.kimi.page`** | Legacy/proto name |
| Hero art | omegaG / DS4·5 / Vibe city badge (repo) | Good after rebrand commit |
| Voice etch | “Keyboard optional when configured” | Safer than live “No keyboard required” |

---

## 6. kimi.page deploy notes

1. **Publish source of truth:** `website/index.html`, `style.css`, `main.js`, `assets/*` only.  
2. **Do not upload** `website/tools/node_modules` (gitignored) or unnecessary tooling to the public host. `tools/` screenshot harness is local-only.  
3. Host re-injects Kimi SDK; keep repo free of that script unless product wants it.  
4. After deploy: hard-refresh; grep live HTML for `Windows-only` and `Upstream DS4CC`; confirm hero badge is not CLAUDE.  
5. Cache: `max-age=60,must-revalidate` — allow a minute for edge refresh.  
6. Plan rename off `ds4cc-proto` when leaving prototype (P1).

---

## 7. Prioritized fix list

### P0 — correctness / user harm

| ID | Item | Status |
|----|------|--------|
| P0.1 | Redeploy live from `website/` | **Open** (host action) |
| P0.2 | Gate publish on `node website/checks.mjs` | Process; checks **pass** |
| P0.3 | Footer never bare “Releases” for upstream | **Fixed in `website/`**; live still wrong until P0.1 |
| P0.4 | Codex / status lightbar marked Windows-only + illustration | **Fixed in `website/`** |
| P0.5 | Binary/package name `ds4cc` on page | **Fixed** hero + stack + footer + checks |
| P0.6 | Hero badge not CLAUDE branding | **Fixed** (`hero-journey.png` regen + checks) |

### P1 — polish

| ID | Item |
|----|------|
| P1.1 | Absolute `og:image` / canonical URL |
| P1.2 | Explicit Windows installer + Linux install CTAs |
| P1.3 | Rename host off `ds4cc-proto` |
| P1.4 | Compress `masterpiece.png` (~1.4 MB) |
| P1.5 | Keep `website/tools/node_modules` out of deploys |

### P2 — nice-to-have

| ID | Item |
|----|------|
| P2.1 | Market launcher actions (HIGHLIGHTS) |
| P2.2 | Self-host or document Inter/mono fonts |
| P2.3 | Privacy note if Kimi SDK stays on production |
| P2.4 | Optional docs link (README/SPEC) |

---

## 8. In-repo changes (this audit line)

| Change | Detail |
|--------|--------|
| Accuracy copy | Already on tree before audit (Windows-only Codex/lightbar, maps, config) |
| Hero rebrand | `01cda4b` — omegaG badge `hero-journey.png`, og:image, CSS hero-art, Puppeteer tools |
| Binary naming | Hero attribution, Stack row, footer; asserts in `checks.mjs` |
| Audit doc | This file |

---

## 9. Actions log

| Action | Status |
|--------|--------|
| `WEBSITE-AUDIT.md` present and refreshed | Done |
| Cross-check README/SPEC/src | Done |
| `node website/checks.mjs` | pass |
| P0 site fixes under `website/` | Done (content + binary name + checks) |
| Commit on `website-audit` | Done this pass |
| Push `origin/website-audit` | Attempted; no force-push |
| Live redeploy | **Not done** (no kimi publish credentials in this workflow) |

---

## 10. Next command (human)

```bash
node website/checks.mjs
# Upload website/{index.html,style.css,main.js,assets/*} to kimi.page
# Verify live: Windows-only + Upstream DS4CC + omegaG hero (not CLAUDE)
```

**Bottom line:** `website/` is accurate and check-gated. **Ship it to kimi.page** to close P0 for real users.
