#!/usr/bin/env bash
# Local-first: publish website/ → VeigaPunk/omegag-site (live accurate host).
# Injects <base href="/omegag-site/"> for project Pages asset paths.
# Does NOT touch veigapunk.github.io apex (Plazir).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
node website/checks.mjs

STAGE="$(mktemp -d)"
WORK="$(mktemp -d)"
trap 'rm -rf "$STAGE" "$WORK"' EXIT

cp website/index.html website/style.css website/main.js "$STAGE/"
cp -a website/assets "$STAGE/assets"
[[ -f website/robots.txt ]] && cp website/robots.txt "$STAGE/"
touch "$STAGE/.nojekyll"
{
  git rev-parse HEAD
  date -u +%Y-%m-%dT%H:%M:%SZ
} >"$STAGE/.build-id"

# Project Pages under /omegag-site/ need a base href for relative CSS/JS/assets.
python3 - "$STAGE/index.html" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
html = p.read_text()
if "<base " not in html:
    html = html.replace("<head>\n", '<head>\n  <base href="/omegag-site/">\n', 1)
p.write_text(html)
PY

git clone --depth 1 "git@github.com:VeigaPunk/omegag-site.git" "$WORK/repo"
cd "$WORK/repo"
# Preserve workflows; replace site assets only
for f in index.html style.css main.js robots.txt .nojekyll .build-id; do
  [[ -f "$STAGE/$f" ]] && cp "$STAGE/$f" .
done
rm -rf assets
cp -a "$STAGE/assets" assets
# keep .github/ — stage site paths only
git add index.html style.css main.js assets .nojekyll .build-id
[[ -f robots.txt ]] && git add robots.txt

if git diff --cached --quiet; then
  echo "omegag-site already up to date"
  exit 0
fi
git -c user.email="deploy@omegag.local" -c user.name="omegaG-deploy" \
  commit -m "deploy(website): $(cd "$ROOT" && git rev-parse --short HEAD)"
git push origin HEAD:main
# Keep gh-pages branch equal to main — some Pages configs deploy from branch, not Actions.
git push origin HEAD:gh-pages --force
echo "pushed https://veigapunk.github.io/omegag-site/ (main + gh-pages)"
