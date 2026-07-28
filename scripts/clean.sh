#!/usr/bin/env bash
# Free disk space: rebuildable caches only (safe).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "→ cleaning Rust target…"
(cd src-tauri && cargo clean) || true

echo "→ removing dist…"
rm -rf dist dist-ssr

echo "→ optional: node_modules (pass --all)"
if [[ "${1:-}" == "--all" ]]; then
  rm -rf node_modules
  echo "  removed node_modules — run: pnpm install"
fi

echo "✓ done"
du -sh . src-tauri/target node_modules dist 2>/dev/null || true
