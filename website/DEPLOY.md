# Deploy the omegaG marketing site

Source of truth: this directory (`website/`).

## Status (2026-08-01) — audit HALT

| Surface | Status |
|---------|--------|
| **https://veigapunk.github.io/omegag-site/** | **LIVE · accurate** ([VeigaPunk/omegag-site](https://github.com/VeigaPunk/omegag-site)) |
| https://veigapunk.github.io/ | **Plazir** user Pages (separate product) — do not overwrite |
| Git `website/` on `master` | Source of truth; checks pass |
| Org `gh-pages` branch | Accurate tree ready |
| https://vgpnk-holdings-llc.github.io/omegaG/ | **404** — enable org Actions Pages env (admin) |
| https://ds4cc-proto.kimi.page/ | Stale (optional) |
| Release zip | tag `website-static` |

Canonical / OG in `index.html` point at `veigapunk.github.io/omegag-site/`.

## Pre-flight

```bash
node website/checks.mjs   # must print: website checks: pass
```

Upload **only**:

- `index.html`
- `style.css`
- `main.js`
- `assets/*`

Do **not** upload `website/tools/`, `node_modules`, or repo docs.

### Local package for kimi / SFTP

```bash
mkdir -p dist/website-static
cp website/index.html website/style.css website/main.js dist/website-static/
cp -a website/assets dist/website-static/assets
(cd dist && tar -czf omegaG-website-static.tgz website-static)
# → dist/omegaG-website-static.tgz  (gitignored)
```

### Rolling GitHub Release (CI)

Workflow: `.github/workflows/website-static-release.yml`

- Tag: **`website-static`** (prerelease, replaced each website change)
- Asset: `omegaG-website-static.zip`
- URL pattern: https://github.com/vgpnk-holdings-llc/omegaG/releases/tag/website-static

Download → unzip → upload contents to kimi.page.

### Docker (any host with a container runtime)

```bash
docker build -t omegag-website -f website/Dockerfile website
docker run --rm -p 8080:80 omegag-website
# open http://127.0.0.1:8080/
```

### DigitalOcean App Platform (attempted)

DO API token works, but create-app returns **`GitHub user not authenticated`**. Link GitHub under DO → Apps → Create → GitHub, then a static site from branch `gh-pages` will deploy. Not automated from this workspace.

---

## A. kimi.page (current public URL)

**URL:** https://ds4cc-proto.kimi.page/

**Blocker (2026-08-01):** no Kimi publish credentials or API in this workspace. Redeploy requires a human (or token) in the Kimi project UI:

1. Open the project that owns `ds4cc-proto.kimi.page`.
2. Replace site files with the tree above from git `website/`.
3. Publish / rebuild.
4. Hard-refresh; confirm HTML contains:
   - `Windows-only lightbar feedback`
   - `Upstream DS4CC releases`
   - `Package and binary name remain`
   - hero asset is omegaG badge (not CLAUDE)

`og:url` / `og:image` / `canonical` currently point at this host so social cards work after redeploy.

---

## B. GitHub Pages

Workflow: `.github/workflows/pages-website.yml`

| Job | Role |
|-----|------|
| `check` | `node website/checks.mjs` |
| `build` | Stage `_site` + upload artifact `website-static` |
| `deploy-pages` | Official Actions Pages (`continue-on-error`) |
| `deploy-gh-pages-branch` | **Always** publish to branch `gh-pages` |

### One-time admin (unblocks github.io) — ~30 seconds

1. Open https://github.com/vgpnk-holdings-llc/omegaG/settings/pages  
2. **Build and deployment → Source:** either  
   - **Deploy from a branch** → Branch `gh-pages` / `/ (root)` → Save, **or**  
   - **GitHub Actions** (then re-run workflow `Pages (website)`)  
3. Wait 1–2 minutes → https://vgpnk-holdings-llc.github.io/omegaG/ should 200.

`GITHUB_TOKEN` cannot create the Pages site for this org (`403 Resource not accessible by integration`). Branch content is already correct; only the Pages switch is missing.

When Pages is the primary public host, update absolute `og:*` and `canonical` in `index.html` to that origin and re-run `node website/checks.mjs` (checks currently pin the kimi.page origin).

---

## C. Local preview

```bash
cd website && python3 -m http.server 8765
# open http://127.0.0.1:8765/
```

## B. GitHub Pages via `gh-pages` branch (fallback)

CI may push static files to branch `gh-pages` when the Pages REST API is unavailable.

**Human enable once:**

1. Repo **Settings → Pages**
2. **Build and deployment → Source → Deploy from a branch**
3. Branch: **`gh-pages`** / folder: **`/`** (root)
4. Save → site at `https://vgpnk-holdings-llc.github.io/omegaG/`

Verify content before enable:

```bash
curl -sI https://raw.githubusercontent.com/vgpnk-holdings-llc/omegaG/gh-pages/index.html
```
