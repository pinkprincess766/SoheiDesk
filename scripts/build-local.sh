#!/usr/bin/env bash
# Build SoheiDesk for the CURRENT operating system only.
# Windows/Linux installers cannot be produced on macOS without CI or a VM.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> Install deps"
pnpm install

OS="$(uname -s)"
case "$OS" in
  Darwin)
    echo "==> macOS build"
    # Prefer universal if Intel target is installed
    if rustup target list --installed | grep -q x86_64-apple-darwin; then
      echo "    universal (arm64 + x86_64)"
      pnpm tauri build --target universal-apple-darwin
      OUT="src-tauri/target/universal-apple-darwin/release/bundle"
    else
      pnpm tauri build
      OUT="src-tauri/target/release/bundle"
    fi
    echo ""
    echo "Artifacts:"
    find "$OUT" -type f \( -name '*.dmg' -o -name '*.app' \) 2>/dev/null || true
    ls -la "$OUT"/dmg 2>/dev/null || true
    ls -la "$OUT"/macos 2>/dev/null || true
    ;;
  Linux)
    echo "==> Linux build"
    pnpm tauri build
    find src-tauri/target/release/bundle -type f \( -name '*.AppImage' -o -name '*.deb' \) -ls
    ;;
  MINGW*|MSYS*|CYGWIN*|Windows_NT)
    echo "==> Windows build"
    pnpm tauri build
    find src-tauri/target/release/bundle -type f \( -name '*.msi' -o -name '*.exe' \) -ls
    ;;
  *)
    echo "Unknown OS: $OS"
    exit 1
    ;;
esac

echo ""
echo "Done. For all three OS at once, use GitHub Actions:"
echo "  .github/workflows/release.yml → Run workflow → download Artifacts"
