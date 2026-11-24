import "./styles/main.css";
import { api } from "./api";
import {
  AGENT_DEFS,
  type AppSettings,
  type MemoryStats,
  type OfficeSnapshot,
  type SnapshotDelta,
  type TabId,
} from "./types";
import { esc, formatDuration } from "./utils";
import { OfficeView } from "./views/office";
import { GraphView } from "./views/graph";
import { renderProjectView, resetProjectViewCache } from "./views/project";
import { ArchiveView } from "./views/archive";
import { renderSettingsView } from "./views/settings";
import { checkForUpdates, installPendingUpdate } from "./updater";

let settings: AppSettings;
let currentTab: TabId = "office";
let latestSnap: OfficeSnapshot | null = null;
let lastSnapshotHash = "";
let officeView: OfficeView | null = null;
let graphView: GraphView | null = null;
let archiveView: ArchiveView | null = null;

const sessionStart = Date.now() / 1000;
let lastMeasure = sessionStart;
let activeSeconds = 0;

let lastQuotasFp = "";
let lastProjectBannerFp = "";
let lastServiceFp = "";
let lastProfileText = "";
let lastTimerText = "";
let pollTimer: number | null = null;
let pollInFlight = false;
let consecutiveFailures = 0;
let wasDisconnected = false;
let nextRetryAtMs = 0;
let reconnectTicker: number | null = null;
let lastMemCheckAt = 0;

const BACKOFF_STEPS_MS = [2000, 4000, 8000, 15000, 30000];
const IS_WINDOWS = /Windows/i.test(navigator.userAgent);

function computeRetryDelayMs(failures: number, baseMs: number, hidden: boolean): number {
  if (failures <= 0) {
    return hidden ? Math.max(baseMs, 5000) : baseMs;
  }
  const step = BACKOFF_STEPS_MS[Math.min(failures - 1, BACKOFF_STEPS_MS.length - 1)];
  return hidden ? Math.max(step, 10000) : step;
}

function stopReconnectTicker() {
  if (reconnectTicker !== null) {
    window.clearInterval(reconnectTicker);
    reconnectTicker = null;
  }
}

function renderReconnectPill() {
  const svc = document.querySelector("#top-service") as HTMLElement | null;
  const dot = document.querySelector("#top-dot") as HTMLElement | null;
  if (!svc) return;
  const errDetail = latestSnap?.error ? ` (${latestSnap.error})` : "";
  if (pollInFlight && consecutiveFailures > 0) {
    svc.className = "pill warn";
    svc.textContent = "↻ Bağlanıyor...";
    svc.title = `Bağlantı deneniyor${errDetail}`;
    svc.style.cursor = "default";
    if (dot) dot.className = "pulse-dot warn";
    return;
  }
  if (consecutiveFailures > 0 && nextRetryAtMs > 0) {
    const remSec = Math.max(1, Math.ceil((nextRetryAtMs - Date.now()) / 1000));
    svc.className = "pill err";
    svc.textContent = `↻ Yeniden bağlanılıyor (${remSec}sn)...`;
    svc.title = `Bağlantı koptu${errDetail} — Hemen tekrar denemek için tıklayın`;
    svc.style.cursor = "pointer";
    if (dot) dot.className = "pulse-dot err";
  }
}

function retryConnectionNow() {
  if (pollInFlight) return;
  if (pollTimer !== null) {
    window.clearTimeout(pollTimer);
    pollTimer = null;
  }
  stopReconnectTicker();
  nextRetryAtMs = 0;
  pollTick();
}

function applyTheme(s: AppSettings) {
  document.documentElement.setAttribute("data-theme", s.ui.theme);
  document.documentElement.setAttribute("data-accent", s.ui.accent);
  document.documentElement.style.setProperty("--log-size", `${s.ui.font_size}px`);
}

let toastTimer: number | null = null;
function showToast(msg: string, isErr = false) {
  document.querySelector(".toast")?.remove();
  const el = document.createElement("div");
  el.className = `toast ${isErr ? "err" : ""}`;
  el.textContent = msg;
  document.body.appendChild(el);
  if (toastTimer) window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => el.remove(), 4000);
}

function buildShell() {
  const app = document.querySelector("#app")!;
  app.innerHTML = `
    <div class="shell">
      <header class="topbar">
        <div class="topbar-row">
          <div class="brand-group">
            <div class="brand">
              <span class="pulse-dot" id="top-dot"></span>
              <span>APP FABRİKASI OFİSİ</span>
            </div>
            <span class="pill" id="top-profile">Yerel</span>
            <span class="pill" id="top-service">Bağlanıyor...</span>
            <span class="pill" id="top-session-time" style="font-family:var(--mono)">⏱ 0dk</span>
          </div>

          <nav class="nav-tabs" id="nav-tabs">
            <button class="nav-tab active" data-tab="office">🖥 Ofis</button>
            <button class="nav-tab" data-tab="graph">🕸️ Hafıza Grafı</button>
            <button class="nav-tab" data-tab="project">🚀 Proje</button>
            <button class="nav-tab" data-tab="archive">📁 Arşiv</button>
            <button class="nav-tab" data-tab="settings">⚙️ Ayarlar</button>
          </nav>

          <div class="controls">
            <button class="btn sm primary" id="btn-update" style="display:none" title="Yeni sürümü indir ve kur">⬆️ Güncelle</button>
            <button class="btn sm" id="btn-research" title="Yeni pazar araştırması başlat">🔍 Araştır</button>
            <button class="btn sm warn" id="btn-pause" title="Duraklat / Devam et (d)">⏸ Duraklat</button>
            <button class="btn sm primary" id="btn-svc-toggle" title="Ofis servisini başlat/durdur">▶ Ofisi Aç</button>
          </div>
        </div>

        <div class="telemetry-strip">
          <div class="quotas-list" id="top-quotas"></div>
          <div id="top-project-wrap"></div>
        </div>
      </header>

      <main class="main-content" id="main-content"></main>

      <footer class="statusbar">
        <div>Kısayollar: <strong>1–9</strong> rol büyüt · <strong>o</strong> akış · <strong>k</strong> konuşma · <strong>m</strong> yaz · <strong>d</strong> duraklat · <strong>ESC</strong> geri</div>
        <div style="display:flex;align-items:center;gap:8px">
          <span class="ram-badge" id="ram-badge" title="Canlı PSS bellek kullanımı">RAM -- MB</span>
          <span id="status-right">Pencereyi kapatmak ajanları durdurmaz</span>
        </div>
      </footer>
    </div>
  `;

  document.querySelectorAll("[data-tab]").forEach((btn) => {
    btn.addEventListener("click", () => {
      switchTab(btn.getAttribute("data-tab") as TabId);
    });
  });

  document.querySelector("#top-service")?.addEventListener("click", () => {
    if (consecutiveFailures > 0 || (latestSnap && !latestSnap.connected)) {
      retryConnectionNow();
    }
  });

  document.querySelector("#btn-update")?.addEventListener("click", () => {
    installPendingUpdate(showToast);
  });

  document.querySelector("#btn-research")?.addEventListener("click", async () => {
    const res = await api.controlFactory("arastir");
    showToast(res.message, !res.ok);
  });

  document.querySelector("#btn-pause")?.addEventListener("click", async () => {
    const act = latestSnap?.paused ? "devam" : "durdur";
    const res = await api.controlFactory(act);
    showToast(res.message, !res.ok);
  });

  document.querySelector("#btn-svc-toggle")?.addEventListener("click", async () => {
    const isRunning = latestSnap?.service_status === "active";
    if (isRunning) {
      const ok = window.confirm(
        "Ofis servisi durdurulsun mu? Süren işler güvenli noktada durur ve Telegram'a 'Ofis kapandı' gider."
      );
      if (!ok) return;
      const res = await api.controlFactory("stop");
      showToast(res.message, !res.ok);
    } else {
      const res = await api.controlFactory("start");
      showToast(res.message, !res.ok);
    }
  });
}

function updateSessionTimer(snap: OfficeSnapshot | null) {
  const now = Date.now() / 1000;
  const dt = Math.min(now - lastMeasure, 10);
  lastMeasure = now;
  if (snap && snap.service_status === "active" && Object.keys(snap.active_agents).length > 0) {
    activeSeconds += dt;
  }
  const timer = document.querySelector("#top-session-time");
  if (timer) {
    const total = now - sessionStart;
    const pct = Math.round((100 * activeSeconds) / Math.max(1, total));
    const txt = `⏱ ${formatDuration(total)} · aktif ${formatDuration(activeSeconds)} (%${pct})`;
    if (txt !== lastTimerText) {
      lastTimerText = txt;
      timer.textContent = txt;
    }
  }
}

function updateTopbar(snap: OfficeSnapshot) {
  updateSessionTimer(snap);

  const profText = `${snap.mode === "remote" ? "☁️" : "💻"} ${snap.profile_name}`;
  if (profText !== lastProfileText) {
    lastProfileText = profText;
    const prof = document.querySelector("#top-profile");
    if (prof) prof.textContent = profText;
  }

  const svcFp = `${snap.connected}|${snap.service_status}|${snap.paused}|${snap.error || ""}`;
  if (svcFp !== lastServiceFp) {
    lastServiceFp = svcFp;
    const dot = document.querySelector("#top-dot");
    const svc = document.querySelector("#top-service") as HTMLElement | null;
    const pauseBtn = document.querySelector("#btn-pause") as HTMLButtonElement | null;
    const svcBtn = document.querySelector("#btn-svc-toggle") as HTMLButtonElement | null;

    if (dot) {
      dot.className = "pulse-dot";
      if (!snap.connected) dot.classList.add("err");
      else if (snap.service_status === "active") dot.classList.add("active");
      else dot.classList.add("warn");
    }
    if (svc) {
      if (!snap.connected) {
        if (consecutiveFailures > 0) {
          renderReconnectPill();
        } else {
          svc.className = "pill err";
          svc.textContent = "✖ Bağlantı Yok";
          svc.title = snap.error
            ? `${snap.error} — Hemen tekrar denemek için tıklayın`
            : "Hemen tekrar denemek için tıklayın";
          svc.style.cursor = "pointer";
        }
      } else if (snap.service_status === "active") {
        svc.className = snap.paused ? "pill warn" : "pill ok";
        svc.textContent = snap.paused ? "⏸ DURAKLATILDI" : "● OFİS AÇIK";
        svc.title = "";
        svc.style.cursor = "default";
      } else {
        svc.className = "pill err";
        svc.textContent = `○ OFİS KAPALI (${snap.service_status})`;
        svc.title = "";
        svc.style.cursor = "default";
      }
    }
    if (pauseBtn) {
      pauseBtn.textContent = snap.paused ? "▶ Devam Et" : "⏸ Duraklat";
      pauseBtn.className = snap.paused ? "btn sm primary" : "btn sm warn";
    }
    if (svcBtn) {
      const running = snap.service_status === "active";
      svcBtn.textContent = running ? "⏹ Ofisi Kapat" : "▶ Ofisi Aç";
      svcBtn.className = running ? "btn sm danger" : "btn sm primary";
    }
  }

  // Quotas (only rebuild DOM when values actually change)
  const quotasFp = snap.quotas
    .map((q) => `${q.id}:${q.has_data ? `${Math.round(q.p5 * 100)}:${Math.round(q.hafta * 100)}` : "-"}`)
    .join("|");
  if (quotasFp !== lastQuotasFp) {
    lastQuotasFp = quotasFp;
    const qEl = document.querySelector("#top-quotas");
    if (qEl) {
      qEl.innerHTML = snap.quotas
        .map((q) => {
          if (!q.has_data) {
            return `<div class="quota-item"><span class="quota-name">${esc(q.name)}</span><span class="quota-pct">veri yok</span></div>`;
          }
          const m = Math.max(q.p5, q.hafta);
          const cls = m < 0.7 ? "" : m < 0.85 ? "warn" : "crit";
          const p5Pct = Math.round(q.p5 * 100);
          const hfPct = Math.round(q.hafta * 100);
          return `
            <div class="quota-item" title="5 saatlik: %${p5Pct} · Haftalık: %${hfPct}">
              <span class="quota-name">${esc(q.name)}</span>
              <div class="quota-bar"><div class="quota-fill ${cls}" style="width:${Math.min(100, p5Pct)}%"></div></div>
              <span class="quota-pct">5s %${p5Pct} · hf %${hfPct}</span>
            </div>`;
        })
        .join("");
    }
  }

  // Active project banner (only rebuild DOM when changed)
  const p = snap.active_project || snap.all_projects[0] || null;
  const done = p ? p.gorevler.filter((g) => g.adim === "tamam").length : 0;
  const total = p ? p.gorevler.length : 0;
  const projFp = p ? `${p.ad}|${p.asama_adi}|${done}/${total}` : "none";
  if (projFp !== lastProjectBannerFp) {
    lastProjectBannerFp = projFp;
    const pWrap = document.querySelector("#top-project-wrap");
    if (pWrap) {
      if (p) {
        const pct = total ? Math.round((done / total) * 100) : 0;
        pWrap.innerHTML = `
          <div class="project-banner" id="go-project-tab">
            <span>🚀 ${esc(p.ad)}</span>
            <span style="color:var(--text-1);font-weight:500">${esc(p.asama_adi)}</span>
            ${
              total
                ? `<div class="mini-progress"><div style="width:${pct}%"></div></div>
                   <span style="font-family:var(--mono);font-size:11px">${done}/${total}</span>`
                : ""
            }
          </div>`;
        pWrap.querySelector("#go-project-tab")?.addEventListener("click", () => switchTab("project"));
      } else {
        pWrap.innerHTML = `<span style="color:var(--text-2);font-size:11px">Aktif proje yok</span>`;
      }
    }
  }
}

async function switchTab(tab: TabId) {
  if (officeView) {
    officeView.unmount();
    officeView = null;
  }
  if (graphView) {
    graphView.unmount();
    graphView = null;
  }
  if (archiveView) {
    archiveView.unmount();
    archiveView = null;
  }
  currentTab = tab;
  document.querySelectorAll("[data-tab]").forEach((b) => {
    b.classList.toggle("active", b.getAttribute("data-tab") === tab);
  });

  const main = document.querySelector("#main-content") as HTMLElement;
  main.innerHTML = "";

  if (tab === "office") {
    officeView = new OfficeView(main, () => settings, showToast);
    if (latestSnap) officeView.update(latestSnap);
  } else if (tab === "graph") {
    graphView = new GraphView(main);
    await graphView.mount();
  } else if (tab === "project") {
    resetProjectViewCache();
    renderProjectView(main, latestSnap);
  } else if (tab === "archive") {
    archiveView = new ArchiveView(main);
    await archiveView.mount();
  } else if (tab === "settings") {
    renderSettingsView(
      main,
      settings,
      async (next) => {
        await api.saveSettings(next);
        settings = next;
        applyTheme(settings);
        lastSnapshotHash = "";
        consecutiveFailures = 0;
        wasDisconnected = false;
        nextRetryAtMs = 0;
        stopReconnectTicker();
        lastServiceFp = "";
        latestSnap = await api.getSnapshot();
        latestSnap.active_project = latestSnap.all_projects[0] || null;
        updateTopbar(latestSnap);
      },
      showToast
    );
  }
}

function bindGlobalShortcuts() {
  window.addEventListener("keydown", (e) => {
    const tag = (e.target as HTMLElement)?.tagName;
    const typing = tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
    if (e.key === "Escape") {
      if (currentTab === "office" && officeView?.getFocus()) {
        officeView.setFocus(null);
      }
      return;
    }
    if (typing) return;

    if (currentTab === "office" && officeView) {
      if (e.key >= "1" && e.key <= String(AGENT_DEFS.length)) {
        const idx = Number(e.key) - 1;
        officeView.setFocus({ kind: "agent", id: AGENT_DEFS[idx].id });
      } else if (e.key === "0") {
        officeView.setFocus(null);
      } else if (e.key === "o" || e.key === "O") {
        officeView.setFocus({ kind: "events" });
      } else if (e.key === "k" || e.key === "K") {
        officeView.setFocus({ kind: "chat" });
      } else if (e.key === "m" || e.key === "M") {
        e.preventDefault();
        officeView.focusChatInput();
      } else if (e.key === "d" || e.key === "D") {
        const act = latestSnap?.paused ? "devam" : "durdur";
        api.controlFactory(act).then((r) => showToast(r.message, !r.ok));
      }
    }
  });
}

function updateRamBadge(mem: MemoryStats) {
  const el = document.querySelector("#ram-badge") as HTMLElement | null;
  if (!el || mem.total_mb <= 0) return;
  const webLabel = IS_WINDOWS ? "WebView2" : "WebKit";
  const netPart = mem.network_mb >= 1 ? ` · Net ${Math.round(mem.network_mb)}` : "";
  el.textContent = `RAM ${Math.round(mem.total_mb)} MB (Rust ${Math.round(mem.rust_mb)} · ${webLabel} ${Math.round(mem.webkit_mb)}${netPart})`;
  el.title = IS_WINDOWS
    ? `Canlı bellek (Private Bytes) — Rust: ${mem.rust_mb} MB · WebView2: ${mem.webkit_mb} MB`
    : `Canlı PSS bellek — Rust: ${mem.rust_mb} MB · WebKit: ${mem.webkit_mb} MB · Network: ${mem.network_mb} MB`;
  el.className = "ram-badge";
  if (mem.total_mb >= 260) el.classList.add("err");
  else if (mem.total_mb >= 180) el.classList.add("warn");
}

function applySnapshotDelta(base: OfficeSnapshot, d: SnapshotDelta): OfficeSnapshot {
  base.connected = d.connected;
  base.error = d.error;
  base.profile_name = d.profile_name;
  base.mode = d.mode;
  base.service_status = d.service_status;
  base.paused = d.paused;
  base.manager_session = d.manager_session;
  base.message_queue_count = d.message_queue_count;
  base.manager_context = d.manager_context;
  base.quotas = d.quotas;
  base.active_agents = d.active_agents;
  base.assigned_models = d.assigned_models;
  base.jobs = d.jobs;
  base.timestamp = d.timestamp;
  if (d.agent_logs) {
    for (const [rol, lines] of Object.entries(d.agent_logs)) {
      base.agent_logs[rol] = lines;
    }
  }
  if (d.events_log !== undefined) base.events_log = d.events_log;
  if (d.system_log !== undefined) base.system_log = d.system_log;
  if (d.messages !== undefined) base.messages = d.messages;
  if (d.all_projects !== undefined) {
    base.all_projects = d.all_projects;
    base.active_project = d.all_projects[0] || null;
  }
  return base;
}

async function pollTick() {
  if (pollInFlight) return;
  pollInFlight = true;
  if (consecutiveFailures > 0) {
    renderReconnectPill();
  }
  let isConnected = true;
  try {
    const reqHash = latestSnap ? lastSnapshotHash : "";
    const res = await api.pollSnapshot(reqHash);
    lastSnapshotHash = res.hash;
    let updatedSnap: OfficeSnapshot | null = null;
    if (res.snapshot) {
      res.snapshot.active_project = res.snapshot.all_projects[0] || null;
      latestSnap = res.snapshot;
      updatedSnap = latestSnap;
    } else if (res.delta && latestSnap) {
      latestSnap = applySnapshotDelta(latestSnap, res.delta);
      updatedSnap = latestSnap;
    }

    isConnected = Boolean(latestSnap?.connected);
    if (isConnected) {
      if (wasDisconnected) {
        wasDisconnected = false;
        showToast("✅ Sunucu bağlantısı yeniden kuruldu.");
      }
      if (consecutiveFailures > 0) {
        consecutiveFailures = 0;
        nextRetryAtMs = 0;
        stopReconnectTicker();
        lastServiceFp = "";
      }
    }

    if (updatedSnap) {
      updateTopbar(updatedSnap);
      if (currentTab === "office" && officeView) {
        officeView.update(updatedSnap);
      } else if (currentTab === "project") {
        const main = document.querySelector("#main-content") as HTMLElement;
        renderProjectView(main, updatedSnap);
      }
    } else if (!document.hidden) {
      updateSessionTimer(latestSnap);
      if (currentTab === "office" && officeView) {
        officeView.tickTimers();
      }
    }

    const nowMs = Date.now();
    if (!document.hidden && nowMs - lastMemCheckAt >= 5000) {
      lastMemCheckAt = nowMs;
      const mem = await api.getMemoryStats().catch(() => null);
      if (mem) updateRamBadge(mem);
    }
  } catch {
    isConnected = false;
  } finally {
    pollInFlight = false;
    const baseMs = Math.max(600, Math.min(10000, settings?.ui?.poll_interval_ms || 1000));
    if (!isConnected) {
      consecutiveFailures += 1;
      wasDisconnected = true;
      const delay = computeRetryDelayMs(consecutiveFailures, baseMs, document.hidden);
      nextRetryAtMs = Date.now() + delay;
      renderReconnectPill();
      if (reconnectTicker === null) {
        reconnectTicker = window.setInterval(renderReconnectPill, 1000);
      }
      pollTimer = window.setTimeout(pollTick, delay);
    } else {
      const delay = computeRetryDelayMs(0, baseMs, document.hidden);
      pollTimer = window.setTimeout(pollTick, delay);
    }
  }
}

async function bootstrap() {
  settings = await api.getSettings();
  applyTheme(settings);
  buildShell();
  bindGlobalShortcuts();

  await switchTab("office");

  const initRes = await api.pollSnapshot("");
  lastSnapshotHash = initRes.hash;
  if (initRes.snapshot) {
    initRes.snapshot.active_project = initRes.snapshot.all_projects[0] || null;
    latestSnap = initRes.snapshot;
    if (!initRes.snapshot.connected) {
      consecutiveFailures = 1;
      wasDisconnected = true;
    }
    updateTopbar(initRes.snapshot);
    officeView?.update(initRes.snapshot);
  }
  lastMemCheckAt = Date.now();
  api.getMemoryStats().then(updateRamBadge).catch(() => {});

  document.addEventListener("visibilitychange", () => {
    if (!document.hidden && !pollInFlight) {
      if (pollTimer !== null) {
        window.clearTimeout(pollTimer);
        pollTimer = null;
      }
      stopReconnectTicker();
      nextRetryAtMs = 0;
      pollTick();
    }
  });

  const baseMs = Math.max(600, Math.min(10000, settings?.ui?.poll_interval_ms || 1000));
  const initDelay = computeRetryDelayMs(consecutiveFailures, baseMs, document.hidden);
  if (consecutiveFailures > 0) {
    nextRetryAtMs = Date.now() + initDelay;
    renderReconnectPill();
    reconnectTicker = window.setInterval(renderReconnectPill, 1000);
  }
  pollTimer = window.setTimeout(pollTick, initDelay);
  window.setTimeout(() => checkForUpdates(showToast, true), 3000);
}

bootstrap();
