#!/usr/bin/env bash
# Run accuracy checks, then publish to org gh-pages + live omegag-site mirror.
# Always run from a clean tree after commits so deploys match master website/.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
bash website/publish-gh-pages.sh
bash website/publish-omegag-site.sh
# github.io edge can lag ~15–30s after push; retry verify-live
for i in 1 2 3 4 5 6; do
  if bash website/verify-live.sh; then
    echo "publish-all: done (HEAD=$(git rev-parse --short HEAD))"
    exit 0
  fi
  echo "verify-live retry $i after edge settle..."
  sleep 10
done
echo "publish-all: verify-live failed after retries" >&2
exit 1
