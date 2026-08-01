#!/usr/bin/env bash
# Compare live marketing hosts against accuracy fingerprints from website/.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
need=(
  'Windows-only lightbar feedback'
  'Upstream DS4CC releases'
  'Package and binary name remain'
  'Windows installer (upstream)'
  'launcher:'
  'No default button is pre-wired to a launcher'
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

check_http() {
  local name="$1" url="$2"
  local code
  code="$(curl -s -o /dev/null -w "%{http_code}" --max-time 20 "$url" || echo 000)"
  if [[ "$code" == "200" ]]; then
    echo "PASS  $name  HTTP $code  $url"
    return 0
  fi
  echo "FAIL  $name  HTTP $code  $url"
  return 1
}

# Primary public accurate host (VeigaPunk mirror)
check_url "veigapunk.github.io" "https://veigapunk.github.io/omegag-site/" || ec=1
check_http "veigapunk robots.txt" "https://veigapunk.github.io/omegag-site/robots.txt" || ec=1
check_url "gh-pages raw (org)" "https://raw.githubusercontent.com/vgpnk-holdings-llc/omegaG/gh-pages/index.html" || ec=1
check_url "org github.io" "https://vgpnk-holdings-llc.github.io/omegaG/" || ec=1
check_http "org robots.txt" "https://vgpnk-holdings-llc.github.io/omegaG/robots.txt" || ec=1
# Soft-fail lagging proto host
check_url "kimi.page (may lag)" "https://ds4cc-proto.kimi.page/" || true
exit "$ec"
