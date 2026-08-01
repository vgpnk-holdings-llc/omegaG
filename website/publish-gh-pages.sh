#!/usr/bin/env bash
# Local-first: publish website/ → origin branch gh-pages (no PAT).
# Org github.io then needs: Settings → Pages → Deploy from branch gh-pages / (root)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
node website/checks.mjs

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
cp website/index.html website/style.css website/main.js "$STAGE/"
cp -a website/assets "$STAGE/assets"
{
  git rev-parse HEAD
  date -u +%Y-%m-%dT%H:%M:%SZ
} >"$STAGE/.build-id"

WORK="$(mktemp -d)"
trap 'rm -rf "$STAGE" "$WORK"' EXIT
git clone --depth 1 -b gh-pages "git@github.com:vgpnk-holdings-llc/omegaG.git" "$WORK/repo" \
  || git clone --depth 1 "git@github.com:vgpnk-holdings-llc/omegaG.git" "$WORK/repo"

cd "$WORK/repo"
git checkout gh-pages 2>/dev/null || git checkout -B gh-pages
find . -mindepth 1 -maxdepth 1 ! -name '.git' -exec rm -rf {} +
cp -a "$STAGE"/. .
git add -A
if git diff --cached --quiet; then
  echo "gh-pages already up to date"
  exit 0
fi
git -c user.email="deploy@omegag.local" -c user.name="omegaG-deploy" \
  commit -m "deploy(website): $(cd "$ROOT" && git rev-parse --short HEAD)"
git push origin HEAD:gh-pages
echo "pushed gh-pages — enable Pages: Settings → Pages → branch gh-pages /"
