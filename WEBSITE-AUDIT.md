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
| P0.1 | Accurate public HTML live | **DONE** via https://veigapunk.github.io/omegag-site/ |
| P0.2 | Gate publish on `node website/checks.mjs` | **pass** (CI + local) |
| P0.3 | Footer never bare “Releases” for upstream | **Fixed** (mirror + source) |
| P0.4 | Codex / status lightbar Windows-only + illustration | **Fixed** |
| P0.5 | Binary/package name `ds4cc` on page | **Fixed** |
| P0.6 | Hero badge not CLAUDE branding | **Fixed** |

### P1 — polish

| ID | Item | Status |
|----|------|--------|
| P1.1 | Absolute `og:image` / canonical URL | **Done** → `veigapunk.github.io/omegag-site` |
| P1.2 | Windows installer + Linux install CTAs | **Done** |
| P1.3 | Rename host off `ds4cc-proto` (kimi) | Open (optional; kimi not accuracy gate) |
| P1.4 | Compress `masterpiece.png` | **Done** |
| P1.5 | Keep `website/tools/` out of deploys | **Done** |
| P1.6 | GitHub Pages workflows | **Done** (org: Settings→Pages→`gh-pages` / once) |

### P2 — nice-to-have (post-HALT)

| ID | Item |
|----|------|
| P2.1 | Market launcher actions (HIGHLIGHTS) | **Done** — Specs Mapping row |
| P2.2 | Self-host Inter/mono fonts |
| P2.3 | Privacy note if Kimi SDK stays |
| P2.4 | Optional docs link (README/SPEC) |
| P2.5 | Auto-sync `website/` → VeigaPunk/omegag-site on master push |
| P2.6 | Org github.io via Settings→Pages→branch `gh-pages` / | External one-click |

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
| **Public accurate site** | **https://veigapunk.github.io/omegag-site/** LIVE |
| Mirror repo | https://github.com/VeigaPunk/omegag-site (synced from `website/`) |
| Live redeploy (kimi.page) | Still stale — optional; zip at `website-static` release |
| Org GitHub Pages | Content on `gh-pages`; enable **Deploy from branch** once (no token) |
| **Release `website-static`** | Zip for kimi/SFTP |
| **Docker** | `website/Dockerfile` |
| DigitalOcean Apps | Needs GitHub OAuth in DO console |
| GitHub PAT “GitHub CLI” in 1Password | Invalid (401) — rotate |

---

## 10. Host map (do not confuse)

| URL | What it is |
|-----|------------|
| https://veigapunk.github.io/ | **Plazir** user site (unrelated fan codex) — LIVE 200 |
| https://veigapunk.github.io/omegag-site/ | **omegaG marketing** — LIVE accurate 200 |
| https://vgpnk-holdings-llc.github.io/omegaG/ | Org project Pages — **LIVE** (branch `gh-pages` /) |
| https://ds4cc-proto.kimi.page/ | Proto marketing — **stale** (optional refresh) |
| `gh-pages` branch on org repo | **Accurate content already published** |

### Org Pages enable (once — no token)

**Settings → Pages → Deploy from a branch → `gh-pages` / `/` (root) → Save.**

Content is already on `gh-pages`. Do not wait for PAT, Actions environment, or API create-site (403).

## 11. HALT

**Audit mission: HALT.**

| Axis | Status |
|------|--------|
| Accuracy vs code/README/SPEC | Done (`website/` + checks) |
| Public accurate host | Done — https://veigapunk.github.io/omegag-site/ |
| Org `gh-pages` content | Done (CI) |
| Ship-quality residuals in `website/` | **None** — no further code work this mission |
| Org github.io | External toggle only (above) |
| kimi.page | Optional; not the accuracy gate |
| Plazir apex `veigapunk.github.io/` | Separate product — **do not overwrite** |

P2 items (launcher marketing, fonts, auto-sync mirror) are out of scope for this audit unless product reopens.

```bash
node website/checks.mjs
bash website/verify-live.sh
```

**Bottom line:** Accurate omegaG site is live on **two** hosts (VeigaPunk mirror + org Pages). Release zip in sync. kimi optional. **HALT** on accuracy mission; polish (Docs footer, robots, theme-color) may continue without reopening P0.
