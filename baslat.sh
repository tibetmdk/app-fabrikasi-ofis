#!/bin/bash
# WebKitGTK bellek optimizasyonları (DMA-BUF önbelleği ve glibc arena şişmesini önler)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export WEBKIT_DISABLE_DMABUF_RENDERER=1
export MALLOC_ARENA_MAX=2
export MALLOC_MMAP_THRESHOLD_=131072
export MALLOC_TRIM_THRESHOLD_=131072

if [ -x "/usr/bin/app-fabrikasi-ofis" ]; then
  exec "/usr/bin/app-fabrikasi-ofis" "$@"
elif [ -x "$HOME/.local/bin/app-fabrikasi-ofis" ]; then
  exec "$HOME/.local/bin/app-fabrikasi-ofis" "$@"
elif [ -x "$SCRIPT_DIR/src-tauri/target/release/app-fabrikasi-ofis" ]; then
  exec "$SCRIPT_DIR/src-tauri/target/release/app-fabrikasi-ofis" "$@"
else
  echo "Hata: app-fabrikasi-ofis yürütülebilir dosyası bulunamadı." >&2
  echo "Kurmak için: npm run tauri build -- --bundles deb && sudo dpkg -i src-tauri/target/release/bundle/deb/*.deb" >&2
  exit 1
fi
