#!/bin/bash
# WebKitGTK bellek optimizasyonları (DMA-BUF önbelleği ve glibc arena şişmesini önler)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export WEBKIT_DISABLE_DMABUF_RENDERER=1
export MALLOC_ARENA_MAX=2
export MALLOC_MMAP_THRESHOLD_=131072
export MALLOC_TRIM_THRESHOLD_=131072
exec "$SCRIPT_DIR/src-tauri/target/release/app-fabrikasi-ofis" "$@"
