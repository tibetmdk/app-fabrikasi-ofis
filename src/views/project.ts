import type { OfficeSnapshot, ProjectInfo } from "../types";
import { esc } from "../utils";

const STAGES = [
  { key: "tasarim", label: "Tasarım" },
  { key: "gorsel_uretimi", label: "Görsel" },
  { key: "figma_tasarimi", label: "Figma" },
  { key: "tasarim_onayi", label: "Onay" },
  { key: "yazilim_plani", label: "Plan" },
  { key: "gelistirme", label: "Geliştirme" },
  { key: "son_kontrol", label: "Test/Güvenlik" },
  { key: "kullanici_testi", label: "Teslim" },
];

let lastProjectFp = "";

export function resetProjectViewCache() {
  lastProjectFp = "";
}

export function renderProjectView(container: HTMLElement, snap: OfficeSnapshot | null) {
  const fp = snap ? JSON.stringify({ a: snap.active_project, all: snap.all_projects }) : "null";
  if (fp === lastProjectFp && container.children.length > 0) return;
  lastProjectFp = fp;
  const prevScroll = container.querySelector(".page-scroll")?.scrollTop ?? 0;

  if (!snap || snap.all_projects.length === 0) {
    container.innerHTML = `
      <div class="page-scroll">
        <div class="page-container">
          <div class="card">
            <div class="card-title">📦 Aktif Proje Yok</div>
            <p style="color:var(--text-1)">Henüz başlatılmış bir proje bulunmuyor. Üstteki <strong>Araştır</strong> butonuyla yeni bir pazar araştırması başlatabilirsiniz.</p>
          </div>
        </div>
      </div>
    `;
    return;
  }

  const p: ProjectInfo = snap.active_project || snap.all_projects[0];
  const doneCount = p.gorevler.filter((g) => g.adim === "tamam").length;
  const totalCount = p.gorevler.length;
  const pct = totalCount ? Math.round((doneCount / totalCount) * 100) : 0;

  const tasksHtml = p.gorevler.length
    ? `<div class="tasks-grid">
        ${p.gorevler
          .map((t) => {
            const cls = t.adim === "tamam" ? "tamam" : t.son_hata ? "hata" : "calisiyor";
            const badge =
              t.adim === "tamam"
                ? `<span class="pill ok">✓ tamam</span>`
                : `<span class="pill info">${esc(t.adim)}</span>`;
            const fixes = t.inceleme_duzeltmeleri.length
              ? `<div style="font-size:11px;color:var(--amber)">🔧 ${t.inceleme_duzeltmeleri.length} inceleme düzeltmesi</div>`
              : "";
            const err = t.son_hata
              ? `<div style="font-size:10.5px;color:var(--red);font-family:var(--mono);max-height:48px;overflow:hidden">${esc(t.son_hata.slice(0, 140))}</div>`
              : "";
            return `
              <div class="task-card ${cls}">
                <div style="display:flex;justify-content:space-between;align-items:center">
                  <strong style="font-family:var(--mono)">${esc(t.id)}</strong>
                  ${badge}
                </div>
                <div style="font-size:11px;color:var(--text-2)">
                  Deneme: ${t.deneme} · ${t.incelendi ? "İncelendi ✓" : "İnceleme bekliyor"}
                </div>
                ${fixes}
                ${err}
              </div>`;
          })
          .join("")}
      </div>`
    : `<p style="color:var(--text-2)">Bu proje için henüz görev kartı oluşturulmamış.</p>`;

  const otherProjectsHtml =
    snap.all_projects.length > 1
      ? `<div class="card">
          <div class="card-title">🗂️ Tüm Projeler (${snap.all_projects.length})</div>
          <div style="display:flex;flex-direction:column;gap:6px">
            ${snap.all_projects
              .map(
                (pr) => `
              <div style="display:flex;justify-content:space-between;align-items:center;padding:6px 8px;background:var(--bg-0);border-radius:6px;border:1px solid var(--border)">
                <div>
                  <strong>${esc(pr.ad)}</strong>
                  <span style="color:var(--text-2);font-size:11px;margin-left:8px">${esc(pr.klasor)}</span>
                </div>
                <span class="pill info">${esc(pr.asama_adi)}</span>
              </div>`
              )
              .join("")}
          </div>
        </div>`
      : "";

  container.innerHTML = `
    <div class="page-scroll">
      <div class="page-container">
        <div class="card">
          <div class="card-title">
            <span>🚀 ${esc(p.ad)} <span class="pill info" style="margin-left:6px">${esc(p.asama_adi)}</span></span>
            <span style="font-family:var(--mono);font-size:12px;color:var(--accent)">${doneCount}/${totalCount} görev (%${pct})</span>
          </div>
          <div style="color:var(--text-2);font-size:11.5px;margin-bottom:10px">
            Klasör: <code>${esc(p.klasor)}</code>
            ${p.su_an ? ` · Şu an: <strong style="color:var(--amber)">${esc(p.su_an)}</strong>` : ""}
            ${p.expo_url ? ` · Expo: <a href="${esc(p.expo_url)}" target="_blank" style="color:var(--cyan)">${esc(p.expo_url)}</a>` : ""}
            ${p.figma_url ? ` · Figma: <a href="${esc(p.figma_url)}" target="_blank" style="color:var(--cyan)">Aç</a>` : ""}
          </div>
          <div style="display:flex;gap:6px;flex-wrap:wrap;margin-bottom:12px">
            ${STAGES.map((s) => {
              const active = p.asama.includes(s.key) || p.asama_adi.toLowerCase().includes(s.label.toLowerCase());
              return `<span class="pill ${active ? "ok" : ""}">${esc(s.label)}</span>`;
            }).join("")}
          </div>
          <div style="height:8px;background:var(--bg-0);border-radius:4px;overflow:hidden;border:1px solid var(--border)">
            <div style="height:100%;width:${pct}%;background:var(--accent);transition:width 0.3s"></div>
          </div>
        </div>

        <div class="card">
          <div class="card-title">📋 Görev Kartları (${totalCount})</div>
          ${tasksHtml}
        </div>

        ${otherProjectsHtml}
      </div>
    </div>
  `;
  const scroller = container.querySelector(".page-scroll");
  if (scroller && prevScroll > 0) scroller.scrollTop = prevScroll;
}
