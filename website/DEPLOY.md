# Deploy the omegaG marketing site

Source of truth: this directory (`website/`).

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

## B. GitHub Pages (repo-hosted fallback)

Workflow: `.github/workflows/pages-website.yml`

- Deploys the `website/` folder from `master` (and `workflow_dispatch`).
- `configure-pages` uses **`enablement: true`** so the first successful Actions run can bootstrap a Pages site (avoids `Get Pages site failed … Not Found`).
- If the deploy job still fails, open **Settings → Pages → Source: GitHub Actions** once (org policy can block auto-enable).
- Expected URL: `https://vgpnk-holdings-llc.github.io/omegaG/`

**Observed 2026-08-01:**

| Run | Result |
|-----|--------|
| First | `configure-pages`: Pages site Not Found |
| After `enablement: true` | Create Pages site **403 Resource not accessible by integration** (GITHUB_TOKEN cannot bootstrap Pages for this org/repo) |
| Mitigation | Workflow also pushes `_site` → **`gh-pages` branch** via `peaceiris/actions-gh-pages`. Admin can set Pages source to branch `gh-pages` / root **or** GitHub Actions once. |

When Pages is the primary public host, update absolute `og:*` and `canonical` in `index.html` to that origin and re-run `node website/checks.mjs` (checks pin the kimi.page origin today).

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
