# App Fabrikası Ofisi — Masaüstü Komuta Merkezi (Linux & Windows)

[AppFabrikasi](../AppFabrikasi) ajan mimarisini **hiçbir koduna dokunmadan** hem yerel bilgisayarda hem de VDS sunucusunda anlık olarak izlemek, yöneticiyle dikte destekli konuşmak ve Fabrika Hafızası grafını incelemek için tasarlanmış düşük RAM tüketimli (**Tauri v2 + Rust + Vanilla TypeScript**) masaüstü uygulamasıdır.

## Özellikler
- **Düşük Bellek Tüketimi:** Electron yerine Tauri v2 (sistem WebView + Rust çekirdek) kullanır.
- **Çift Mod (Yerel & VDS):**
  - **Yerel Mod:** `AppFabrikasi` klasöründeki `durum.db`, `kayitlar/canli/*.log`, `aktif.json`, `config.json` ve `hafiza/` dosyalarını doğrudan okur.
  - **VDS Modu:** `server/kopru.py` sıfır-bağımlılık Python köprüsü üzerinden HTTP + Bearer Token ile bağlanır.
- **Güvenli Yaşam Döngüsü:** Masaüstü penceresini kapatmak arka plandaki ajanları/servisi durdurmaz. Uygulama içindeki kontrol panelinden **Ofisi Aç / Kapat**, **Duraklat / Devam Et**, **Araştır** komutlarını verebilirsiniz.
- **9 Ajan Terminali + Odak Modu:** Canlı durum, model/muadil göstergesi, yönetici bağlam çubuğu, tam ekran odak (`1–9`), canlı arama ve ikon filtresi (`❌`, `⚙️`, `📝`, `🧠`).
- **Yönetici Sohbeti + Dikte (Ses → Metin):** Linux (`pw-record`/`parecord`/`arecord`) ve Windows (`cpal` WASAPI) üzerinden mikrofon kaydı alıp **Groq Whisper** veya **OpenAI Whisper** ile Türkçe metne çevirir.
- **Gömülü Fabrika Hafızası Grafı:** `hafiza/.obsidian/graph.json` renk gruplarını birebir okuyan 60 FPS Canvas2D kuvvet-tabanlı graf + tıklanabilir Markdown not okuyucu ve `[[wikilink]]` gezintisi.
- **Responsive & Kişiselleştirilebilir:** Yarım pencerede sorunsuz çalışır. Ayarlar sayfasından tema, vurgu rengi, yazı boyutu, sütun sayısı, görünür paneller/ajanlar ve bağlantı profilleri düzenlenebilir.

## Geliştirme & Çalıştırma

```bash
npm install
npm run tauri dev
```

Üretim derlemesi (Linux AppImage / deb / binary):
```bash
npm run tauri build
```

## VDS Sunucusuna Taşıma (`server/kopru.py`)
`AppFabrikasi` koduna dokunmadan yalnızca `server/kopru.py` dosyasını VDS sunucusuna kopyalayıp çalıştırın:

```bash
python3 kopru.py --root /sunucu/AppFabrikasi --host 0.0.0.0 --port 8787 --token COK_GIZLI_TOKEN
```

Ardından uygulamada **Ayarlar → Bağlantı Profilleri → VDS Sunucu** sekmesine sunucu adresinizi ve token'ınızı girin. (Güvenlik için Tailscale, SSH tüneli `ssh -L 8787:127.0.0.1:8787 user@vds` veya HTTPS reverse proxy önerilir.)

## Windows & Linux Güncellemelerini Paylaşma
Depoyu GitHub'a yükleyip yeni bir sürüm etiketi (`git tag v1.0.1 && git push --tags`) attığınızda `.github/workflows/release.yml` otomatik olarak hem **Linux (AppImage/deb)** hem de **Windows (MSI/NSIS `.exe`)** kurulum dosyalarını oluşturup GitHub Releases altında yayınlar.
