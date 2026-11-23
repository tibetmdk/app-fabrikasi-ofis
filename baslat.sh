#!/bin/bash
# WebKitGTK bellek optimizasyonları (DMA-BUF önbelleği ve glibc arena şişmesini önler)
export WEBKIT_DISABLE_DMABUF_RENDERER=1
export MALLOC_ARENA_MAX=2
export MALLOC_MMAP_THRESHOLD_=131072
export MALLOC_TRIM_THRESHOLD_=131072
exec /home/tibet/Desktop/AppFabrikasiOfis/src-tauri/target/release/app-fabrikasi-ofis "$@"
