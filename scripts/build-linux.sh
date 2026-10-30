#!/usr/bin/env bash
# Build de release para Linux (.deb + .AppImage + binario). Salida: src-tauri/target/release/bundle
set -euo pipefail
cd "$(dirname "$0")/.."
[ -d node_modules ] || pnpm install --frozen-lockfile
# AppImage necesita esto en algunas distros recientes
export NO_STRIP=true
pnpm tauri build "$@"
echo
echo "Artefactos:"
find src-tauri/target/release/bundle -maxdepth 2 \( -name '*.deb' -o -name '*.AppImage' -o -name '*.rpm' \) 2>/dev/null
echo "Binario: src-tauri/target/release/mibodega"
