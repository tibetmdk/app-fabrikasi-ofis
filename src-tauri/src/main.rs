#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    {
        // WebKitGTK bellek optimizasyonları (alt WebKit süreçleri tarafından da devralınır)
        for (k, v) in [
            ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
            ("MALLOC_ARENA_MAX", "2"),
            ("MALLOC_MMAP_THRESHOLD_", "131072"),
            ("MALLOC_TRIM_THRESHOLD_", "131072"),
        ] {
            if std::env::var_os(k).is_none() {
                std::env::set_var(k, v);
            }
        }
    }

    app_fabrikasi_ofis_lib::run()
}
