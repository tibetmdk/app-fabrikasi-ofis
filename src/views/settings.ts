import { api } from "../api";
import { AGENT_DEFS, type AppSettings, type ConnectionProfile } from "../types";
import { esc } from "../utils";
import { checkForUpdates } from "../updater";

export function renderSettingsView(
  container: HTMLElement,
  settings: AppSettings,
  onSave: (next: AppSettings) => Promise<void>,
  showToast: (msg: string, err?: boolean) => void
) {
  const draft: AppSettings = JSON.parse(JSON.stringify(settings));

  const render = () => {
    const activeProf =
      draft.profiles.find((p) => p.id === draft.active_profile_id) || draft.profiles[0];

    container.innerHTML = `
      <div class="page-scroll">
        <div class="page-container">
          <!-- Connection Profiles -->
          <div class="card">
            <div class="card-title">
              <span>🌐 Bağlantı Profilleri (Yerel & VDS)</span>
              <div style="display:flex;gap:6px;flex-wrap:wrap">
                <button class="btn sm" id="btn-export-conn" title="VDS bağlantı kodunu panoya kopyala (diğer cihaza tek tıkla aktarmak için)">📤 Kodu Kopyala</button>
                <button class="btn sm" id="btn-import-conn" title="Diğer cihazdan kopyalanan bağlantı kodunu yapıştır">📥 Kodu Yapıştır</button>
                <button class="btn sm" id="btn-add-profile">+ Yeni Profil</button>
                ${
                  draft.profiles.length > 1
                    ? `<button class="btn sm danger" id="btn-del-profile" title="Seçili profili sil">🗑️ Sil</button>`
                    : ""
                }
                <button class="btn sm" id="btn-test-conn">⚡ Bağlantıyı Test Et</button>
                <button class="btn sm primary" id="btn-save-profile">💾 Kaydet</button>
              </div>
            </div>
            <div style="display:flex;gap:6px;flex-wrap:wrap;margin-bottom:12px">
              ${draft.profiles
                .map(
                  (p) => `
                <button class="btn ${p.id === activeProf.id ? "primary" : ""}" data-prof="${esc(p.id)}">
                  ${p.mode === "local" ? "💻" : "☁️"} ${esc(p.name)}
                </button>`
                )
                .join("")}
            </div>
            <div class="form-grid">
              <div class="field">
                <label>Profil Adı</label>
                <input id="p-name" value="${esc(activeProf.name)}" />
              </div>
              <div class="field">
                <label>Bağlantı Modu</label>
                <select id="p-mode">
                  <option value="local" ${activeProf.mode === "local" ? "selected" : ""}>Yerel (Doğrudan Klasör / Salt Okuma)</option>
                  <option value="remote" ${activeProf.mode === "remote" ? "selected" : ""}>Uzak Sunucu (VDS Köprüsü HTTP)</option>
                </select>
              </div>
              ${
                activeProf.mode === "local"
                  ? `<div class="field" style="grid-column:1 / -1">
                      <label>AppFabrikasi Kök Klasörü</label>
                      <input id="p-root" value="${esc(activeProf.local_root)}" placeholder="/home/tibet/Desktop/AppFabrikasi" />
                    </div>`
                  : `<div class="field">
                      <label>VDS Köprü Adresi (URL)</label>
                      <input id="p-url" value="${esc(activeProf.remote_url)}" placeholder="http://vds-ip:8787 veya https://ofis.domain.com" />
                    </div>
                    <div class="field">
                      <label>Bearer Token (Güvenlik Anahtarı)</label>
                      <input id="p-token" type="password" value="${esc(activeProf.remote_token)}" placeholder="Sunucudaki --token değeri" />
                    </div>`
              }
            </div>
            <div style="margin-top:10px;padding:8px 10px;background:var(--bg-0);border:1px solid var(--border);border-radius:6px;font-size:11.5px;color:var(--text-2)">
              <strong>VDS'ye taşırken:</strong> <code>AppFabrikasiOfis/server/kopru.py</code> dosyasını VDS'ye kopyalayıp şu komutla çalıştırmanız yeterli (AppFabrikasi koduna hiç dokunmaz):<br/>
              <code style="color:var(--cyan)">python3 kopru.py --root /sunucu/AppFabrikasi --host 0.0.0.0 --port 8787 --token GIZLI_ANAHTAR</code>
            </div>
          </div>

          <!-- Dictation -->
          <div class="card">
            <div class="card-title">🎙️ Dikte (Ses → Metin) Ayarları</div>
            <div class="form-grid">
              <div class="field">
                <label>Sağlayıcı</label>
                <select id="d-provider">
                  <option value="groq" ${draft.dictation.provider === "groq" ? "selected" : ""}>Groq Whisper (Önerilen · Ultra Hızlı)</option>
                  <option value="openai" ${draft.dictation.provider === "openai" ? "selected" : ""}>OpenAI Whisper API</option>
                  <option value="custom" ${draft.dictation.provider === "custom" ? "selected" : ""}>Özel OpenAI-Uyumlu Endpoint</option>
                </select>
              </div>
              <div class="field">
                <label>API Anahtarı</label>
                <input id="d-key" type="password" value="${esc(draft.dictation.api_key)}" placeholder="gsk_... veya sk-..." />
              </div>
              <div class="field">
                <label>Model</label>
                <input id="d-model" value="${esc(draft.dictation.model)}" placeholder="whisper-large-v3-turbo" />
              </div>
              <div class="field">
                <label>Dil Kodu</label>
                <input id="d-lang" value="${esc(draft.dictation.language)}" placeholder="tr" />
              </div>
              ${
                draft.dictation.provider === "custom"
                  ? `<div class="field" style="grid-column:1 / -1">
                      <label>Özel Endpoint URL</label>
                      <input id="d-url" value="${esc(draft.dictation.custom_url)}" placeholder="https://.../v1/audio/transcriptions" />
                    </div>`
                  : ""
              }
            </div>
            <label class="check-item" style="margin-top:10px;width:fit-content">
              <input type="checkbox" id="d-auto" ${draft.dictation.auto_send ? "checked" : ""} />
              Dikte bitince metni beklemeden doğrudan Yöneticiye gönder
            </label>
          </div>

          <!-- Appearance & Layout Customization -->
          <div class="card">
            <div class="card-title">🎨 Arayüz, Tema ve Kişiselleştirme</div>
            <div class="form-grid">
              <div class="field">
                <label>Tema</label>
                <select id="u-theme">
                  <option value="obsidian" ${draft.ui.theme === "obsidian" ? "selected" : ""}>Obsidian Dark (Varsayılan)</option>
                  <option value="terminal" ${draft.ui.theme === "terminal" ? "selected" : ""}>Matrix Terminal</option>
                  <option value="midnight" ${draft.ui.theme === "midnight" ? "selected" : ""}>Midnight Blue</option>
                  <option value="slate" ${draft.ui.theme === "slate" ? "selected" : ""}>Neutral Slate</option>
                </select>
              </div>
              <div class="field">
                <label>Vurgu Rengi</label>
                <select id="u-accent">
                  <option value="emerald" ${draft.ui.accent === "emerald" ? "selected" : ""}>Zümrüt Yeşili</option>
                  <option value="cyan" ${draft.ui.accent === "cyan" ? "selected" : ""}>Siyan</option>
                  <option value="amber" ${draft.ui.accent === "amber" ? "selected" : ""}>Amber</option>
                  <option value="violet" ${draft.ui.accent === "violet" ? "selected" : ""}>Mor</option>
                </select>
              </div>
              <div class="field">
                <label>Terminal Yazı Boyutu (${draft.ui.font_size}px)</label>
                <input id="u-font" type="range" min="10" max="16" value="${draft.ui.font_size}" />
              </div>
              <div class="field">
                <label>Ajan Izgara Sütun Sayısı</label>
                <select id="u-cols">
                  <option value="auto" ${draft.ui.grid_columns === "auto" ? "selected" : ""}>Otomatik (Responsive)</option>
                  <option value="2" ${draft.ui.grid_columns === "2" ? "selected" : ""}>2 Sütun</option>
                  <option value="3" ${draft.ui.grid_columns === "3" ? "selected" : ""}>3 Sütun</option>
                </select>
              </div>
              <div class="field">
                <label>Yenileme Sıklığı (ms)</label>
                <input id="u-poll" type="number" min="400" max="10000" step="100" value="${draft.ui.poll_interval_ms}" />
              </div>
            </div>

            <div style="margin-top:12px;font-size:11.5px;font-weight:600;color:var(--text-1);margin-bottom:6px">Sol Panel Bileşenleri</div>
            <div class="check-grid">
              <label class="check-item"><input type="checkbox" id="u-events" ${draft.ui.show_events ? "checked" : ""}/> İş Akışı (Olaylar)</label>
              <label class="check-item"><input type="checkbox" id="u-chat" ${draft.ui.show_chat ? "checked" : ""}/> Yönetici Sohbeti</label>
              <label class="check-item"><input type="checkbox" id="u-sys" ${draft.ui.show_system ? "checked" : ""}/> Otomatik Kontroller</label>
              <label class="check-item"><input type="checkbox" id="u-notif" ${draft.ui.notifications_enabled ? "checked" : ""}/> Masaüstü Bildirimleri</label>
            </div>

            <div style="margin-top:12px;font-size:11.5px;font-weight:600;color:var(--text-1);margin-bottom:6px">Görünür Ajan Pencereleri</div>
            <div class="check-grid">
              ${AGENT_DEFS.map(
                (a) => `
                <label class="check-item">
                  <input type="checkbox" data-vis-agent="${a.id}" ${draft.ui.visible_agents.includes(a.id) ? "checked" : ""} />
                  <span>${a.key}· ${esc(a.short)}</span>
                </label>`
              ).join("")}
            </div>
          </div>

          <div style="display:flex;justify-content:space-between;align-items:center;gap:8px;padding-bottom:16px">
            <button class="btn" id="btn-check-update">🔄 Güncellemeleri Kontrol Et</button>
            <button class="btn primary" id="btn-save-settings">💾 Ayarları Kaydet ve Uygula</button>
          </div>
        </div>
      </div>
    `;

    // Profile switching
    container.querySelector("#btn-export-conn")?.addEventListener("click", async () => {
      syncDraft();
      const prof =
        draft.profiles.find((p) => p.id === draft.active_profile_id && p.mode === "remote") ||
        draft.profiles.find((p) => p.mode === "remote");
      if (!prof || !prof.remote_url.trim() || !prof.remote_token.trim()) {
        showToast("Dışa aktarılacak dolu bir VDS profili bulunamadı.", true);
        return;
      }
      const payload = JSON.stringify({
        name: prof.name,
        remote_url: prof.remote_url.trim(),
        remote_token: prof.remote_token.trim(),
      });
      const bytes = new TextEncoder().encode(payload);
      let bin = "";
      bytes.forEach((b) => (bin += String.fromCharCode(b)));
      const code = `af-ofis-vds:${btoa(bin)}`;
      try {
        await navigator.clipboard.writeText(code);
        showToast("Bağlantı kodu panoya kopyalandı. Diğer cihazda '📥 Kodu Yapıştır'a tıklayın.");
      } catch {
        window.prompt("Bağlantı kodunu kopyalayın:", code);
      }
    });

    container.querySelector("#btn-import-conn")?.addEventListener("click", async () => {
      const raw = window.prompt(
        "Diğer cihazdan kopyaladığınız bağlantı kodunu (af-ofis-vds:...) veya JSON'u yapıştırın:"
      );
      if (!raw || !raw.trim()) return;
      try {
        const trimmed = raw.trim();
        let jsonStr = trimmed;
        if (trimmed.startsWith("af-ofis-vds:")) {
          const b64 = trimmed.slice("af-ofis-vds:".length).trim();
          const bin = atob(b64);
          const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
          jsonStr = new TextDecoder().decode(bytes);
        }
        const parsed = JSON.parse(jsonStr);
        const url = String(parsed.remote_url || "").trim();
        const token = String(parsed.remote_token || "").trim();
        const name = String(parsed.name || "VDS Sunucu").trim() || "VDS Sunucu";
        if (!url || !token) {
          showToast("Geçersiz bağlantı kodu (URL veya token eksik).", true);
          return;
        }
        let vds = draft.profiles.find((p) => p.id === "vds");
        if (!vds) {
          vds = {
            id: "vds",
            name,
            mode: "remote",
            local_root: "",
            remote_url: url,
            remote_token: token,
          };
          draft.profiles.push(vds);
        } else {
          vds.name = name;
          vds.mode = "remote";
          vds.remote_url = url;
          vds.remote_token = token;
        }
        draft.active_profile_id = vds.id;
        await onSave(JSON.parse(JSON.stringify(draft)));
        showToast("VDS bağlantısı içe aktarıldı ve kaydedildi!");
        render();
      } catch {
        showToast("Bağlantı kodu çözülemedi. Kodu eksiksiz yapıştırdığınızdan emin olun.", true);
      }
    });

    container.querySelectorAll("[data-prof]").forEach((btn) => {
      btn.addEventListener("click", () => {
        syncDraft();
        draft.active_profile_id = btn.getAttribute("data-prof") || draft.active_profile_id;
        render();
      });
    });

    container.querySelector("#btn-add-profile")?.addEventListener("click", () => {
      syncDraft();
      const id = `vds-${Date.now()}`;
      const newProf: ConnectionProfile = {
        id,
        name: "Yeni VDS",
        mode: "remote",
        local_root: "",
        remote_url: "http://",
        remote_token: "",
      };
      draft.profiles.push(newProf);
      draft.active_profile_id = id;
      render();
    });

    container.querySelector("#p-mode")?.addEventListener("change", () => {
      syncDraft();
      render();
    });

    container.querySelector("#d-provider")?.addEventListener("change", () => {
      syncDraft();
      if (draft.dictation.provider === "groq") draft.dictation.model = "whisper-large-v3-turbo";
      else if (draft.dictation.provider === "openai") draft.dictation.model = "whisper-1";
      render();
    });

    container.querySelector("#btn-del-profile")?.addEventListener("click", async () => {
      if (draft.profiles.length <= 1) return;
      syncDraft();
      const idx = draft.profiles.findIndex((p) => p.id === draft.active_profile_id);
      if (idx === -1) return;
      const target = draft.profiles[idx];
      if (!window.confirm(`"${target.name}" profili silinsin mi?`)) return;
      draft.profiles.splice(idx, 1);
      draft.active_profile_id = draft.profiles[0].id;
      await onSave(JSON.parse(JSON.stringify(draft)));
      showToast(`"${target.name}" profili silindi.`);
      render();
    });

    container.querySelector("#btn-save-profile")?.addEventListener("click", async () => {
      syncDraft();
      await onSave(JSON.parse(JSON.stringify(draft)));
      showToast("Bağlantı profili kaydedildi ve uygulandı.");
    });

    container.querySelector("#btn-test-conn")?.addEventListener("click", async () => {
      syncDraft();
      const prof = draft.profiles.find((p) => p.id === draft.active_profile_id) || draft.profiles[0];
      const res = await api.testConnection(prof);
      if (res.ok) {
        await onSave(JSON.parse(JSON.stringify(draft)));
        showToast(`${res.message} · Profil otomatik kaydedildi.`);
      } else {
        showToast(res.message, true);
      }
    });

    container.querySelector("#btn-check-update")?.addEventListener("click", async () => {
      await checkForUpdates(showToast, false);
    });

    container.querySelector("#btn-save-settings")?.addEventListener("click", async () => {
      syncDraft();
      await onSave(JSON.parse(JSON.stringify(draft)));
      showToast("Ayarlar kaydedildi ve uygulandı.");
    });
  };

  const syncDraft = () => {
    const activeProf =
      draft.profiles.find((p) => p.id === draft.active_profile_id) || draft.profiles[0];
    const val = (id: string) => (container.querySelector(`#${id}`) as HTMLInputElement | null)?.value;
    const chk = (id: string) =>
      Boolean((container.querySelector(`#${id}`) as HTMLInputElement | null)?.checked);

    if (val("p-name") !== undefined) activeProf.name = val("p-name")!;
    if (val("p-mode") !== undefined) activeProf.mode = val("p-mode") as "local" | "remote";
    if (val("p-root") !== undefined) activeProf.local_root = val("p-root")!;
    if (val("p-url") !== undefined) activeProf.remote_url = val("p-url")!;
    if (val("p-token") !== undefined) activeProf.remote_token = val("p-token")!;

    if (val("d-provider") !== undefined)
      draft.dictation.provider = val("d-provider") as "groq" | "openai" | "custom";
    if (val("d-key") !== undefined) draft.dictation.api_key = val("d-key")!;
    if (val("d-model") !== undefined) draft.dictation.model = val("d-model")!;
    if (val("d-lang") !== undefined) draft.dictation.language = val("d-lang")!;
    if (val("d-url") !== undefined) draft.dictation.custom_url = val("d-url")!;
    draft.dictation.auto_send = chk("d-auto");

    if (val("u-theme") !== undefined) draft.ui.theme = val("u-theme") as AppSettings["ui"]["theme"];
    if (val("u-accent") !== undefined)
      draft.ui.accent = val("u-accent") as AppSettings["ui"]["accent"];
    if (val("u-font") !== undefined) draft.ui.font_size = Number(val("u-font")) || 12;
    if (val("u-cols") !== undefined)
      draft.ui.grid_columns = val("u-cols") as AppSettings["ui"]["grid_columns"];
    if (val("u-poll") !== undefined)
      draft.ui.poll_interval_ms = Math.max(400, Number(val("u-poll")) || 1000);

    draft.ui.show_events = chk("u-events");
    draft.ui.show_chat = chk("u-chat");
    draft.ui.show_system = chk("u-sys");
    draft.ui.notifications_enabled = chk("u-notif");

    const vis: string[] = [];
    container.querySelectorAll("[data-vis-agent]").forEach((el) => {
      const input = el as HTMLInputElement;
      if (input.checked) vis.push(input.getAttribute("data-vis-agent") || "");
    });
    if (vis.length > 0) draft.ui.visible_agents = vis;
  };

  render();
}
