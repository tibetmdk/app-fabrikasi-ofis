import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { api, type InstallInfo } from "./api";

let pendingUpdate: Update | null = null;
let installing = false;
let cachedInstallInfo: InstallInfo | null = null;

async function getInstallInfoSafe(): Promise<InstallInfo | null> {
  if (cachedInstallInfo) return cachedInstallInfo;
  try {
    cachedInstallInfo = await api.getInstallInfo();
    return cachedInstallInfo;
  } catch {
    return null;
  }
}

export async function checkForUpdates(
  showToast: (msg: string, err?: boolean) => void,
  silent = true
): Promise<boolean> {
  try {
    const update = await check();
    const btn = document.querySelector("#btn-update") as HTMLButtonElement | null;
    if (update) {
      pendingUpdate = update;
      const info = await getInstallInfoSafe();
      if (btn) {
        btn.style.display = "inline-flex";
        btn.textContent = `⬆️ v${update.version} Güncelle`;
        btn.title =
          info && !info.safe_self_update && info.reason
            ? info.reason
            : `Yeni sürüm hazır (v${update.version}) — tıklayıp güncelle`;
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

  const info = await getInstallInfoSafe();
  if (info && !info.safe_self_update) {
    showToast(
      info.reason ||
        "Yerel tek-dosya sürümü doğrudan güncellenemez. Lütfen .deb paketiyle kurun.",
      true
    );
    return;
  }

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
