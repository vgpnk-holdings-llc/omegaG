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
# Primary public accurate host (VeigaPunk mirror)
check_url "veigapunk.github.io" "https://veigapunk.github.io/omegag-site/" || ec=1
check_url "gh-pages raw (org)" "https://raw.githubusercontent.com/vgpnk-holdings-llc/omegaG/gh-pages/index.html" || ec=1
# Legacy / optional — report but only kimi is soft-fail for now (may lag)
check_url "org github.io" "https://vgpnk-holdings-llc.github.io/omegaG/" || ec=1
check_url "kimi.page (may lag)" "https://ds4cc-proto.kimi.page/" || true
exit "$ec"
