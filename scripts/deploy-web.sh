#!/usr/bin/env bash
# Publish the browser build to autocaded.yemelianov.dev: the `autocaded`
# Worker serves web/ as static assets (wrangler.jsonc), with AutoCADED's own
# demo assets.
#
#   scripts/deploy-web.sh        # needs `npx wrangler login`
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
"${REPO_ROOT}/scripts/build-wasm.sh" --assets demo

cd "${REPO_ROOT}"
npx --yes wrangler deploy
