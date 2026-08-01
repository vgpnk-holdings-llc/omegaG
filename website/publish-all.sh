#!/usr/bin/env bash
# Run accuracy checks, then publish to org gh-pages + live omegag-site mirror.
# Always run from a clean tree after commits so deploys match master website/.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
bash website/publish-gh-pages.sh
bash website/publish-omegag-site.sh
# Brief edge settle for github.io / raw CDN
sleep 2
bash website/verify-live.sh
echo "publish-all: done (HEAD=$(git rev-parse --short HEAD))"
