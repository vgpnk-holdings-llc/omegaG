#!/usr/bin/env bash
# Run accuracy checks, then publish to org gh-pages + live omegag-site mirror.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
bash website/publish-gh-pages.sh
bash website/publish-omegag-site.sh
bash website/verify-live.sh
echo "publish-all: done"
