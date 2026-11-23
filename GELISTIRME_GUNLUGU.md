# App Fabrikası Ofisi — Geliştirme ve Sürüm Günlüğü

> **Bu dosya ne içindir?**
> Bu projeye Ubuntu üzerinden yeni bir özellik eklendiğinde veya hata düzeltildiğinde, çalışan yapay zeka ajanının **tam olarak hangi adımları izleyeceğini** bilmesi ve yapılan her değişikliğin kısa özetini kaydetmesi için tutulur.

---

## 1. Ajan İçin Standart Geliştirme & Yayın Akışı

Kullanıcı bu uygulamaya yeni bir özellik, arayüz iyileştirmesi veya hata düzeltmesi istediğinde şu sırayı izle:

### A. Geliştirme ve Yerel Doğrulama
1. İlgili dosyaları düzenle (`src/`, `src-tauri/src/`, `server/kopru.py`).
2. Frontend ve köprü doğrulamasını çalıştır:
   ```bash
   npm run build
   python3 -m py_compile server/kopru.py
   ```
3. Ubuntu'daki yerel masaüstü uygulamasını (`./baslat.sh`) güncellemek için release binary'sini derle:
   ```bash
   npm run tauri build -- --no-bundle
   ```

### B. Yeni Sürüm Yayınlama (Windows & Linux Otomatik Güncelleme)
Değişikliklerin Windows ve diğer cihazlara **tek tıkla güncelleme** (`⬆️ vX.Y.Z Güncelle`) olarak gitmesi için:
1. Sürüm numarasını şu **3 dosyada aynı anda** artır (örn. `1.0.0` → `1.0.1` veya yeni özellikse `1.1.0`):
   - `package.json` → `"version"`
   - `src-tauri/tauri.conf.json` → `"version"`
   - `src-tauri/Cargo.toml` → `version`
2. Aşağıdaki **Sürüm ve Değişiklik Günlüğü** tablosuna/listesine tarih, sürüm ve yapılanların kısa özetini ekle.
3. Commit at, push et ve sürüm etiketini gönder:
   ```bash
   git add -A
   git commit -m "vX.Y.Z: kısa açıklama"
   git push origin main
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```
4. `.github/workflows/release.yml` otomatik olarak hem **Linux** hem **Windows** paketlerini imzalayıp `latest.json` ile birlikte GitHub Releases'a yükler. Kurulu uygulamalar bir sonraki açılışta (3 sn içinde) veya **Ayarlar → Güncellemeleri Kontrol Et** butonunda yeni sürümü görür.

### C. Kritik Kurallar
- **Public repo güvenliği:** Bu depo **herkese açıktır**. Asla Bearer Token, API anahtarı, VDS/Tailscale IP adresi, `~/.tauri/*.key` özel anahtarı veya `VDS_KURULUM_PROMPT.md` dosyasını commit etme.
- **Köprü değişikliği uyarısı:** `server/kopru.py` dosyasında değişiklik yaptıysan, VDS'de çalışan `~/.local/bin/kopru.py` dosyasının da güncellenip `app-fabrikasi-kopru.service` servisinin yeniden başlatılması gerektiğini kullanıcıya mutlaka bildir.
- **İmza anahtarı:** Tauri güncelleme imzaları GitHub Actions `TAURI_SIGNING_PRIVATE_KEY` sırrıyla otomatik yapılır; yerel makinede özel anahtar `~/.tauri/app-fabrikasi-ofis.key` konumundadır (repoya eklenmez).

---

## 2. Hızlı Dosya Haritası

| Konum | Görev |
|---|---|
| `src/main.ts` | Üst bar, sekmeler, polling döngüsü, kısayollar |
| `src/updater.ts` | Otomatik güncelleme kontrolü (`tauri-plugin-updater`) ve yeniden başlatma |
| `src/views/office.ts` | 9 ajan terminali, odak modu, sohbet + dikte |
| `src/views/graph.ts` | Fabrika Hafızası 2D kuvvet-tabanlı graf ve not okuyucu |
| `src/views/project.ts` | Aktif proje görev ve aşama görünümü |
| `src/views/archive.ts` | Canlı arşiv ve yönetici günlüğü |
| `src/views/settings.ts` | Bağlantı profilleri (Yerel / VDS), dikte, tema ve güncelleme kontrolü |
| `src-tauri/src/lib.rs` | Tauri IPC komutları, eklenti kaydı, bellek ölçümü |
| `src-tauri/src/remote.rs` | VDS köprü HTTP istemcisi |
| `src-tauri/src/local.rs` | Yerel klasör okuyucu |
| `server/kopru.py` | VDS üzerinde çalışan sıfır bağımlılıklı Python HTTP köprüsü |
| `.github/workflows/release.yml` | Etiket (`v*`) push edildiğinde Linux + Windows imzalı release derlemesi |

---

## 3. Sürüm ve Değişiklik Günlüğü

### `v1.1.0` — 2026-11-23
- **Windows otomatik masaüstü kısayolu:** NSIS kurulumuna (`src-tauri/nsis/hooks.nsh`) kurulum sonrası otomatik masaüstü kısayolu oluşturma ve kaldırma sırasında temizleme eklendi (WiX/MSI ile birlikte her iki Windows kurucusunda masaüstü simgesi oluşur).
- **Ubuntu / Linux masaüstü simgesi ve menü entegrasyonu:** `.deb` paketi için özel `.desktop` şablonu (`src-tauri/linux/app-fabrikasi-ofis.desktop.hbs`) ile `postinst.sh` / `postrm.sh` betikleri eklendi; Ubuntu'da kurulumda masaüstü simgesi otomatik oluşturulur ve uygulama menüsüne eklenir.
- **Dahili WebKitGTK bellek optimizasyonu:** `src-tauri/src/main.rs` içinde Linux açılışında `WEBKIT_DISABLE_DMABUF_RENDERER=1` otomatik ayarlanır (`baslat.sh` bağımlılığı kaldırıldı).

### `v1.0.0` — 2026-11-23
- İlk kararlı sürüm GitHub'a (`tibetmdk/app-fabrikasi-ofis`) yayınlandı (Linux + Windows).
- **Otomatik güncelleme** (`tauri-plugin-updater` + `tauri-plugin-process`) eklendi: açılışta kontrol, üst barda tek tıkla güncelleme butonu, Ayarlar sayfasında elle kontrol butonu.
- **Hafıza grafı fizik düzeltmesi** (`src/views/graph.ts`): açılışta düğümlerin sağa sola fırlaması 180 adımlık ön-oturtma ve hız sınırı (`maxV = 18`) ile giderildi.
- **Köprü güvenliği** (`server/kopru.py`): `/api/note` uç noktasında `/` veya `\` ile başlayan yollar ve `hafiza/` dışına çıkan symlink/yollar engellendi.
