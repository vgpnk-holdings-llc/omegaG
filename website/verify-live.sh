#!/usr/bin/env bash
# Compare live marketing hosts against accuracy fingerprints from website/.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
need=(
  'Windows-only lightbar feedback'
  'Upstream DS4CC releases'
  'Package and binary name remain'
  'Windows installer (upstream)'
)

check_url() {
  local name="$1" url="$2"
  local body
  body="$(curl -fsSL "$url" 2>/dev/null || true)"
  if [[ -z "$body" ]]; then
    echo "FAIL  $name  (fetch empty/error)  $url"
    return 1
  fi
  local miss=0
  for s in "${need[@]}"; do
    if ! grep -Fq "$s" <<<"$body"; then
      echo "  miss: $s"
      miss=1
    fi
  done
  if [[ "$miss" -eq 0 ]]; then
    echo "PASS  $name  $url"
    return 0
  fi
  echo "FAIL  $name  $url"
  return 1
}

echo "Fingerprints from $ROOT/website/index.html must appear on live hosts."
ec=0
check_url "kimi.page" "https://ds4cc-proto.kimi.page/" || ec=1
check_url "gh-pages raw" "https://raw.githubusercontent.com/vgpnk-holdings-llc/omegaG/gh-pages/index.html" || ec=1
check_url "github.io" "https://vgpnk-holdings-llc.github.io/omegaG/" || true
# github.io may 404 until Pages enabled — do not fail the script solely for that if raw passes
exit "$ec"
