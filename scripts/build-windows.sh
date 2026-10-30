#!/usr/bin/env bash
# Cross-compila desde Linux a Windows (instalador NSIS .exe) con cargo-xwin.
# Requisitos: clang, lld, llvm-rc, nsis (makensis). Salida: src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis
set -euo pipefail
cd "$(dirname "$0")/.."

command -v makensis >/dev/null || { echo "Falta NSIS: sudo apt install nsis"; exit 1; }
for t in clang lld llvm-rc; do
  command -v "$t" >/dev/null || { echo "Falta $t: sudo apt install clang lld llvm"; exit 1; }
done
rustup target list --installed | grep -q x86_64-pc-windows-msvc || rustup target add x86_64-pc-windows-msvc
command -v cargo-xwin >/dev/null || cargo install --locked cargo-xwin

[ -d node_modules ] || pnpm install --frozen-lockfile
pnpm tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc "$@"
echo
find src-tauri/target/x86_64-pc-windows-msvc/release/bundle -name '*.exe' 2>/dev/null
