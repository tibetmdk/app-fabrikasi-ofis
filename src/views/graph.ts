import { api } from "../api";
import type { GraphData, GraphNode, NoteContent } from "../types";
import { esc, renderMarkdown } from "../utils";

interface SimNode extends GraphNode {
  x: number;
  y: number;
  vx: number;
  vy: number;
}

export class GraphView {
  private root: HTMLElement;
  private data: GraphData | null = null;
  private nodes: SimNode[] = [];
  private nodeMap = new Map<string, SimNode>();
  private hiddenGroups = new Set<string>();
  private searchQuery = "";
  private selectedId: string | null = null;
  private hoveredId: string | null = null;
  private selectedNote: NoteContent | null = null;

  private canvas: HTMLCanvasElement | null = null;
  private ctx: CanvasRenderingContext2D | null = null;
  private rafId: number | null = null;
  private camX = 0;
  private camY = 0;
  private zoom = 1;
  private draggingNode: SimNode | null = null;
  private panning = false;
  private moved = false;
  private lastMouseX = 0;
  private lastMouseY = 0;
  private energy = 1;

  constructor(container: HTMLElement) {
    this.root = container;
  }

  async mount() {
    this.root.innerHTML = `
      <div class="graph-layout no-note" id="graph-layout">
        <div class="graph-stage" id="graph-stage">
          <canvas class="graph-canvas" id="graph-canvas"></canvas>
          <div class="graph-overlay-top">
            <div class="graph-controls">
              <input class="search-input" id="graph-search" placeholder="Hafızada ara..." style="min-width:160px" />
              <button class="btn sm" id="graph-fit">Ortala</button>
              <button class="btn sm" id="graph-reload">↻ Yenile</button>
            </div>
            <div class="graph-controls" id="graph-legend"></div>
          </div>
        </div>
        <div class="note-drawer" id="note-drawer" style="display:none"></div>
      </div>
    `;

    this.canvas = this.root.querySelector("#graph-canvas");
    this.ctx = this.canvas?.getContext("2d") || null;

    this.root.querySelector("#graph-search")?.addEventListener("input", (e) => {
      this.searchQuery = (e.target as HTMLInputElement).value.trim().toLowerCase();
      this.requestRedraw();
    });
    this.root.querySelector("#graph-fit")?.addEventListener("click", () => {
      this.resetCamera();
      this.requestRedraw();
    });
    this.root.querySelector("#graph-reload")?.addEventListener("click", () => this.loadData());

    this.bindCanvasEvents();
    this.resizeCanvas();
    window.addEventListener("resize", this.resizeCanvas);

    await this.loadData();
    this.startLoop();
  }

  unmount() {
    if (this.rafId !== null) {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
    }
    window.removeEventListener("resize", this.resizeCanvas);
    window.removeEventListener("mousemove", this.handleMouseMove);
    window.removeEventListener("mouseup", this.handleMouseUp);
    if (this.canvas) {
      this.canvas.width = 0;
      this.canvas.height = 0;
    }
    this.ctx = null;
    this.canvas = null;
    this.data = null;
    this.nodes = [];
    this.nodeMap.clear();
    this.selectedNote = null;
    this.draggingNode = null;
  }

  private resizeCanvas = () => {
    if (!this.canvas) return;
    const parent = this.canvas.parentElement;
    if (!parent) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const w = parent.clientWidth;
    const h = parent.clientHeight;
    this.canvas.width = w * dpr;
    this.canvas.height = h * dpr;
    if (this.ctx) this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.requestRedraw();
  };

  private async loadData() {
    try {
      this.data = await api.getGraph();
      this.initSimulation();
      this.renderLegend();
      this.resetCamera();
      this.startLoop();
    } catch (err) {
      const stage = this.root.querySelector("#graph-stage");
      if (stage) {
        const msg = document.createElement("div");
        msg.className = "toast err";
        msg.textContent = `Hafıza grafı yüklenemedi: ${err}`;
        stage.appendChild(msg);
      }
    }
  }

  private initSimulation() {
    if (!this.data) return;
    const count = this.data.nodes.length;
    this.nodes = this.data.nodes.map((n, i) => {
      const angle = (i / Math.max(1, count)) * Math.PI * 2;
      const r = 110 + (i % 5) * 35;
      return {
        ...n,
        x: Math.cos(angle) * r,
        y: Math.sin(angle) * r,
        vx: 0,
        vy: 0,
      };
    });
    this.nodeMap.clear();
    for (const n of this.nodes) this.nodeMap.set(n.id, n);
    this.energy = 1.0;
    // İlk açılışta düğümlerin sağa sola fırlamaması için simülasyonu arka planda oturt
    for (let s = 0; s < 180; s++) {
      this.stepPhysics();
    }
    this.energy = 0.06;
  }

  private resetCamera() {
    this.camX = 0;
    this.camY = 0;
    this.zoom = 1;
  }

  private renderLegend() {
    const el = this.root.querySelector("#graph-legend");
    if (!el || !this.data) return;
    el.innerHTML = this.data.groups
      .map(
        (g) => `
      <span class="legend-chip ${this.hiddenGroups.has(g.name) ? "off" : ""}" data-group="${esc(g.name)}">
        <span class="legend-dot" style="background:${esc(g.color)}"></span>
        ${esc(g.name)}
      </span>`
      )
      .join("");
    el.querySelectorAll(".legend-chip").forEach((chip) => {
      chip.addEventListener("click", () => {
        const g = chip.getAttribute("data-group") || "";
        if (this.hiddenGroups.has(g)) this.hiddenGroups.delete(g);
        else this.hiddenGroups.add(g);
        this.renderLegend();
        this.requestRedraw();
      });
    });
  }

  private screenToWorld(sx: number, sy: number): [number, number] {
    if (!this.canvas) return [0, 0];
    const rect = this.canvas.getBoundingClientRect();
    const wx = (sx - rect.width / 2) / this.zoom + this.camX;
    const wy = (sy - rect.height / 2) / this.zoom + this.camY;
    return [wx, wy];
  }

  private findNodeAt(wx: number, wy: number): SimNode | null {
    for (let i = this.nodes.length - 1; i >= 0; i--) {
      const n = this.nodes[i];
      if (this.hiddenGroups.has(n.group)) continue;
      const r = 6 + Math.min(10, n.degree * 1.4) + 5 / this.zoom;
      const dx = wx - n.x;
      const dy = wy - n.y;
      if (dx * dx + dy * dy <= r * r) return n;
    }
    return null;
  }

  private handleMouseMove = (e: MouseEvent) => {
    if (!this.canvas) return;
    const rect = this.canvas.getBoundingClientRect();
    if (this.draggingNode) {
      this.moved = true;
      const [wx, wy] = this.screenToWorld(e.clientX - rect.left, e.clientY - rect.top);
      this.draggingNode.x = wx;
      this.draggingNode.y = wy;
      this.draggingNode.vx = 0;
      this.draggingNode.vy = 0;
      this.requestRedraw();
    } else if (this.panning) {
      this.moved = true;
      this.camX -= (e.clientX - this.lastMouseX) / this.zoom;
      this.camY -= (e.clientY - this.lastMouseY) / this.zoom;
      this.lastMouseX = e.clientX;
      this.lastMouseY = e.clientY;
      this.requestRedraw();
    } else {
      const [wx, wy] = this.screenToWorld(e.clientX - rect.left, e.clientY - rect.top);
      const hit = this.findNodeAt(wx, wy);
      const nextHover = hit ? hit.id : null;
      if (nextHover !== this.hoveredId) {
        this.hoveredId = nextHover;
        this.canvas.style.cursor = hit ? "pointer" : "grab";
        this.requestRedraw();
      }
    }
  };

  private handleMouseUp = () => {
    if (this.draggingNode && !this.moved) {
      this.openNote(this.draggingNode.id);
    }
    this.draggingNode = null;
    this.panning = false;
  };

  private bindCanvasEvents() {
    if (!this.canvas) return;
    const c = this.canvas;

    c.addEventListener("mousedown", (e) => {
      const rect = c.getBoundingClientRect();
      const sx = e.clientX - rect.left;
      const sy = e.clientY - rect.top;
      const [wx, wy] = this.screenToWorld(sx, sy);
      const hit = this.findNodeAt(wx, wy);
      this.moved = false;
      this.lastMouseX = e.clientX;
      this.lastMouseY = e.clientY;
      if (hit) {
        this.draggingNode = hit;
        this.energy = Math.max(this.energy, 0.35);
        this.startLoop();
      } else {
        this.panning = true;
        this.startLoop();
      }
    });

    window.addEventListener("mousemove", this.handleMouseMove);
    window.addEventListener("mouseup", this.handleMouseUp);

    c.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        const factor = e.deltaY < 0 ? 1.12 : 0.89;
        this.zoom = Math.max(0.25, Math.min(3.5, this.zoom * factor));
        this.requestRedraw();
      },
      { passive: false }
    );
  }

  private async openNote(noteId: string) {
    this.selectedId = noteId;
    try {
      this.selectedNote = await api.getNote(noteId);
      this.renderNoteDrawer();
    } catch {
      // Try matching by label stem
      const found = this.nodes.find(
        (n) => n.label.toLowerCase() === noteId.toLowerCase() || n.id.endsWith("/" + noteId)
      );
      if (found && found.id !== noteId) {
        this.selectedId = found.id;
        this.selectedNote = await api.getNote(found.id).catch(() => null);
        this.renderNoteDrawer();
      }
    }
  }

  private renderNoteDrawer() {
    const layout = this.root.querySelector("#graph-layout");
    const drawer = this.root.querySelector("#note-drawer") as HTMLElement | null;
    if (!layout || !drawer) return;

    if (!this.selectedNote) {
      layout.classList.add("no-note");
      drawer.style.display = "none";
      this.resizeCanvas();
      return;
    }

    layout.classList.remove("no-note");
    drawer.style.display = "flex";
    const n = this.selectedNote;

    const backlinksHtml = n.backlinks.length
      ? `<div style="margin-top:14px;padding-top:10px;border-top:1px solid var(--border)">
          <div style="font-size:11px;color:var(--text-2);margin-bottom:6px">Gelen Bağlantılar (${n.backlinks.length})</div>
          <div style="display:flex;flex-wrap:wrap;gap:5px">
            ${n.backlinks.map((b) => `<span class="wikilink" data-wiki="${esc(b)}">${esc(b.split("/").pop() || b)}</span>`).join("")}
          </div>
        </div>`
      : "";

    drawer.innerHTML = `
      <div class="panel-header">
        <div class="panel-title">📄 ${esc(n.title)} <span class="panel-key">${esc(n.folder)}</span></div>
        <button class="icon-btn" id="close-note">✕</button>
      </div>
      <div class="note-body">
        ${renderMarkdown(n.content)}
        ${backlinksHtml}
      </div>
    `;

    drawer.querySelector("#close-note")?.addEventListener("click", () => {
      this.selectedNote = null;
      this.selectedId = null;
      this.renderNoteDrawer();
    });
    drawer.querySelectorAll(".wikilink").forEach((el) => {
      el.addEventListener("click", () => {
        const target = el.getAttribute("data-wiki");
        if (target) this.openNote(target);
      });
    });

    this.resizeCanvas();
  }

  private stepPhysics() {
    if (!this.data || this.energy < 0.004) return;
    const nodes = this.nodes;
    const nLen = nodes.length;
    const repel = 2400;
    const centerStrength = 0.014;
    const linkDist = 115;
    const linkStrength = 0.035;

    for (let i = 0; i < nLen; i++) {
      const a = nodes[i];
      if (a === this.draggingNode) continue;
      // Center gravity
      a.vx += -a.x * centerStrength;
      a.vy += -a.y * centerStrength;

      for (let j = i + 1; j < nLen; j++) {
        const b = nodes[j];
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let dist2 = dx * dx + dy * dy;
        if (dist2 < 1) dist2 = 1;
        const dist = Math.sqrt(dist2);
        const f = repel / dist2;
        const fx = (dx / dist) * f;
        const fy = (dy / dist) * f;
        a.vx -= fx;
        a.vy -= fy;
        if (b !== this.draggingNode) {
          b.vx += fx;
          b.vy += fy;
        }
      }
    }

    for (const e of this.data.edges) {
      const a = this.nodeMap.get(e.source);
      const b = this.nodeMap.get(e.target);
      if (!a || !b) continue;
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const dist = Math.sqrt(dx * dx + dy * dy) || 1;
      const diff = (dist - linkDist) * linkStrength;
      const fx = (dx / dist) * diff;
      const fy = (dy / dist) * diff;
      if (a !== this.draggingNode) { a.vx += fx; a.vy += fy; }
      if (b !== this.draggingNode) { b.vx -= fx; b.vy -= fy; }
    }

    const maxV = 18;
    for (const n of nodes) {
      if (n === this.draggingNode) continue;
      n.vx = Math.max(-maxV, Math.min(maxV, n.vx * 0.82));
      n.vy = Math.max(-maxV, Math.min(maxV, n.vy * 0.82));
      n.x += n.vx * this.energy;
      n.y += n.vy * this.energy;
    }
    this.energy *= 0.992;
  }

  private draw() {
    if (!this.canvas || !this.ctx || !this.data) return;
    const ctx = this.ctx;
    const rect = this.canvas.getBoundingClientRect();
    const w = rect.width;
    const h = rect.height;

    ctx.clearRect(0, 0, w, h);
    ctx.save();
    ctx.translate(w / 2, h / 2);
    ctx.scale(this.zoom, this.zoom);
    ctx.translate(-this.camX, -this.camY);

    const focusId = this.hoveredId || this.selectedId;
    const neighbors = new Set<string>();
    if (focusId) {
      neighbors.add(focusId);
      for (const e of this.data.edges) {
        if (e.source === focusId) neighbors.add(e.target);
        if (e.target === focusId) neighbors.add(e.source);
      }
    }

    // Draw edges
    for (const e of this.data.edges) {
      const a = this.nodeMap.get(e.source);
      const b = this.nodeMap.get(e.target);
      if (!a || !b || this.hiddenGroups.has(a.group) || this.hiddenGroups.has(b.group)) continue;
      const isHi = focusId && (e.source === focusId || e.target === focusId);
      ctx.strokeStyle = isHi ? "rgba(56, 189, 248, 0.72)" : "rgba(148, 163, 184, 0.18)";
      ctx.lineWidth = isHi ? 1.6 / this.zoom : 1 / this.zoom;
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      ctx.stroke();

      if (this.data.show_arrow) {
        const angle = Math.atan2(b.y - a.y, b.x - a.x);
        const rTarget = 6 + Math.min(10, b.degree * 1.4) + 3;
        const ax = b.x - Math.cos(angle) * rTarget;
        const ay = b.y - Math.sin(angle) * rTarget;
        const len = 6;
        ctx.fillStyle = ctx.strokeStyle;
        ctx.beginPath();
        ctx.moveTo(ax, ay);
        ctx.lineTo(ax - len * Math.cos(angle - 0.4), ay - len * Math.sin(angle - 0.4));
        ctx.lineTo(ax - len * Math.cos(angle + 0.4), ay - len * Math.sin(angle + 0.4));
        ctx.closePath();
        ctx.fill();
      }
    }

    // Draw nodes
    for (const n of this.nodes) {
      if (this.hiddenGroups.has(n.group)) continue;
      const r = 6 + Math.min(10, n.degree * 1.4);
      const matchesSearch =
        !this.searchQuery ||
        n.label.toLowerCase().includes(this.searchQuery) ||
        n.folder.toLowerCase().includes(this.searchQuery);
      const dimmed = (focusId && !neighbors.has(n.id)) || !matchesSearch;

      ctx.globalAlpha = dimmed ? 0.2 : 1;
      ctx.beginPath();
      ctx.arc(n.x, n.y, r, 0, Math.PI * 2);
      ctx.fillStyle = n.color;
      ctx.fill();

      if (n.id === this.selectedId || n.id === this.hoveredId) {
        ctx.strokeStyle = "#ffffff";
        ctx.lineWidth = 2 / this.zoom;
        ctx.stroke();
      }

      if (this.zoom > 0.55 || n.id === focusId || (this.searchQuery && matchesSearch)) {
        ctx.font = `${11 / Math.sqrt(this.zoom)}px Inter, sans-serif`;
        ctx.fillStyle = "#e2e8f0";
        ctx.textAlign = "center";
        ctx.fillText(n.label, n.x, n.y + r + 12 / Math.sqrt(this.zoom));
      }
    }
    ctx.globalAlpha = 1;
    ctx.restore();
  }

  private startLoop() {
    if (this.rafId !== null) return;
    const tick = () => {
      this.stepPhysics();
      this.draw();
      if (this.energy >= 0.004 || this.draggingNode || this.panning) {
        this.rafId = requestAnimationFrame(tick);
      } else {
        this.rafId = null;
      }
    };
    this.rafId = requestAnimationFrame(tick);
  }

  private requestRedraw() {
    if (this.rafId === null) {
      this.rafId = requestAnimationFrame(() => {
        this.rafId = null;
        this.draw();
      });
    }
  }
}
