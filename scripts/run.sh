#!/usr/bin/env bash
# Levanta la app en modo desarrollo (ventana Tauri con recarga en caliente).
set -euo pipefail
cd "$(dirname "$0")/.."
[ -d node_modules ] || pnpm install
# WebKitGTK cierra la ventana en algunos drivers GPU sin esto
export WEBKIT_DISABLE_DMABUF_RENDERER="${WEBKIT_DISABLE_DMABUF_RENDERER:-1}"
exec pnpm tauri dev
