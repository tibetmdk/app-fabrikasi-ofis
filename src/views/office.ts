import { api } from "../api";
import { AGENT_DEFS, type AppSettings, type OfficeSnapshot } from "../types";
import { classifyLogLine, esc, formatMMSS, formatTimeHM, linkifyText } from "../utils";

export type FocusTarget = null | { kind: "agent"; id: string } | { kind: "events" } | { kind: "chat" };

interface StreamTask {
  el: HTMLElement;
  scrollEl: HTMLElement | null;
  fullText: string;
  shown: number;
  done?: () => void;
}

function extractTouchedFile(line: string): string | null {
  const m = line.match(/(?:📝\s*dosya yazıyor:|📖\s*okuyor:)\s*(.+)$/);
  if (!m) return null;
  const raw = m[1].trim().replace(/^["'`]|["'`]$/g, "");
  return raw || null;
}

function parseThoughtLine(line: string): { ts: string; body: string } | null {
  const m = line.match(/^(\d{2}:\d{2}:\d{2})\s+🧠\s*(.*)$/);
  if (!m) return null;
  return {
    ts: m[1],
    body: m[2].replace(/\s*⏎\s*/g, "\n").trim(),
  };
}

export class OfficeView {
  private root: HTMLElement;
  private focus: FocusTarget = null;
  private focusLines: string[] = [];
  private focusSearch = "";
  private focusFilter: "all" | "err" | "cmd" | "write" | "think" = "all";
  private lastMsgCount = -1;
  private lastLogTail = new Map<string, { len: number; first: string; last: string }>();
  private lastMetaFingerprints = new Map<string, string>();
  private lastStateTexts = new Map<string, string>();
  private recording = false;
  private transcribing = false;
  private builtMode: string = "";
  private showToast: (msg: string, err?: boolean) => void;
  private getSettings: () => AppSettings;
  private lastSnap: OfficeSnapshot | null = null;

  // Live Code / Diff Inspector state (Level 1)
  private showCodePane = true;
  private codeMode: "diff" | "file" = "diff";
  private autoFollowFile = true;
  private selectedFile: string | null = null;
  private changedFiles: string[] = [];
  private codeLines: string[] = [];
  private codeFingerprint = "";
  private lastCodeFetchAt = 0;
  private codeFetchInFlight = false;

  // Single shared lightweight rAF streaming queue (Gemini-like gradual reveal)
  private streamQueue: StreamTask[] = [];
  private streamRafId: number | null = null;
  private lastStreamFrameAt = 0;

  constructor(
    container: HTMLElement,
    getSettings: () => AppSettings,
    showToast: (msg: string, err?: boolean) => void
  ) {
    this.root = container;
    this.getSettings = getSettings;
    this.showToast = showToast;
  }

  private cancelStreams() {
    if (this.streamRafId !== null) {
      cancelAnimationFrame(this.streamRafId);
      this.streamRafId = null;
    }
    for (const task of this.streamQueue) {
      task.el.classList.remove("streaming-cursor");
    }
    this.streamQueue = [];
  }

  private enqueueStream(task: StreamTask) {
    task.el.classList.add("streaming-cursor");
    this.streamQueue.push(task);
    if (this.streamRafId === null) {
      this.lastStreamFrameAt = performance.now();
      this.streamRafId = requestAnimationFrame(this.stepStreams);
    }
  }

  private stepStreams = (now: number) => {
    const dt = Math.min(64, Math.max(8, now - this.lastStreamFrameAt));
    this.lastStreamFrameAt = now;

    // Adaptive speed: silky ~115 chars/sec normally, speeds up gently if backlog grows
    let backlog = 0;
    for (let i = 0; i < this.streamQueue.length; i++) {
      backlog += this.streamQueue[i].fullText.length - this.streamQueue[i].shown;
    }
    const charsPerSec = backlog > 600 ? 480 : backlog > 260 ? 240 : 120;
    let budget = Math.max(1, Math.round((charsPerSec * dt) / 1000));

    while (budget > 0 && this.streamQueue.length > 0) {
      const head = this.streamQueue[0];
      if (!head.el.isConnected) {
        this.streamQueue.shift();
        continue;
      }
      const scrollEl = head.scrollEl;
      const atBottom =
        scrollEl !== null &&
        scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight < 48;

      const remaining = head.fullText.length - head.shown;
      const take = Math.min(remaining, budget);
      head.shown += take;
      budget -= take;
      head.el.textContent = head.fullText.slice(0, head.shown);

      if (atBottom && scrollEl) {
        scrollEl.scrollTop = scrollEl.scrollHeight;
      }

      if (head.shown >= head.fullText.length) {
        head.el.classList.remove("streaming-cursor");
        head.done?.();
        this.streamQueue.shift();
      }
    }

    if (this.streamQueue.length > 0) {
      this.streamRafId = requestAnimationFrame(this.stepStreams);
    } else {
      this.streamRafId = null;
    }
  };

  public unmount() {
    this.cancelStreams();
    this.focus = null;
    this.focusLines = [];
    this.lastSnap = null;
    this.lastLogTail.clear();
    this.lastMetaFingerprints.clear();
    this.lastStateTexts.clear();
    this.lastMsgCount = -1;
    this.builtMode = "";
    this.selectedFile = null;
    this.changedFiles = [];
    this.codeLines = [];
    this.codeFingerprint = "";
  }

  public setFocus(target: FocusTarget) {
    this.cancelStreams();
    this.focus = target;
    this.focusLines = [];
    this.focusSearch = "";
    this.focusFilter = "all";
    this.builtMode = "";
    this.lastLogTail.clear();
    this.lastMetaFingerprints.clear();
    this.lastStateTexts.clear();
    this.lastMsgCount = -1;
    this.selectedFile = null;
    this.changedFiles = [];
    this.codeLines = [];
    this.codeFingerprint = "";
    this.lastCodeFetchAt = 0;
    this.autoFollowFile = true;
    if (this.lastSnap) this.update(this.lastSnap);
    if (target && target.kind !== "chat") {
      const logName = target.kind === "events" ? "olaylar" : target.id;
      api.getFullLog(logName, 1500).then((lines) => {
        if (!this.focus || (this.focus.kind === "agent" && this.focus.id !== logName)) return;
        this.focusLines = lines;
        this.renderFocusLogs();
        if (this.focus.kind === "agent" && this.showCodePane) {
          // Find the most recently touched file in this agent's log
          for (let i = lines.length - 1; i >= 0; i--) {
            const f = extractTouchedFile(lines[i]);
            if (f) {
              this.selectedFile = f;
              break;
            }
          }
          this.refreshCodePane(true);
        }
      });
    }
  }

  public focusChatInput() {
    const el = this.root.querySelector("#chat-input") as HTMLTextAreaElement | null;
    el?.focus();
  }

  public getFocus(): FocusTarget {
    return this.focus;
  }

  public update(snap: OfficeSnapshot) {
    this.lastSnap = snap;
    const settings = this.getSettings();
    const ui = settings.ui;

    const modeKey = this.focus
      ? `focus:${this.focus.kind}:${this.focus.kind === "agent" ? this.focus.id : ""}`
      : `grid:${ui.grid_columns}:${ui.show_events}:${ui.show_chat}:${ui.show_system}:${ui.visible_agents.join(",")}`;

    if (this.builtMode !== modeKey) {
      this.builtMode = modeKey;
      this.lastLogTail.clear();
      this.lastMetaFingerprints.clear();
      this.lastStateTexts.clear();
      this.lastMsgCount = -1;
      if (this.focus) {
        this.buildFocusSkeleton(snap);
      } else {
        this.buildGridSkeleton(ui);
      }
    }

    if (this.focus) {
      this.updateFocusView(snap);
    } else {
      this.updateGridView(snap, ui);
    }
  }

  private buildGridSkeleton(ui: AppSettings["ui"]) {
    const hasLeft = ui.show_events || ui.show_chat || ui.show_system;
    const visAgents = AGENT_DEFS.filter((a) => ui.visible_agents.includes(a.id));
    const gridCls =
      ui.grid_columns === "2" ? "agents-grid cols-2" : ui.grid_columns === "3" ? "agents-grid" : "agents-grid";

    this.root.innerHTML = `
      <div class="office-layout ${hasLeft ? "" : "no-left"}">
        ${
          hasLeft
            ? `<div class="left-column">
                ${
                  ui.show_events
                    ? `<div class="panel flex-1">
                        <div class="panel-header">
                          <div class="panel-title">⚡ İş Akışı · Ajan Devirleri</div>
                          <div class="panel-actions">
                            <button class="icon-btn" id="btn-expand-events" title="Tam ekran (o)">⤢</button>
                          </div>
                        </div>
                        <div class="log-body" id="log-events"></div>
                      </div>`
                    : ""
                }
                ${
                  ui.show_chat
                    ? `<div class="panel flex-chat">
                        <div class="panel-header">
                          <div class="panel-title">💬 Yönetici ile Konuşma <span class="panel-key">📱/🖥</span></div>
                          <div class="panel-actions">
                            <span class="pill info" id="chat-queue-pill" style="display:none"></span>
                            <button class="icon-btn" id="btn-expand-chat" title="Tam ekran (k)">⤢</button>
                          </div>
                        </div>
                        <div class="chat-body" id="chat-body"></div>
                        <div class="chat-composer">
                          <button class="mic-btn" id="btn-mic" title="Dikte (Basıp konuşun, tekrar basınca metne çevrilir)">🎙️</button>
                          <textarea class="chat-input" id="chat-input" rows="1" placeholder="Yöneticiye yazın (Enter gönder, Shift+Enter satır)..."></textarea>
                          <button class="btn primary" id="btn-send">Gönder</button>
                        </div>
                      </div>`
                    : ""
                }
                ${
                  ui.show_system
                    ? `<div class="panel flex-sys">
                        <div class="panel-header">
                          <div class="panel-title">🛠️ Otomatik Kontroller</div>
                        </div>
                        <div class="log-body" id="log-system"></div>
                      </div>`
                    : ""
                }
              </div>`
            : ""
        }
        <div class="${gridCls}" id="agents-grid">
          ${visAgents
            .map(
              (a) => `
            <div class="panel" id="agent-card-${a.id}">
              <div class="panel-header">
                <div class="panel-title">
                  <span class="panel-key">${a.key}</span>
                  <span class="pulse-dot" id="agent-dot-${a.id}"></span>
                  <span>${esc(a.short)}</span>
                  <span id="agent-state-${a.id}" class="agent-state-idle"></span>
                </div>
                <div class="panel-actions">
                  <button class="icon-btn" data-expand-agent="${a.id}" title="Tam ekran (${a.key})">⤢</button>
                </div>
              </div>
              <div class="agent-meta" id="agent-meta-${a.id}"></div>
              <div class="log-body" id="log-agent-${a.id}"></div>
            </div>`
            )
            .join("")}
        </div>
      </div>
    `;

    this.root.querySelector("#btn-expand-events")?.addEventListener("click", () => {
      this.setFocus({ kind: "events" });
    });
    this.root.querySelector("#btn-expand-chat")?.addEventListener("click", () => {
      this.setFocus({ kind: "chat" });
    });
    this.root.querySelectorAll("[data-expand-agent]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const id = btn.getAttribute("data-expand-agent");
        if (id) this.setFocus({ kind: "agent", id });
      });
    });

    this.bindChatControls();
  }

  private buildFocusSkeleton(_snap: OfficeSnapshot) {
    if (!this.focus) return;
    if (this.focus.kind === "chat") {
      this.root.innerHTML = `
        <div class="focus-view">
          <div class="panel" style="flex:1">
            <div class="panel-header">
              <div class="panel-title">💬 Yönetici ile Konuşma (Tam Ekran)</div>
              <div class="panel-actions">
                <span class="pill info" id="chat-queue-pill" style="display:none"></span>
                <button class="btn sm" id="btn-focus-back">← Geri (ESC)</button>
              </div>
            </div>
            <div class="chat-body" id="chat-body"></div>
            <div class="chat-composer">
              <button class="mic-btn" id="btn-mic" title="Dikte">🎙️</button>
              <textarea class="chat-input" id="chat-input" rows="2" placeholder="Yöneticiye yazın (Enter gönder, Shift+Enter satır)..."></textarea>
              <button class="btn primary" id="btn-send">Gönder</button>
            </div>
          </div>
        </div>
      `;
      this.root.querySelector("#btn-focus-back")?.addEventListener("click", () => this.setFocus(null));
      this.bindChatControls();
      return;
    }

    const title =
      this.focus.kind === "events"
        ? "⚡ Tüm İş Akışı (olaylar.log)"
        : (() => {
            const def = AGENT_DEFS.find((a) => a.id === (this.focus as { kind: "agent"; id: string }).id);
            return def ? `${def.key}· ${def.name}` : "Ajan";
          })();

    const isAgentFocus = this.focus.kind === "agent";
    const splitCls = isAgentFocus && this.showCodePane ? "focus-split" : "focus-split single";

    this.root.innerHTML = `
      <div class="focus-view">
        <div class="${splitCls}" id="focus-split">
          <div class="panel" id="focus-panel">
            <div class="panel-header">
              <div class="panel-title">${esc(title)} <span id="focus-status" style="margin-left:6px"></span></div>
              <div class="panel-actions">
                ${
                  isAgentFocus
                    ? `<button class="btn sm ${this.showCodePane ? "primary" : ""}" id="btn-toggle-code" title="Canlı Kod/Diff panelini aç/kapat">💻 Kod/Diff</button>`
                    : ""
                }
                <button class="btn sm" id="btn-focus-bottom">↓ En Alta İn</button>
                <button class="btn sm" id="btn-focus-back">← Geri (ESC)</button>
              </div>
            </div>
            <div class="agent-meta" id="focus-meta" style="display:none"></div>
            <div class="focus-toolbar">
              <input class="search-input" id="focus-search" placeholder="Log içinde ara (Ctrl+F)..." />
              <button class="btn sm primary" data-flt="all">Tümü</button>
              <button class="btn sm" data-flt="err">❌ Hatalar</button>
              <button class="btn sm" data-flt="cmd">⚙️ Komutlar</button>
              <button class="btn sm" data-flt="write">📝 Dosya</button>
              <button class="btn sm" data-flt="think">🧠 Düşünce</button>
            </div>
            <div class="log-body" id="focus-log-body"></div>
          </div>
          ${
            isAgentFocus
              ? `<div class="panel" id="focus-code-panel" style="${this.showCodePane ? "" : "display:none"}">
                  <div class="panel-header">
                    <div class="panel-title">💻 Canlı Kod & Diff <span class="panel-key" id="code-project-tag"></span></div>
                    <div class="panel-actions">
                      <button class="btn sm ${this.codeMode === "diff" ? "primary" : ""}" id="btn-code-diff">🔀 Diff</button>
                      <button class="btn sm ${this.codeMode === "file" ? "primary" : ""}" id="btn-code-file">📄 Dosya</button>
                      <button class="btn sm ${this.autoFollowFile ? "primary" : ""}" id="btn-code-follow" title="Ajanın dokunduğu dosyayı otomatik takip et">⚡ Oto</button>
                      <button class="icon-btn" id="btn-code-refresh" title="Şimdi yenile">↻</button>
                    </div>
                  </div>
                  <div class="focus-toolbar">
                    <select class="code-file-select" id="code-file-select">
                      <option value="">Tüm değişen dosyalar (git diff)</option>
                    </select>
                    <span id="code-meta-info" style="font-size:10.5px;color:var(--text-2);font-family:var(--mono);margin-left:auto"></span>
                  </div>
                  <div class="code-body" id="focus-code-body">
                    <div class="log-empty">Ajan bir dosyaya dokunduğunda burada canlı akacak...</div>
                  </div>
                </div>`
              : ""
          }
        </div>
      </div>
    `;

    this.root.querySelector("#btn-focus-back")?.addEventListener("click", () => this.setFocus(null));
    this.root.querySelector("#btn-focus-bottom")?.addEventListener("click", () => {
      const el = this.root.querySelector("#focus-log-body");
      if (el) el.scrollTop = el.scrollHeight;
    });
    this.root.querySelector("#btn-toggle-code")?.addEventListener("click", () => {
      this.showCodePane = !this.showCodePane;
      const split = this.root.querySelector("#focus-split");
      const pane = this.root.querySelector("#focus-code-panel") as HTMLElement | null;
      const btn = this.root.querySelector("#btn-toggle-code");
      split?.classList.toggle("single", !this.showCodePane);
      if (pane) pane.style.display = this.showCodePane ? "" : "none";
      btn?.classList.toggle("primary", this.showCodePane);
      if (this.showCodePane) this.refreshCodePane(true);
    });
    this.root.querySelector("#btn-code-diff")?.addEventListener("click", () => {
      this.codeMode = "diff";
      this.root.querySelector("#btn-code-diff")?.classList.add("primary");
      this.root.querySelector("#btn-code-file")?.classList.remove("primary");
      this.codeFingerprint = "";
      this.codeLines = [];
      this.refreshCodePane(true);
    });
    this.root.querySelector("#btn-code-file")?.addEventListener("click", () => {
      this.codeMode = "file";
      this.root.querySelector("#btn-code-file")?.classList.add("primary");
      this.root.querySelector("#btn-code-diff")?.classList.remove("primary");
      this.codeFingerprint = "";
      this.codeLines = [];
      this.refreshCodePane(true);
    });
    this.root.querySelector("#btn-code-follow")?.addEventListener("click", () => {
      this.autoFollowFile = !this.autoFollowFile;
      this.root.querySelector("#btn-code-follow")?.classList.toggle("primary", this.autoFollowFile);
    });
    this.root.querySelector("#btn-code-refresh")?.addEventListener("click", () => {
      this.refreshCodePane(true);
    });
    const fileSelect = this.root.querySelector("#code-file-select") as HTMLSelectElement | null;
    fileSelect?.addEventListener("change", () => {
      this.selectedFile = fileSelect.value || null;
      this.autoFollowFile = false;
      this.root.querySelector("#btn-code-follow")?.classList.remove("primary");
      this.codeFingerprint = "";
      this.codeLines = [];
      this.refreshCodePane(true);
    });
    const search = this.root.querySelector("#focus-search") as HTMLInputElement | null;
    search?.addEventListener("input", () => {
      this.focusSearch = search.value.trim().toLowerCase();
      this.renderFocusLogs();
    });
    this.root.querySelectorAll("[data-flt]").forEach((btn) => {
      btn.addEventListener("click", () => {
        this.focusFilter = (btn.getAttribute("data-flt") as typeof this.focusFilter) || "all";
        this.root.querySelectorAll("[data-flt]").forEach((b) => b.classList.remove("primary"));
        btn.classList.add("primary");
        this.renderFocusLogs();
      });
    });
  }

  private bindChatControls() {
    const input = this.root.querySelector("#chat-input") as HTMLTextAreaElement | null;
    const sendBtn = this.root.querySelector("#btn-send") as HTMLButtonElement | null;
    const micBtn = this.root.querySelector("#btn-mic") as HTMLButtonElement | null;

    const resizeInput = () => {
      if (!input) return;
      input.style.height = "auto";
      input.style.height = `${Math.min(140, input.scrollHeight)}px`;
    };

    const doSend = async () => {
      if (!input) return;
      const txt = input.value.trim();
      if (!txt) return;
      input.value = "";
      resizeInput();
      const res = await api.sendMessage(txt);
      this.showToast(res.message, !res.ok);
    };

    sendBtn?.addEventListener("click", doSend);
    input?.addEventListener("input", resizeInput);
    input?.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        doSend();
      }
    });

    micBtn?.addEventListener("click", async () => {
      if (this.transcribing) return;
      if (!this.recording) {
        try {
          await api.startDictation();
          this.recording = true;
          micBtn.classList.add("recording");
          micBtn.textContent = "⏹️";
          this.showToast("🎙️ Dinleniyor... Bitirmek için tekrar basın.");
        } catch (err) {
          this.showToast(String(err), true);
        }
      } else {
        this.recording = false;
        this.transcribing = true;
        micBtn.classList.remove("recording");
        micBtn.classList.add("transcribing");
        micBtn.textContent = "⏳";
        try {
          const text = await api.stopDictation();
          if (text && input) {
            input.value = input.value ? `${input.value} ${text}` : text;
            input.focus();
            if (this.getSettings().dictation.auto_send) {
              await doSend();
            } else {
              this.showToast("Dikte tamamlandı. Kontrol edip Enter ile gönderebilirsiniz.");
            }
          } else {
            this.showToast("Ses metne çevrilemedi (boş yanıt).", true);
          }
        } catch (err) {
          this.showToast(String(err), true);
        } finally {
          this.transcribing = false;
          micBtn.classList.remove("transcribing");
          micBtn.textContent = "🎙️";
        }
      }
    });
  }

  public tickTimers() {
    const snap = this.lastSnap;
    if (!snap) return;
    const activeIds = Object.keys(snap.active_agents);
    if (activeIds.length === 0) return;
    const nowSec = Date.now() / 1000;

    if (this.focus && this.focus.kind === "agent") {
      const active = snap.active_agents[this.focus.id];
      const statusEl = this.root.querySelector("#focus-status") as HTMLElement | null;
      if (active && statusEl) {
        const text = `● ÇALIŞIYOR ${formatMMSS(nowSec - active.baslangic)}`;
        const pill = statusEl.firstElementChild as HTMLElement | null;
        if (pill && pill.textContent !== text) pill.textContent = text;
      }
      return;
    }
    if (this.focus) return;

    for (const id of activeIds) {
      const active = snap.active_agents[id];
      const stateEl = this.root.querySelector(`#agent-state-${id}`);
      if (!active || !stateEl) continue;
      const text = `${id === "yonetici" ? "MEŞGUL" : "ÇALIŞIYOR"} ${formatMMSS(nowSec - active.baslangic)}`;
      if (this.lastStateTexts.get(id) !== text) {
        this.lastStateTexts.set(id, text);
        stateEl.textContent = text;
      }
    }
  }

  private updateGridView(snap: OfficeSnapshot, ui: AppSettings["ui"]) {
    if (ui.show_events) {
      this.patchLogContainer("#log-events", "events", snap.events_log, "(henüz olay yok)");
    }
    if (ui.show_system) {
      this.patchLogContainer("#log-system", "system", snap.system_log, "(yok)");
    }
    if (ui.show_chat) {
      this.patchChat(snap);
    }

    const nowSec = Date.now() / 1000;
    for (const a of AGENT_DEFS) {
      if (!ui.visible_agents.includes(a.id)) continue;
      const card = this.root.querySelector(`#agent-card-${a.id}`);
      const dot = this.root.querySelector(`#agent-dot-${a.id}`);
      const stateEl = this.root.querySelector(`#agent-state-${a.id}`);
      const metaEl = this.root.querySelector(`#agent-meta-${a.id}`);
      if (!card || !dot || !stateEl || !metaEl) continue;

      const active = snap.active_agents[a.id];
      const assigned = snap.assigned_models[a.id] || "?";

      const nextStateText = active
        ? `${a.id === "yonetici" ? "MEŞGUL" : "ÇALIŞIYOR"} ${formatMMSS(nowSec - active.baslangic)}`
        : "boşta";
      if (this.lastStateTexts.get(a.id) !== nextStateText) {
        this.lastStateTexts.set(a.id, nextStateText);
        card.classList.toggle("is-active", Boolean(active));
        dot.classList.toggle("active", Boolean(active));
        stateEl.textContent = nextStateText;
        stateEl.className = active ? "agent-state-active" : "agent-state-idle";
      }

      const modelText = active
        ? `${active.cli}·${active.model}${active.muadil ? " ⚠ MUADİL" : ""}`
        : assigned;
      const sessionBadge =
        a.id === "yonetici" && snap.manager_session
          ? `<span style="color:var(--text-2)">oturum ${esc(snap.manager_session.slice(0, 8))}</span>`
          : "";
      let ctxHtml = "";
      if (a.id === "yonetici" && snap.manager_context) {
        const mc = snap.manager_context;
        const pct = Math.min(100, Math.round((mc.token / Math.max(1, mc.pencere)) * 100));
        const col = pct < 50 ? "var(--accent)" : pct < 80 ? "var(--amber)" : "var(--red)";
        ctxHtml = `
          <div class="ctx-bar-wrap">
            <span>Bağlam</span>
            <div class="ctx-bar"><div style="width:${pct}%;background:${col}"></div></div>
            <span style="color:${col}">${Math.round(mc.token / 1000)}k (%${pct})</span>
          </div>`;
      }
      const nowTask = active?.aciklama || active?.etiket || "";
      const metaFp = `${modelText}|${sessionBadge}|${ctxHtml}|${nowTask}`;
      if (this.lastMetaFingerprints.get(a.id) !== metaFp) {
        this.lastMetaFingerprints.set(a.id, metaFp);
        metaEl.innerHTML = `
          <div class="agent-meta-row">
            <span>${esc(modelText)}</span>
            ${sessionBadge}
          </div>
          ${ctxHtml}
          ${nowTask ? `<div class="agent-now">▸ ${esc(nowTask)}</div>` : ""}
        `;
      }

      const lines = snap.agent_logs[a.id] || [];
      this.patchLogContainer(
        `#log-agent-${a.id}`,
        `agent:${a.id}`,
        lines,
        "(henüz hareket yok)"
      );
    }
  }

  private matchesFocusFilter(l: string): boolean {
    const q = this.focusSearch;
    if (q && !l.toLowerCase().includes(q)) return false;
    if (this.focusFilter === "err") return l.includes("❌") || l.includes("⏱️") || l.includes("ERROR");
    if (this.focusFilter === "cmd") return l.includes("⚙️");
    if (this.focusFilter === "write") return l.includes("📝") || l.includes("📖");
    if (this.focusFilter === "think") return l.includes("🧠");
    return true;
  }

  private buildFocusLineNode(
    line: string,
    animate: boolean,
    scrollEl: HTMLElement | null
  ): HTMLElement {
    const thought = parseThoughtLine(line);
    if (thought) {
      const card = document.createElement("div");
      card.className = animate ? "log-thought-card stream-enter" : "log-thought-card";
      const head = document.createElement("div");
      head.className = "log-thought-head";
      head.textContent = `🧠 Düşünce · ${thought.ts}`;
      const body = document.createElement("div");
      card.appendChild(head);
      card.appendChild(body);
      if (animate && thought.body.length > 0) {
        this.enqueueStream({ el: body, scrollEl, fullText: thought.body, shown: 0 });
      } else {
        body.textContent = thought.body;
      }
      return card;
    }

    const touched = this.focus?.kind === "agent" ? extractTouchedFile(line) : null;
    const div = document.createElement("div");
    const cls = classifyLogLine(line);
    const formatted = line.replace(/\s*⏎\s*/g, "\n");
    if (touched) {
      div.className = `log-line ${cls} log-file-link${animate ? " stream-enter" : ""}`;
      const textSpan = document.createElement("span");
      const badge = document.createElement("span");
      badge.className = "log-file-badge";
      badge.textContent = "Gör →";
      div.appendChild(textSpan);
      div.appendChild(badge);
      div.addEventListener("click", () => {
        this.selectedFile = touched;
        this.autoFollowFile = false;
        this.root.querySelector("#btn-code-follow")?.classList.remove("primary");
        if (!this.showCodePane) {
          this.showCodePane = true;
          this.root.querySelector("#focus-split")?.classList.remove("single");
          const pane = this.root.querySelector("#focus-code-panel") as HTMLElement | null;
          if (pane) pane.style.display = "";
          this.root.querySelector("#btn-toggle-code")?.classList.add("primary");
        }
        this.codeFingerprint = "";
        this.codeLines = [];
        this.refreshCodePane(true);
      });
      if (animate) {
        this.enqueueStream({ el: textSpan, scrollEl, fullText: formatted, shown: 0 });
      } else {
        textSpan.textContent = formatted;
      }
      return div;
    }

    div.className = `log-line ${cls}${animate ? " stream-enter" : ""}`;
    if (animate) {
      this.enqueueStream({ el: div, scrollEl, fullText: formatted, shown: 0 });
    } else {
      div.textContent = formatted;
    }
    return div;
  }

  private updateFocusView(snap: OfficeSnapshot) {
    if (!this.focus) return;
    if (this.focus.kind === "chat") {
      this.patchChat(snap);
      return;
    }
    const latest =
      this.focus.kind === "events" ? snap.events_log : snap.agent_logs[this.focus.id] || [];
    let newLines: string[] = [];
    if (this.focusLines.length === 0) {
      this.focusLines = latest;
      this.renderFocusLogs();
    } else if (latest.length > 0) {
      const lastExisting = this.focusLines[this.focusLines.length - 1];
      const idx = latest.lastIndexOf(lastExisting);
      if (idx >= 0 && idx < latest.length - 1) {
        newLines = latest.slice(idx + 1);
        this.focusLines = this.focusLines.concat(newLines).slice(-2000);
        this.appendFocusLinesAnimated(newLines);
      } else if (idx === -1 && latest[latest.length - 1] !== lastExisting) {
        this.focusLines = latest;
        this.renderFocusLogs();
      }
    }

    if (this.focus.kind === "agent") {
      const metaEl = this.root.querySelector("#focus-meta") as HTMLElement | null;
      const statusEl = this.root.querySelector("#focus-status") as HTMLElement | null;
      const active = snap.active_agents[this.focus.id];
      const assigned = snap.assigned_models[this.focus.id] || "?";
      if (metaEl) {
        metaEl.style.display = "flex";
        metaEl.innerHTML = `<div class="agent-meta-row"><span>Model: ${esc(
          active ? `${active.cli}·${active.model}` : assigned
        )}</span>${active ? `<span class="agent-now">▸ ${esc(active.aciklama || active.etiket)}</span>` : ""}</div>`;
      }
      if (statusEl) {
        statusEl.innerHTML = active
          ? `<span class="pill ok">● ÇALIŞIYOR ${formatMMSS(Date.now() / 1000 - active.baslangic)}</span>`
          : `<span class="pill">○ boşta</span>`;
      }

      // Live Code/Diff auto-follow & throttled refresh
      if (this.showCodePane) {
        let touchedNewFile = false;
        if (this.autoFollowFile && newLines.length > 0) {
          for (let i = newLines.length - 1; i >= 0; i--) {
            const f = extractTouchedFile(newLines[i]);
            if (f) {
              if (f !== this.selectedFile) {
                this.selectedFile = f;
                this.codeFingerprint = "";
                this.codeLines = [];
              }
              touchedNewFile = true;
              break;
            }
          }
        }
        const now = Date.now();
        if (touchedNewFile || (active && newLines.length > 0 && now - this.lastCodeFetchAt > 1200)) {
          this.refreshCodePane(false);
        } else if (this.lastCodeFetchAt === 0 && snap.all_projects.length > 0) {
          this.refreshCodePane(false);
        }
      }
    }
  }

  private appendFocusLinesAnimated(lines: string[]) {
    const el = this.root.querySelector("#focus-log-body") as HTMLElement | null;
    if (!el || lines.length === 0) return;
    const matched = lines.filter((l) => this.matchesFocusFilter(l));
    if (matched.length === 0) return;

    if (el.firstElementChild?.classList.contains("log-empty")) {
      el.innerHTML = "";
    }
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 48;
    const frag = document.createDocumentFragment();
    // If many lines burst in at once, animate only the last 8 for smooth responsiveness
    const animateFrom = Math.max(0, matched.length - 8);
    for (let i = 0; i < matched.length; i++) {
      frag.appendChild(this.buildFocusLineNode(matched[i], i >= animateFrom, el));
    }
    el.appendChild(frag);
    while (el.children.length > 2000) {
      el.firstElementChild?.remove();
    }
    if (atBottom) el.scrollTop = el.scrollHeight;
  }

  private renderFocusLogs() {
    this.cancelStreams();
    const el = this.root.querySelector("#focus-log-body") as HTMLElement | null;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 36 || el.scrollTop === 0;
    const filtered = this.focusLines.filter((l) => this.matchesFocusFilter(l));
    if (!filtered.length) {
      el.innerHTML = `<div class="log-empty">Eşleşen kayıt yok.</div>`;
      return;
    }
    const frag = document.createDocumentFragment();
    for (const l of filtered) {
      frag.appendChild(this.buildFocusLineNode(l, false, el));
    }
    el.replaceChildren(frag);
    if (atBottom) el.scrollTop = el.scrollHeight;
  }

  private async refreshCodePane(force: boolean) {
    if (!this.focus || this.focus.kind !== "agent" || !this.showCodePane) return;
    if (this.codeFetchInFlight && !force) return;
    const proj = this.lastSnap?.active_project || this.lastSnap?.all_projects[0] || null;
    const projTag = this.root.querySelector("#code-project-tag");
    const body = this.root.querySelector("#focus-code-body") as HTMLElement | null;
    const metaInfo = this.root.querySelector("#code-meta-info");
    if (!proj) {
      if (projTag) projTag.textContent = "";
      if (body) body.innerHTML = `<div class="log-empty">Aktif proje yok — proje başladığında dosya ve diff burada görünecek.</div>`;
      return;
    }
    if (projTag) projTag.textContent = proj.ad;

    this.codeFetchInFlight = true;
    this.lastCodeFetchAt = Date.now();
    try {
      if (this.codeMode === "file" && this.selectedFile) {
        const res = await api.getProjectFile(proj.klasor, this.selectedFile);
        if (!this.focus || this.focus.kind !== "agent") return;
        if (!res.ok) {
          if (body) body.innerHTML = `<div class="log-empty">${esc(res.error || "Dosya okunamadı.")}</div>`;
          if (metaInfo) metaInfo.textContent = "";
          return;
        }
        this.syncFileDropdown(this.changedFiles, res.path);
        if (metaInfo) {
          metaInfo.textContent = `${res.total_lines} satır${res.truncated ? " (kısmi)" : ""}`;
        }
        this.renderCodeLinesSmooth(res.lines, false);
      } else {
        const res = await api.getProjectDiff(proj.klasor, this.selectedFile);
        if (!this.focus || this.focus.kind !== "agent") return;
        if (!res.ok) {
          if (body) body.innerHTML = `<div class="log-empty">${esc(res.error || "Diff alınamadı.")}</div>`;
          if (metaInfo) metaInfo.textContent = "";
          return;
        }
        this.changedFiles = res.changed_files;
        this.syncFileDropdown(res.changed_files, this.selectedFile);
        if (metaInfo) {
          metaInfo.textContent = `${res.changed_files.length} değişen dosya`;
        }
        if (res.lines.length === 0) {
          this.codeFingerprint = "empty";
          this.codeLines = [];
          if (body) {
            body.innerHTML = `<div class="log-empty">${
              this.selectedFile
                ? `${esc(this.selectedFile)} için bekleyen git diff yok (Dosya sekmesine geçerek içeriği görebilirsiniz).`
                : "Henüz kaydedilmiş bir git diff yok."
            }</div>`;
          }
          return;
        }
        this.renderCodeLinesSmooth(res.lines, true);
      }
    } finally {
      this.codeFetchInFlight = false;
    }
  }

  private syncFileDropdown(files: string[], current: string | null) {
    const sel = this.root.querySelector("#code-file-select") as HTMLSelectElement | null;
    if (!sel) return;
    const opts = new Set<string>(files);
    if (current) opts.add(current);
    const list = Array.from(opts);
    const fp = `${current || ""}|${list.join(",")}`;
    if (sel.getAttribute("data-fp") === fp) return;
    sel.setAttribute("data-fp", fp);
    sel.innerHTML =
      `<option value="">Tüm değişen dosyalar (git diff)</option>` +
      list
        .map(
          (f) =>
            `<option value="${esc(f)}" ${f === current ? "selected" : ""}>${esc(f)}</option>`
        )
        .join("");
  }

  private renderCodeLinesSmooth(nextLines: string[], isDiff: boolean) {
    const body = this.root.querySelector("#focus-code-body") as HTMLElement | null;
    if (!body) return;
    const fp = `${isDiff ? "d" : "f"}:${nextLines.length}:${nextLines[0] || ""}:${nextLines[nextLines.length - 1] || ""}`;
    if (fp === this.codeFingerprint) return;

    const prevLines = this.codeLines;
    const prevSet = new Set(prevLines);
    const hadPrevious = prevLines.length > 0;
    this.codeLines = nextLines;
    this.codeFingerprint = fp;

    const atBottom = body.scrollHeight - body.scrollTop - body.clientHeight < 48 || body.scrollTop === 0;
    const frag = document.createDocumentFragment();
    let animatedCount = 0;
    const MAX_ANIMATED_LINES = 18;

    for (let i = 0; i < nextLines.length; i++) {
      const text = nextLines[i];
      const row = document.createElement("div");
      let diffCls = "";
      if (isDiff) {
        if (text.startsWith("+++") || text.startsWith("---") || text.startsWith("diff --git") || text.startsWith("index ")) {
          diffCls = "diff-meta";
        } else if (text.startsWith("@@")) {
          diffCls = "diff-hunk";
        } else if (text.startsWith("+")) {
          diffCls = "diff-add";
        } else if (text.startsWith("-")) {
          diffCls = "diff-del";
        }
      }
      const isFresh =
        hadPrevious
          ? !prevSet.has(text) && text.trim().length > 0
          : isDiff
            ? diffCls === "diff-add"
            : false;

      row.className = `code-line ${diffCls}${isFresh ? " code-line-fresh stream-enter" : ""}`;
      const ln = document.createElement("span");
      ln.className = "code-ln";
      ln.textContent = String(i + 1);
      const codeSpan = document.createElement("span");
      codeSpan.className = "code-text";
      row.appendChild(ln);
      row.appendChild(codeSpan);

      if (isFresh && animatedCount < MAX_ANIMATED_LINES) {
        animatedCount += 1;
        this.enqueueStream({ el: codeSpan, scrollEl: body, fullText: text, shown: 0 });
      } else {
        codeSpan.textContent = text;
      }
      frag.appendChild(row);
    }

    body.replaceChildren(frag);
    if (atBottom) body.scrollTop = body.scrollHeight;
  }

  private patchLogContainer(selector: string, key: string, lines: string[], emptyMsg: string) {
    const el = this.root.querySelector(selector) as HTMLElement | null;
    if (!el) return;
    const first = lines[0] || "";
    const last = lines[lines.length - 1] || "";
    const prev = this.lastLogTail.get(key);
    if (prev && prev.len === lines.length && prev.first === first && prev.last === last) return;

    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40 || el.scrollTop === 0;

    if (lines.length === 0) {
      el.innerHTML = `<div class="log-empty">${esc(emptyMsg)}</div>`;
      this.lastLogTail.set(key, { len: 0, first: "", last: "" });
      return;
    }

    // Incremental append when new lines arrived at the tail (avoids full innerHTML rebuild)
    if (prev && prev.len > 0 && el.children.length > 0 && !el.firstElementChild?.classList.contains("log-empty")) {
      const idx = lines.lastIndexOf(prev.last);
      if (idx >= 0 && idx < lines.length - 1 && lines.length - 1 - idx <= 30) {
        const frag = document.createDocumentFragment();
        for (let i = idx + 1; i < lines.length; i++) {
          const div = document.createElement("div");
          const cls = classifyLogLine(lines[i]);
          div.className = cls ? `log-line ${cls}` : "log-line";
          div.textContent = lines[i];
          frag.appendChild(div);
        }
        el.appendChild(frag);
        while (el.children.length > lines.length) {
          el.firstElementChild?.remove();
        }
        this.lastLogTail.set(key, { len: lines.length, first, last });
        if (atBottom) el.scrollTop = el.scrollHeight;
        return;
      }
    }

    const frag = document.createDocumentFragment();
    for (let i = 0; i < lines.length; i++) {
      const div = document.createElement("div");
      const cls = classifyLogLine(lines[i]);
      div.className = cls ? `log-line ${cls}` : "log-line";
      div.textContent = lines[i];
      frag.appendChild(div);
    }
    el.replaceChildren(frag);
    this.lastLogTail.set(key, { len: lines.length, first, last });
    if (atBottom) el.scrollTop = el.scrollHeight;
  }

  private buildMsgNode(m: OfficeSnapshot["messages"][number]): HTMLElement {
    const cls = m.kaynak === "sahip" ? "sahip" : m.kaynak === "yonetici" ? "yonetici" : "sistem";
    const icon = m.kanal === "telegram" ? "📱" : "🖥";
    const who = m.kaynak === "sahip" ? "Siz" : m.kaynak === "yonetici" ? "Yönetici" : "Sistem";
    const row = document.createElement("div");
    row.className = `msg-row ${cls}`;
    row.innerHTML = `
      <div class="msg-head"><span class="who-${cls}">${who}</span><span>${icon} ${formatTimeHM(m.zaman)}</span></div>
      <div class="msg-text">${linkifyText(m.metin)}</div>
    `;
    return row;
  }

  private patchChat(snap: OfficeSnapshot) {
    const pill = this.root.querySelector("#chat-queue-pill") as HTMLElement | null;
    if (pill) {
      if (snap.message_queue_count > 0) {
        pill.style.display = "inline-flex";
        pill.textContent = `Sırada ${snap.message_queue_count} mesaj`;
      } else {
        pill.style.display = "none";
      }
    }

    const body = this.root.querySelector("#chat-body") as HTMLElement | null;
    if (!body) return;
    const lastId = snap.messages.length ? snap.messages[snap.messages.length - 1].id : 0;
    if (this.lastMsgCount === lastId) return;
    const prevLastId = this.lastMsgCount;
    this.lastMsgCount = lastId;

    const atBottom = body.scrollHeight - body.scrollTop - body.clientHeight < 60 || body.scrollTop === 0;
    if (!snap.messages.length) {
      body.innerHTML = `<div class="log-empty">Henüz konuşma yok — aşağıdan yazın veya mikrofonla dikte edin.</div>`;
      return;
    }

    // Incremental append if we already rendered previous messages
    if (prevLastId > 0 && lastId > prevLastId && !body.firstElementChild?.classList.contains("log-empty")) {
      const frag = document.createDocumentFragment();
      for (const m of snap.messages) {
        if (m.id > prevLastId) {
          frag.appendChild(this.buildMsgNode(m));
        }
      }
      body.appendChild(frag);
      while (body.children.length > 80) {
        body.firstElementChild?.remove();
      }
      if (atBottom) body.scrollTop = body.scrollHeight;
      return;
    }

    const frag = document.createDocumentFragment();
    for (const m of snap.messages) {
      frag.appendChild(this.buildMsgNode(m));
    }
    body.replaceChildren(frag);
    if (atBottom) body.scrollTop = body.scrollHeight;
  }
}
