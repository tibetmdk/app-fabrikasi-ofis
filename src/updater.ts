import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

let pendingUpdate: Update | null = null;
let installing = false;

export async function checkForUpdates(
  showToast: (msg: string, err?: boolean) => void,
  silent = true
): Promise<boolean> {
  try {
    const update = await check();
    const btn = document.querySelector("#btn-update") as HTMLButtonElement | null;
    if (update) {
      pendingUpdate = update;
      if (btn) {
        btn.style.display = "inline-flex";
        btn.textContent = `⬆️ v${update.version} Güncelle`;
        btn.title = `Yeni sürüm hazır (v${update.version}) — tıklayıp güncelle`;
      }
      showToast(`Yeni sürüm mevcut: v${update.version}`);
      return true;
    }
    if (!silent) {
      showToast("Uygulama güncel (en son sürüm yüklü).");
    }
    return false;
  } catch (err) {
    if (!silent) {
      showToast(`Güncelleme kontrolü başarısız: ${err}`, true);
    }
    return false;
  }
}

export async function installPendingUpdate(showToast: (msg: string, err?: boolean) => void) {
  if (!pendingUpdate || installing) return;
  const btn = document.querySelector("#btn-update") as HTMLButtonElement | null;
  installing = true;
  if (btn) {
    btn.disabled = true;
    btn.textContent = "⬇️ İndiriliyor...";
  }
  try {
    let downloaded = 0;
    let total = 0;
    await pendingUpdate.downloadAndInstall((event) => {
      if (event.event === "Started") {
        total = event.data.contentLength || 0;
      } else if (event.event === "Progress") {
        downloaded += event.data.chunkLength;
        if (btn && total > 0) {
          const pct = Math.min(100, Math.round((downloaded / total) * 100));
          btn.textContent = `⬇️ %${pct}`;
        }
      } else if (event.event === "Finished") {
        if (btn) btn.textContent = "🔄 Yeniden başlatılıyor...";
      }
    });
    showToast("Güncelleme kuruldu, uygulama yeniden başlatılıyor...");
    await relaunch();
  } catch (err) {
    installing = false;
    if (btn) {
      btn.disabled = false;
      btn.textContent = `⬆️ v${pendingUpdate.version} Güncelle`;
    }
    showToast(`Güncelleme kurulamadı: ${err}`, true);
  }
}
