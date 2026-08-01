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

### Org github.io — one-time enable (no token / no API)

Accurate static files are **already on branch `gh-pages`** (CI `deploy-gh-pages-branch`).

1. Open https://github.com/vgpnk-holdings-llc/omegaG/settings/pages  
2. **Build and deployment → Source → Deploy from a branch**  
3. Branch: **`gh-pages`** · folder: **`/`** (root) · **Save**  
4. Wait ~1–2 min → https://vgpnk-holdings-llc.github.io/omegaG/ should 200  

Do **not** wait for tokens, PAT rotation, or Actions “github-pages” environment bootstrap. Branch deploy is sufficient.

Verify branch content anytime:

```bash
curl -sL https://raw.githubusercontent.com/vgpnk-holdings-llc/omegaG/gh-pages/index.html | head
# or after local checks:
bash website/publish-gh-pages.sh   # force-sync website/ → origin/gh-pages via SSH
```

Primary public URL remains https://veigapunk.github.io/omegag-site/ until org Pages is toggled (optional). Canonical/OG already point at the VeigaPunk mirror.

---

## C. Local preview

```bash
cd website && python3 -m http.server 8765
# open http://127.0.0.1:8765/
```
