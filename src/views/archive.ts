import { api } from "../api";
import type { ArchiveOverview } from "../types";
import { classifyLogLine, esc } from "../utils";

export class ArchiveView {
  private root: HTMLElement;
  private data: ArchiveOverview | null = null;
  private selectedMode: "journal" | "file" = "journal";
  private selectedArchive = "";
  private selectedFile = "";
  private fileLines: string[] = [];
  private searchQuery = "";

  constructor(container: HTMLElement) {
    this.root = container;
  }

  async mount() {
    this.root.innerHTML = `<div class="page-scroll"><div class="card">Arşiv yükleniyor...</div></div>`;
    try {
      this.data = await api.getArchives();
      this.renderShell();
      this.renderLines();
    } catch (e) {
      this.root.innerHTML = `<div class="page-scroll"><div class="card">Arşiv yüklenemedi: ${esc(String(e))}</div></div>`;
    }
  }

  unmount() {
    this.data = null;
    this.fileLines = [];
  }

  private async openArchiveFile(archive: string, file: string) {
    this.selectedMode = "file";
    this.selectedArchive = archive;
    this.selectedFile = file;
    this.fileLines = await api.getArchiveFile(archive, file);
    this.updateActiveSelection();
    this.renderLines();
  }

  private updateActiveSelection() {
    const jEl = this.root.querySelector("#arc-journal");
    jEl?.classList.toggle("active", this.selectedMode === "journal");
    this.root.querySelectorAll("[data-arc]").forEach((el) => {
      const a = el.getAttribute("data-arc") || "";
      const f = el.getAttribute("data-file") || "";
      el.classList.toggle(
        "active",
        this.selectedMode === "file" && this.selectedArchive === a && this.selectedFile === f
      );
    });
    const titleEl = this.root.querySelector("#arc-title");
    if (titleEl) {
      titleEl.textContent =
        this.selectedMode === "journal"
          ? "📓 Yönetici Günlüğü (yonetici-gunlugu.md)"
          : `📁 ${this.selectedArchive} / ${this.selectedFile}`;
    }
  }

  private renderLines() {
    if (!this.data) return;
    const body = this.root.querySelector("#arc-log-body");
    if (!body) return;
    const lines = this.selectedMode === "journal" ? this.data.journal : this.fileLines;
    const q = this.searchQuery.toLowerCase();
    const filtered = q ? lines.filter((l) => l.toLowerCase().includes(q)) : lines;
    body.innerHTML = filtered.length
      ? filtered
          .map((l) => `<div class="log-line ${classifyLogLine(l)}">${esc(l)}</div>`)
          .join("")
      : `<div class="log-empty">Kayıt bulunamadı.</div>`;
  }

  private renderShell() {
    if (!this.data) return;
    const title =
      this.selectedMode === "journal"
        ? "📓 Yönetici Günlüğü (yonetici-gunlugu.md)"
        : `📁 ${this.selectedArchive} / ${this.selectedFile}`;

    this.root.innerHTML = `
      <div class="archive-layout">
        <div class="archive-sidebar">
          <div style="font-size:11px;font-weight:700;color:var(--text-2);padding:4px 6px">KALICI KAYITLAR</div>
          <div class="archive-item ${this.selectedMode === "journal" ? "active" : ""}" id="arc-journal">
            📓 Yönetici Günlüğü (${this.data.journal.length})
          </div>
          <div style="font-size:11px;font-weight:700;color:var(--text-2);padding:8px 6px 2px">PROJE ARŞİVLERİ (${this.data.archives.length})</div>
          ${
            this.data.archives.length
              ? this.data.archives
                  .map(
                    (a) => `
                  <div style="margin-bottom:6px">
                    <div style="font-size:11.5px;font-weight:700;color:var(--text-0);padding:4px 6px">${esc(a.name)}</div>
                    ${a.files
                      .map(
                        (f) => `
                      <div class="archive-item" data-arc="${esc(a.name)}" data-file="${esc(f)}" style="padding-left:16px">
                        📄 ${esc(f)}
                      </div>`
                      )
                      .join("")}
                  </div>`
                  )
                  .join("")
              : `<div style="padding:6px;color:var(--text-2);font-size:11.5px">Henüz arşivlenmiş proje yok.</div>`
          }
        </div>
        <div class="panel">
          <div class="panel-header">
            <div class="panel-title" id="arc-title">${esc(title)}</div>
            <input class="search-input" id="arc-search" value="${esc(this.searchQuery)}" placeholder="Satırlarda ara..." />
          </div>
          <div class="log-body" id="arc-log-body"></div>
        </div>
      </div>
    `;

    this.root.querySelector("#arc-journal")?.addEventListener("click", () => {
      this.selectedMode = "journal";
      this.updateActiveSelection();
      this.renderLines();
    });
    this.root.querySelectorAll("[data-arc]").forEach((el) => {
      el.addEventListener("click", () => {
        const a = el.getAttribute("data-arc") || "";
        const f = el.getAttribute("data-file") || "";
        this.openArchiveFile(a, f);
      });
    });
    const search = this.root.querySelector("#arc-search") as HTMLInputElement | null;
    search?.addEventListener("input", () => {
      this.searchQuery = search.value;
      this.renderLines();
    });
  }
}
