// shared/bugReporter.js
// -----------------------------------------------------------------------------
// Floating bug-report widget for the Ninja Workflow Tool sidepanel, options
// page, and individual views. Adapted from the ctox business-basic bug-report
// widget — vanilla JS, persistence via RxDB (bug_reports + bug_report_chunks)
// to mirror the candidate-photo chunking pattern, so reports replicate via the
// existing P2P sync. Submission to the backend happens from the "Bug Reports"
// tab in options.html.
// -----------------------------------------------------------------------------

let _rxdbClientPromise = null;
async function rxdb() {
  if (!_rxdbClientPromise) {
    // Lazy import — only paid for once per page, and only if a report is saved.
    _rxdbClientPromise = import(chrome.runtime.getURL("data/rxdbClient.js"))
      .catch((err) => {
        console.warn("[bugReporter] rxdbClient import failed", err);
        _rxdbClientPromise = null;
        throw err;
      });
  }
  return _rxdbClientPromise;
}

// Module-level state -----------------------------------------------------------

let initialized = false;
let host = null;          // outer container appended to document.body
let fabBtn = null;
let modalEl = null;
let overlayEl = null;
let toastEl = null;

let modalOpen = false;
let summary = "";
let expected = "";
let severity = "medium";
let kind = "bug";
let attachment = null;    // { rect, strokes, screenshotDataUrl, compositeDataUrl, ... }

// Markup state machine ---------------------------------------------------------
// modes: "idle" | "selecting" | "drawing"
let markupMode = "idle";
let selectionOrigin = null;   // {x,y}
let selectionRect = null;     // {x,y,width,height}
let strokes = [];             // [[ {x,y}, ... ], ...]
let activeStroke = null;
let savingMarkup = false;

// ----------------------------------------------------------------------------
// Locale
// ----------------------------------------------------------------------------

function detectLocale() {
  try {
    const docLang = (document?.documentElement?.lang || "").slice(0, 2).toLowerCase();
    if (docLang === "de" || docLang === "en") return docLang;
    const uiLang = (chrome?.i18n?.getUILanguage?.() || "").slice(0, 2).toLowerCase();
    if (uiLang === "de" || uiLang === "en") return uiLang;
  } catch {}
  return "de";
}

const COPY = {
  de: {
    fab: "Bug / Feature melden",
    title: "Fehler melden / Feature hinzufügen",
    close: "Schließen",
    kind: "Typ",
    kindBug: "Bug",
    kindFeature: "Feature",
    severity: "Schweregrad",
    severityLow: "Niedrig",
    severityMedium: "Mittel",
    severityHigh: "Hoch",
    summary: "Was ist falsch / welches Feature wünschst du dir?",
    summaryPlaceholder: "Beschreibe das Anliegen kurz.",
    expected: "Was hast du erwartet?",
    expectedPlaceholder: "Beschreibe das erwartete Verhalten.",
    markArea: "Screenshot + Markup",
    markupTitle: "Bereich auswählen und markieren",
    markupHint: "Ziehe einen Bereich auf. Danach kannst du mit dem Stift darauf zeichnen.",
    cancelMarkup: "Abbrechen",
    saveMarkup: "Übernehmen",
    clearMarkup: "Löschen",
    attachmentScreen: "Screenshot mit Markup",
    attachmentFallback: "Nur Markup gespeichert. Bildschirmfreigabe wurde nicht übernommen.",
    removeAttachment: "Markup entfernen",
    save: "In Sammlung speichern",
    saving: "Speichern …",
    saved: "Gespeichert",
    failed: "Speichern fehlgeschlagen",
    queueHint: "Reports werden in Optionen → „Bug Reports“ gesammelt und können dort als Schwung versendet werden.",
    openList: "Bug-Liste öffnen",
  },
  en: {
    fab: "Report bug / feature",
    title: "Report bug / add feature",
    close: "Close",
    kind: "Type",
    kindBug: "Bug",
    kindFeature: "Feature",
    severity: "Severity",
    severityLow: "Low",
    severityMedium: "Medium",
    severityHigh: "High",
    summary: "What is wrong / which feature do you want?",
    summaryPlaceholder: "Briefly describe the issue.",
    expected: "What did you expect?",
    expectedPlaceholder: "Describe the expected behavior.",
    markArea: "Screenshot + markup",
    markupTitle: "Select area and draw",
    markupHint: "Drag over the area. Then draw on it with the pen.",
    cancelMarkup: "Cancel",
    saveMarkup: "Attach",
    clearMarkup: "Clear",
    attachmentScreen: "Screenshot with markup",
    attachmentFallback: "Only markup saved. Screen capture was not attached.",
    removeAttachment: "Remove markup",
    save: "Save to collection",
    saving: "Saving …",
    saved: "Saved",
    failed: "Save failed",
    queueHint: "Reports are collected under Options → “Bug Reports” and can be submitted in batches there.",
    openList: "Open bug list",
  },
};

let t = COPY[detectLocale()];

// ----------------------------------------------------------------------------
// Init / mount
// ----------------------------------------------------------------------------

export function initBugReporter() {
  if (initialized) return;
  if (typeof document === "undefined") return;
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", initBugReporter, { once: true });
    return;
  }
  initialized = true;
  t = COPY[detectLocale()];
  injectStyles();
  host = document.createElement("div");
  host.className = "nwt-bug-reporter-root";
  document.body.appendChild(host);
  renderFab();
}

function injectStyles() {
  if (document.getElementById("nwt-bug-reporter-styles")) return;
  const style = document.createElement("style");
  style.id = "nwt-bug-reporter-styles";
  style.textContent = BUG_REPORTER_CSS;
  document.head.appendChild(style);
}

const BUG_REPORTER_CSS = `
.nwt-bug-reporter-root { position: fixed; inset: auto 0 0 auto; z-index: 2147483646; }

/* Invisible 96×96 hot-zone in the bottom-right corner. The FAB only becomes
   visible while the cursor is inside this zone or the FAB has focus. */
.nwt-bug-fab-zone {
  position: fixed; right: 0; bottom: 0;
  width: 96px; height: 96px;
  z-index: 2147483646;
  pointer-events: auto;
}
.nwt-bug-fab {
  position: absolute; right: 14px; bottom: 14px;
  width: 38px; height: 38px; border-radius: 999px;
  display: flex; align-items: center; justify-content: center;
  background: linear-gradient(180deg, #1f2933 0%, #141a20 100%);
  color: #ef4444; border: 1px solid rgba(239,68,68,0.45);
  box-shadow: 0 6px 14px rgba(0,0,0,0.35); cursor: pointer;
  opacity: 0; pointer-events: none;
  transform: translateY(8px);
  transition: opacity .18s ease, transform .18s ease, border-color .12s ease, box-shadow .12s ease;
}
.nwt-bug-fab-zone:hover .nwt-bug-fab,
.nwt-bug-fab-zone:focus-within .nwt-bug-fab,
.nwt-bug-fab:hover {
  opacity: 1; pointer-events: auto;
  transform: translateY(0);
}
.nwt-bug-fab:hover { border-color: rgba(239,68,68,0.85); box-shadow: 0 10px 20px rgba(0,0,0,0.4); }
.nwt-bug-fab:focus-visible { outline: 2px solid #ef4444; outline-offset: 2px; opacity: 1; pointer-events: auto; transform: translateY(0); }

.nwt-bug-modal-backdrop {
  position: fixed; inset: 0; background: rgba(8,12,18,0.55);
  display: flex; align-items: center; justify-content: center;
  z-index: 2147483647; padding: 16px;
}
/* Browser default [hidden] { display: none } is overridden by display:flex above —
   restore it explicitly so closeModal()'s hidden=true actually hides the modal. */
.nwt-bug-modal-backdrop[hidden] { display: none !important; }
.nwt-bug-modal {
  background: #141a20; color: #e7ecf2;
  border: 1px solid rgba(255,255,255,0.08); border-radius: 14px;
  width: min(560px, 100%); max-height: calc(100vh - 32px);
  display: flex; flex-direction: column; overflow: hidden;
  box-shadow: 0 20px 40px rgba(0,0,0,0.55);
  font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  font-size: 13px;
}
.nwt-bug-modal-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 12px 14px; border-bottom: 1px solid rgba(255,255,255,0.06);
  background: linear-gradient(180deg,#161c22 0%, #12171d 100%);
}
.nwt-bug-modal-header strong { font-size: 14px; }
.nwt-bug-modal-body {
  padding: 14px; overflow: auto; display: flex; flex-direction: column; gap: 10px;
}
.nwt-bug-modal-footer {
  display: flex; align-items: center; justify-content: space-between; gap: 10px;
  padding: 10px 14px; border-top: 1px solid rgba(255,255,255,0.06);
  background: linear-gradient(180deg,#11161c 0%, #0f1318 100%);
}
.nwt-bug-row-2 { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
.nwt-bug-field { display: flex; flex-direction: column; gap: 4px; font-size: 12px; }
.nwt-bug-field > span { color: #a3afbd; font-weight: 600; font-size: 11px; letter-spacing: .02em; text-transform: uppercase; }
.nwt-bug-field textarea, .nwt-bug-select {
  background: #0f1419; color: #e7ecf2;
  border: 1px solid rgba(255,255,255,0.08); border-radius: 8px;
  padding: 8px 10px; font: inherit; resize: vertical;
}
.nwt-bug-field textarea:focus, .nwt-bug-select:focus {
  outline: none; border-color: rgba(103,232,249,0.7);
  box-shadow: 0 0 0 3px rgba(103,232,249,0.18);
}
.nwt-bug-actions-row { display: flex; gap: 8px; flex-wrap: wrap; }
.nwt-bug-btn {
  appearance: none; cursor: pointer; font: inherit;
  padding: 7px 12px; border-radius: 8px; border: 1px solid rgba(255,255,255,0.10);
  background: #1b232a; color: #e7ecf2;
  transition: background .12s ease, border-color .12s ease;
}
.nwt-bug-btn:hover { background: #222b34; border-color: rgba(255,255,255,0.16); }
.nwt-bug-btn-primary { background: #d14b3f; border-color: #d14b3f; color: #fff; }
.nwt-bug-btn-primary:hover { background: #c23d31; border-color: #c23d31; }
.nwt-bug-btn-ghost { background: transparent; }
.nwt-bug-link { background: transparent; border: 0; color: #67e8f9; cursor: pointer; padding: 4px 6px; font: inherit; }
.nwt-bug-link:hover { text-decoration: underline; }
.nwt-bug-status { font-size: 12px; color: #a3afbd; }
.nwt-bug-hint { color: #7c8997; font-size: 11px; line-height: 1.45; margin: 4px 0 0; }
.nwt-bug-attachment {
  border: 1px dashed rgba(255,255,255,0.15); border-radius: 10px;
  padding: 8px; display: flex; flex-direction: column; gap: 6px;
}
.nwt-bug-attachment-meta {
  display: flex; align-items: center; justify-content: space-between;
  font-size: 11px; color: #a3afbd;
}
.nwt-bug-attachment img { max-width: 100%; max-height: 240px; object-fit: contain; border-radius: 6px; background: #0a0d11; }

.nwt-bug-markup-overlay {
  position: fixed; inset: 0; z-index: 2147483647;
  background: rgba(8,12,18,0.18); cursor: crosshair;
}
.nwt-bug-markup-toolbar {
  position: fixed; top: 12px; left: 50%; transform: translateX(-50%);
  background: #141a20; border: 1px solid rgba(255,255,255,0.10);
  border-radius: 10px; box-shadow: 0 12px 26px rgba(0,0,0,0.4);
  padding: 8px 12px; display: flex; align-items: center; gap: 12px;
  color: #e7ecf2; cursor: default; max-width: calc(100vw - 24px);
}
.nwt-bug-markup-toolbar strong { font-size: 13px; }
.nwt-bug-markup-hint { font-size: 11px; color: #a3afbd; max-width: 260px; }
.nwt-bug-markup-toolbar-actions { display: flex; gap: 6px; }
.nwt-bug-markup-selection {
  position: absolute; box-sizing: border-box;
  border: 2px solid #ef4444; background: rgba(239,68,68,0.08);
  pointer-events: auto;
}
.nwt-bug-markup-selection[data-mode="drawing"] { cursor: crosshair; }
`;

function renderFab() {
  // Hot-zone wrapper — only when the cursor enters the bottom-right corner
  // does the FAB fade in. The user shouldn't see it permanently.
  const zone = document.createElement("div");
  zone.className = "nwt-bug-fab-zone";

  fabBtn = document.createElement("button");
  fabBtn.type = "button";
  fabBtn.className = "nwt-bug-fab";
  fabBtn.setAttribute("aria-label", t.fab);
  fabBtn.title = t.fab;
  fabBtn.innerHTML = bugIconSvg();
  fabBtn.addEventListener("click", openModal);

  zone.appendChild(fabBtn);
  host.appendChild(zone);
}

function bugIconSvg() {
  // simple bug glyph
  return `
    <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true" focusable="false">
      <path d="M12 2.5a3.5 3.5 0 0 1 3.5 3.5h-7A3.5 3.5 0 0 1 12 2.5Zm-7 8a1 1 0 0 1 1-1h1.2A6 6 0 0 1 8 9V8h8v1a6 6 0 0 1 .8.5H18a1 1 0 1 1 0 2h-1.06A6 6 0 0 1 17 13h2a1 1 0 1 1 0 2h-2a6 6 0 0 1-.27 1.37L18.7 17.3a1 1 0 1 1-1.4 1.4l-1.7-1.7A6 6 0 0 1 13 18.94V12h-2v6.94A6 6 0 0 1 8.4 17l-1.7 1.7a1 1 0 1 1-1.4-1.4l1.97-1.96A6 6 0 0 1 7 15H5a1 1 0 1 1 0-2h2a6 6 0 0 1 .06-1.5H6a1 1 0 0 1-1-1Z" fill="currentColor"/>
    </svg>`;
}

// ----------------------------------------------------------------------------
// Modal lifecycle
// ----------------------------------------------------------------------------

function openModal() {
  if (modalOpen) return;
  modalOpen = true;
  if (!modalEl) renderModal();
  modalEl.hidden = false;
  modalEl.setAttribute("aria-hidden", "false");
  setTimeout(() => {
    const ta = modalEl.querySelector('textarea[data-field="summary"]');
    ta?.focus();
  }, 30);
}

function closeModal() {
  if (!modalOpen) return;
  modalOpen = false;
  if (modalEl) {
    modalEl.hidden = true;
    modalEl.setAttribute("aria-hidden", "true");
  }
  if (markupMode !== "idle") cancelMarkup();
}

function renderModal() {
  modalEl = document.createElement("div");
  modalEl.className = "nwt-bug-modal-backdrop";
  modalEl.hidden = true;
  modalEl.setAttribute("role", "dialog");
  modalEl.setAttribute("aria-modal", "true");
  modalEl.setAttribute("aria-label", t.title);
  modalEl.innerHTML = `
    <div class="nwt-bug-modal" role="document">
      <header class="nwt-bug-modal-header">
        <strong>${escapeHtml(t.title)}</strong>
        <button type="button" class="nwt-bug-link" data-action="close">${escapeHtml(t.close)}</button>
      </header>
      <div class="nwt-bug-modal-body">
        <div class="nwt-bug-row-2">
          <label class="nwt-bug-field">
            <span>${escapeHtml(t.kind)}</span>
            <select data-field="kind" class="nwt-bug-select">
              <option value="bug">${escapeHtml(t.kindBug)}</option>
              <option value="feature">${escapeHtml(t.kindFeature)}</option>
            </select>
          </label>
          <label class="nwt-bug-field">
            <span>${escapeHtml(t.severity)}</span>
            <select data-field="severity" class="nwt-bug-select">
              <option value="low">${escapeHtml(t.severityLow)}</option>
              <option value="medium" selected>${escapeHtml(t.severityMedium)}</option>
              <option value="high">${escapeHtml(t.severityHigh)}</option>
            </select>
          </label>
        </div>
        <label class="nwt-bug-field">
          <span>${escapeHtml(t.summary)}</span>
          <textarea data-field="summary" rows="3" placeholder="${escapeHtml(t.summaryPlaceholder)}"></textarea>
        </label>
        <label class="nwt-bug-field">
          <span>${escapeHtml(t.expected)}</span>
          <textarea data-field="expected" rows="2" placeholder="${escapeHtml(t.expectedPlaceholder)}"></textarea>
        </label>
        <div class="nwt-bug-actions-row">
          <button type="button" class="nwt-bug-btn nwt-bug-btn-ghost" data-action="markup">📷 ${escapeHtml(t.markArea)}</button>
          <button type="button" class="nwt-bug-btn nwt-bug-btn-ghost" data-action="open-list">${escapeHtml(t.openList)}</button>
        </div>
        <div class="nwt-bug-attachment" data-attachment hidden>
          <div class="nwt-bug-attachment-meta">
            <span data-attachment-mode></span>
            <button type="button" class="nwt-bug-link" data-action="remove-attachment">${escapeHtml(t.removeAttachment)}</button>
          </div>
          <img alt="" data-attachment-img />
        </div>
        <p class="nwt-bug-hint">${escapeHtml(t.queueHint)}</p>
      </div>
      <footer class="nwt-bug-modal-footer">
        <span class="nwt-bug-status" data-status></span>
        <button type="button" class="nwt-bug-btn nwt-bug-btn-primary" data-action="save">${escapeHtml(t.save)}</button>
      </footer>
    </div>
  `;
  document.body.appendChild(modalEl);

  // event delegation
  modalEl.addEventListener("click", onModalClick);
  modalEl.addEventListener("input", onModalInput);
  modalEl.addEventListener("change", onModalInput);
  // close on backdrop click
  modalEl.addEventListener("mousedown", (event) => {
    if (event.target === modalEl) closeModal();
  });
}

function onModalClick(event) {
  const btn = event.target.closest("[data-action]");
  if (!btn) return;
  switch (btn.dataset.action) {
    case "close":
      closeModal();
      break;
    case "markup":
      startMarkup();
      break;
    case "remove-attachment":
      attachment = null;
      syncAttachmentInModal();
      break;
    case "open-list":
      try {
        const url = chrome?.runtime?.getURL?.("options.html#bug-reports");
        if (url && chrome?.tabs?.create) {
          chrome.tabs.create({ url });
        } else if (url) {
          window.open(url, "_blank", "noopener");
        } else {
          chrome?.runtime?.openOptionsPage?.();
        }
      } catch {}
      break;
    case "save":
      saveReport();
      break;
  }
}

function onModalInput(event) {
  const target = event.target;
  if (!target?.dataset?.field) return;
  switch (target.dataset.field) {
    case "summary":
      summary = target.value;
      break;
    case "expected":
      expected = target.value;
      break;
    case "kind":
      kind = target.value;
      break;
    case "severity":
      severity = target.value;
      break;
  }
}

function syncAttachmentInModal() {
  if (!modalEl) return;
  const wrap = modalEl.querySelector("[data-attachment]");
  const meta = modalEl.querySelector("[data-attachment-mode]");
  const img = modalEl.querySelector("[data-attachment-img]");
  if (!wrap || !meta || !img) return;
  if (!attachment) {
    wrap.hidden = true;
    img.removeAttribute("src");
    return;
  }
  wrap.hidden = false;
  img.src = attachment.compositeDataUrl;
  meta.textContent = attachment.captureMode === "markup-only" ? t.attachmentFallback : t.attachmentScreen;
}

function setStatus(text) {
  if (!modalEl) return;
  const el = modalEl.querySelector("[data-status]");
  if (el) el.textContent = text || "";
}

// ----------------------------------------------------------------------------
// Save report to RxDB. Large screenshots are stored as bug_report_chunks so
// WebRTC replication does not have to move a huge inline data URL.
// ----------------------------------------------------------------------------

async function saveReport() {
  const trimmed = (summary || "").trim();
  if (!trimmed) {
    setStatus(t.failed + " — " + t.summaryPlaceholder);
    return;
  }
  setStatus(t.saving);
  try {
    const id = (crypto?.randomUUID?.() || `bug-${Date.now()}-${Math.random().toString(16).slice(2, 8)}`);
    const meta = buildReportMetadata(id, trimmed);
    const client = await rxdb();
    if (!client?.upsertBugReport) {
      throw new Error("rxdbClient not available");
    }

    // 1) Persist screenshot chunks first (if any) so the parent doc records
    //    the right chunk count.
    let screenshotInfo = { mime: "", total: 0 };
    if (attachment?.compositeDataUrl) {
      try {
        screenshotInfo = await client.saveBugReportScreenshot(id, attachment.compositeDataUrl);
      } catch (err) {
        console.warn("[bugReporter] screenshot chunking failed", err);
        throw new Error("Screenshot konnte nicht in RxDB-Chunks gespeichert werden.");
      }
    }

    // 2) Persist parent doc with chunk count + small inline markup fallback
    await client.upsertBugReport({
      ...meta,
      screenshotMime: screenshotInfo.mime || "",
      screenshotChunkCount: screenshotInfo.total || 0,
      markupSvgDataUrl: attachment?.markupSvgDataUrl || null,
      annotationRect: attachment?.rect || null,
      annotationStrokes: Array.isArray(attachment?.strokes) ? attachment.strokes : null,
      annotationCaptureMode: attachment?.captureMode || "",
      annotationCapturedAt: attachment?.capturedAt || ""
    });

    setStatus(t.saved + " (" + id.slice(0, 8) + ")");
    summary = "";
    expected = "";
    attachment = null;
    if (modalEl) {
      modalEl.querySelector('textarea[data-field="summary"]').value = "";
      modalEl.querySelector('textarea[data-field="expected"]').value = "";
      syncAttachmentInModal();
    }
    try {
      chrome?.runtime?.sendMessage?.({ type: "bug-reports:updated", id }, () => void chrome?.runtime?.lastError);
    } catch {}
    setTimeout(() => setStatus(""), 2500);
  } catch (err) {
    console.warn("[bugReporter] save failed", err);
    setStatus(t.failed + " — " + (err?.message || String(err)));
  }
}

function buildReportMetadata(id, trimmedSummary) {
  const appVersion = String(
    document.querySelector("[data-app-version]")?.dataset?.appVersion || readManifestVersion() || ""
  );
  const baustein = String(document.body?.dataset?.baustein || "");
  // Best-effort module hint based on document title / pathname.
  const path = (location?.pathname || "").split("/").filter(Boolean);
  const fallbackModule = path[path.length - 1]?.replace(/\.html$/i, "") || "panel";
  return {
    id,
    type: "bug_report",
    kind,
    title: trimmedSummary.slice(0, 120),
    summary: trimmedSummary,
    expected: (expected || "").trim(),
    severity,
    status: "draft",
    pageUrl: location.href,
    appVersion,
    moduleId: baustein || fallbackModule,
    submoduleId: "",
    viewport: {
      width: window.innerWidth,
      height: window.innerHeight,
      scrollX: window.scrollX,
      scrollY: window.scrollY,
      devicePixelRatio: window.devicePixelRatio || 1
    },
    userAgent: navigator?.userAgent || "",
    source: "ninja-workflow-tool-bug-report",
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    submittedAt: null
  };
}

function readManifestVersion() {
  try { return chrome?.runtime?.getManifest?.()?.version || ""; } catch { return ""; }
}

// ----------------------------------------------------------------------------
// Markup overlay (selection + drawing)
// ----------------------------------------------------------------------------

function startMarkup() {
  if (markupMode !== "idle") return;
  markupMode = "selecting";
  selectionOrigin = null;
  selectionRect = null;
  strokes = [];
  activeStroke = null;
  // Make the underlying view fully visible — modal & FAB would block the
  // very area the user wants to capture and would also bleed into the
  // captured frame. Restored on cancel / commit.
  hideHostChrome();
  renderOverlay();
}

function cancelMarkup() {
  markupMode = "idle";
  selectionOrigin = null;
  selectionRect = null;
  strokes = [];
  activeStroke = null;
  destroyOverlay();
  showHostChrome();
}

// --- Hide/show modal + FAB during markup ---
function hideHostChrome() {
  if (modalEl) {
    modalEl.dataset._wasOpen = modalEl.hidden ? "0" : "1";
    modalEl.hidden = true;
    modalEl.style.display = "none";
  }
  if (fabBtn) {
    fabBtn.dataset._wasVisible = fabBtn.hidden ? "0" : "1";
    fabBtn.style.display = "none";
  }
}

function showHostChrome() {
  if (modalEl) {
    modalEl.style.display = "";
    if (modalEl.dataset._wasOpen === "1") {
      modalEl.hidden = false;
    }
    delete modalEl.dataset._wasOpen;
  }
  if (fabBtn) {
    fabBtn.style.display = "";
    delete fabBtn.dataset._wasVisible;
  }
}

function renderOverlay() {
  if (overlayEl) destroyOverlay();
  overlayEl = document.createElement("div");
  overlayEl.className = "nwt-bug-markup-overlay";
  overlayEl.innerHTML = `
    <div class="nwt-bug-markup-toolbar" data-toolbar>
      <strong>${escapeHtml(t.markupTitle)}</strong>
      <span class="nwt-bug-markup-hint">${escapeHtml(t.markupHint)}</span>
      <div class="nwt-bug-markup-toolbar-actions">
        <button type="button" class="nwt-bug-btn nwt-bug-btn-ghost" data-toolbar-action="cancel">${escapeHtml(t.cancelMarkup)}</button>
        <button type="button" class="nwt-bug-btn nwt-bug-btn-ghost" data-toolbar-action="clear" hidden>${escapeHtml(t.clearMarkup)}</button>
        <button type="button" class="nwt-bug-btn nwt-bug-btn-primary" data-toolbar-action="save" hidden>${escapeHtml(t.saveMarkup)}</button>
      </div>
    </div>
    <div class="nwt-bug-markup-selection" data-selection hidden></div>
  `;
  document.body.appendChild(overlayEl);

  overlayEl.addEventListener("pointerdown", onOverlayPointerDown);
  overlayEl.addEventListener("pointermove", onOverlayPointerMove);
  overlayEl.addEventListener("pointerup", onOverlayPointerUp);

  const toolbar = overlayEl.querySelector("[data-toolbar]");
  toolbar.addEventListener("pointerdown", (event) => event.stopPropagation());
  toolbar.addEventListener("click", (event) => {
    const btn = event.target.closest("[data-toolbar-action]");
    if (!btn) return;
    if (btn.dataset.toolbarAction === "cancel") cancelMarkup();
    else if (btn.dataset.toolbarAction === "clear") {
      strokes = [];
      activeStroke = null;
      paintSelection();
    }
    else if (btn.dataset.toolbarAction === "save") commitMarkup();
  });

  paintSelection();
}

function destroyOverlay() {
  if (!overlayEl) return;
  overlayEl.remove();
  overlayEl = null;
}

function onOverlayPointerDown(event) {
  if (markupMode === "selecting") {
    selectionOrigin = { x: event.clientX, y: event.clientY };
    selectionRect = { x: selectionOrigin.x, y: selectionOrigin.y, width: 0, height: 0 };
    overlayEl.setPointerCapture?.(event.pointerId);
    paintSelection();
  } else if (markupMode === "drawing") {
    if (!isInsideRect(event.clientX, event.clientY, selectionRect)) return;
    activeStroke = [relativePoint(event)];
    overlayEl.setPointerCapture?.(event.pointerId);
    paintSelection();
  }
}

function onOverlayPointerMove(event) {
  if (markupMode === "selecting" && selectionOrigin) {
    selectionRect = normalizeRect(selectionOrigin, { x: event.clientX, y: event.clientY });
    paintSelection();
  } else if (markupMode === "drawing" && activeStroke) {
    activeStroke.push(relativePoint(event));
    paintSelection();
  }
}

function onOverlayPointerUp(event) {
  if (markupMode === "selecting" && selectionRect) {
    overlayEl.releasePointerCapture?.(event.pointerId);
    if (selectionRect.width < 12 || selectionRect.height < 12) {
      // ignore tiny selections, restart
      selectionOrigin = null;
      selectionRect = null;
      paintSelection();
      return;
    }
    markupMode = "drawing";
    paintSelection();
  } else if (markupMode === "drawing" && activeStroke) {
    overlayEl.releasePointerCapture?.(event.pointerId);
    if (activeStroke.length > 1) strokes.push(activeStroke);
    activeStroke = null;
    paintSelection();
  }
}

function relativePoint(event) {
  const rect = selectionRect;
  return {
    x: Math.max(0, Math.min(rect.width, event.clientX - rect.x)),
    y: Math.max(0, Math.min(rect.height, event.clientY - rect.y)),
  };
}

function normalizeRect(start, end) {
  const x = Math.min(start.x, end.x);
  const y = Math.min(start.y, end.y);
  return {
    x,
    y,
    width: Math.abs(end.x - start.x),
    height: Math.abs(end.y - start.y),
  };
}

function isInsideRect(x, y, rect) {
  if (!rect) return false;
  return x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height;
}

function paintSelection() {
  if (!overlayEl) return;
  const sel = overlayEl.querySelector("[data-selection]");
  const cancelBtn = overlayEl.querySelector('[data-toolbar-action="cancel"]');
  const clearBtn = overlayEl.querySelector('[data-toolbar-action="clear"]');
  const saveBtn = overlayEl.querySelector('[data-toolbar-action="save"]');

  if (!selectionRect) {
    sel.hidden = true;
    if (clearBtn) clearBtn.hidden = true;
    if (saveBtn) saveBtn.hidden = true;
    return;
  }
  sel.hidden = false;
  sel.style.left = selectionRect.x + "px";
  sel.style.top = selectionRect.y + "px";
  sel.style.width = selectionRect.width + "px";
  sel.style.height = selectionRect.height + "px";
  sel.dataset.mode = markupMode;

  const allStrokes = activeStroke ? [...strokes, activeStroke] : strokes;
  const polylines = allStrokes.map((stroke) => {
    const points = stroke.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ");
    return `<polyline points="${points}" fill="none" stroke="#ef4444" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>`;
  }).join("");
  sel.innerHTML = `
    <svg width="100%" height="100%" viewBox="0 0 ${selectionRect.width} ${selectionRect.height}">
      ${polylines}
    </svg>
  `;
  if (clearBtn) clearBtn.hidden = markupMode !== "drawing" || allStrokes.length === 0;
  if (saveBtn) saveBtn.hidden = markupMode !== "drawing";
}

// ----------------------------------------------------------------------------
// Commit markup -> attachment
// ----------------------------------------------------------------------------

async function commitMarkup() {
  if (markupMode !== "drawing" || !selectionRect) return;
  if (savingMarkup) return;
  savingMarkup = true;

  const rect = { ...selectionRect };
  const finalStrokes = activeStroke ? [...strokes, activeStroke] : [...strokes];
  const markupSvgDataUrl = buildSvgDataUrl(rect, finalStrokes);
  const previousMode = markupMode;
  markupMode = "idle";

  // Hide the overlay (toolbar + selection rect) BEFORE we sample the frame,
  // so the captured screenshot only contains the underlying page content.
  if (overlayEl) {
    overlayEl.style.visibility = "hidden";
    overlayEl.style.pointerEvents = "none";
  }
  // Yield two animation frames so the browser actually paints the hidden
  // state before captureVisibleTab samples the tab — without this, the
  // overlay can still be in the captured PNG.
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));

  try {
    const screenDataUrl = await captureScreenRegion(rect).catch(() => null);
    const domDataUrl = screenDataUrl ? null : await captureDomRegion(rect).catch(() => null);
    const screenshotDataUrl = screenDataUrl ?? domDataUrl;
    const compositeDataUrl = screenshotDataUrl
      ? await buildCompositeDataUrl(rect, finalStrokes, screenshotDataUrl).catch(() => markupSvgDataUrl)
      : markupSvgDataUrl;

    attachment = {
      rect,
      strokes: finalStrokes,
      screenshotDataUrl: screenshotDataUrl || null,
      markupSvgDataUrl,
      compositeDataUrl,
      captureMode: screenDataUrl ? "screen" : domDataUrl ? "dom" : "markup-only",
      capturedAt: new Date().toISOString(),
    };
    syncAttachmentInModal();
  } catch (err) {
    console.warn("[bugReporter] markup commit failed", err);
    markupMode = previousMode;
  } finally {
    destroyOverlay();
    selectionOrigin = null;
    selectionRect = null;
    strokes = [];
    activeStroke = null;
    savingMarkup = false;
    // Bring the modal + FAB back so the user sees their attachment.
    showHostChrome();
  }
}

function buildSvgDataUrl(rect, strokeList) {
  const polylines = strokeList.map((stroke) => {
    const points = stroke.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ");
    return `<polyline points="${points}" fill="none" stroke="#ef4444" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>`;
  }).join("");
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${rect.width}" height="${rect.height}" viewBox="0 0 ${rect.width} ${rect.height}"><rect width="100%" height="100%" fill="rgba(239,68,68,0.08)" stroke="#ef4444" stroke-width="2"/>${polylines}</svg>`;
  return `data:image/svg+xml;base64,${window.btoa(unescape(encodeURIComponent(svg)))}`;
}

// Primary path: chrome.tabs.captureVisibleTab — instant, no permission picker,
// captures exactly the active tab. Works for view pages and the options tab,
// because their FAB lives in the tab itself. For the side panel the captured
// frame is the *adjacent* tab — we detect that mismatch via dimensions and
// fall through to getDisplayMedia / DOM serialization.
function captureVisibleTabPng() {
  return new Promise((resolve, reject) => {
    if (!chrome?.tabs?.captureVisibleTab) return resolve(null);
    try {
      chrome.tabs.captureVisibleTab({ format: "png" }, (dataUrl) => {
        const err = chrome.runtime?.lastError;
        if (err) return reject(new Error(err.message || "captureVisibleTab failed"));
        resolve(dataUrl || null);
      });
    } catch (err) { reject(err); }
  });
}

async function captureScreenRegion(rect) {
  // 1) Try chrome.tabs.captureVisibleTab first — no picker, no focus loss.
  try {
    const dataUrl = await captureVisibleTabPng();
    if (dataUrl) {
      const image = await loadImage(dataUrl).catch(() => null);
      if (image) {
        // Validate the captured frame matches our viewport. captureVisibleTab
        // returns the *active* tab; if we're in the side panel that's the
        // adjacent tab and the rect coords don't apply — skip.
        const dpr = window.devicePixelRatio || 1;
        const expectedW = window.innerWidth * dpr;
        const expectedH = window.innerHeight * dpr;
        const widthRatio = Math.abs(image.naturalWidth - expectedW) / Math.max(1, expectedW);
        const heightRatio = Math.abs(image.naturalHeight - expectedH) / Math.max(1, expectedH);
        const matchesOurViewport = widthRatio < 0.20 && heightRatio < 0.40;
        if (matchesOurViewport) {
          const sx = Math.max(0, Math.round(rect.x * dpr));
          const sy = Math.max(0, Math.round(rect.y * dpr));
          const sw = Math.max(1, Math.min(image.naturalWidth - sx, Math.round(rect.width * dpr)));
          const sh = Math.max(1, Math.min(image.naturalHeight - sy, Math.round(rect.height * dpr)));
          const canvas = document.createElement("canvas");
          canvas.width = sw; canvas.height = sh;
          const ctx = canvas.getContext("2d");
          if (ctx) {
            ctx.drawImage(image, sx, sy, sw, sh, 0, 0, sw, sh);
            return canvas.toDataURL("image/png");
          }
        }
        // Mismatch (likely side panel) → fall through.
      }
    }
  } catch (err) {
    console.warn("[bugReporter] captureVisibleTab unavailable, falling back", err);
  }

  // 2) Fallback to getDisplayMedia (user picker) — only used in side panel
  //    or when captureVisibleTab is blocked.
  if (!navigator?.mediaDevices?.getDisplayMedia) return null;
  let stream;
  try {
    stream = await navigator.mediaDevices.getDisplayMedia({
      video: { displaySurface: "browser" },
      audio: false,
    });
  } catch {
    return null;
  }
  try {
    const video = document.createElement("video");
    video.muted = true;
    video.playsInline = true;
    video.srcObject = stream;
    await video.play();
    await waitForVideoFrame(video);

    const width = Math.max(1, video.videoWidth);
    const height = Math.max(1, video.videoHeight);
    const scaleX = width / window.innerWidth;
    const scaleY = height / window.innerHeight;
    const sourceX = Math.max(0, Math.round(rect.x * scaleX));
    const sourceY = Math.max(0, Math.round(rect.y * scaleY));
    const sourceWidth = Math.max(1, Math.min(width - sourceX, Math.round(rect.width * scaleX)));
    const sourceHeight = Math.max(1, Math.min(height - sourceY, Math.round(rect.height * scaleY)));
    const canvas = document.createElement("canvas");
    canvas.width = sourceWidth;
    canvas.height = sourceHeight;
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    ctx.drawImage(video, sourceX, sourceY, sourceWidth, sourceHeight, 0, 0, sourceWidth, sourceHeight);
    return canvas.toDataURL("image/png");
  } finally {
    try { stream.getTracks().forEach((track) => track.stop()); } catch {}
  }
}

async function captureDomRegion(rect) {
  try {
    const width = Math.max(1, Math.round(window.innerWidth));
    const height = Math.max(1, Math.round(window.innerHeight));
    const clone = document.documentElement.cloneNode(true);
    clone.querySelectorAll("script, .nwt-bug-markup-overlay, .nwt-bug-modal-backdrop").forEach((node) => node.remove());
    clone.querySelectorAll("textarea").forEach((node) => {
      const name = node.getAttribute("name") || "";
      const source = name ? document.querySelector(`textarea[name="${name}"]`) : null;
      if (source) node.textContent = source.value;
    });
    clone.querySelectorAll("input").forEach((node) => {
      const name = node.getAttribute("name") || "";
      const source = name ? document.querySelector(`input[name="${name}"]`) : null;
      if (source) node.setAttribute("value", source.value);
    });
    const styleEl = document.createElement("style");
    styleEl.textContent = collectStyleText();
    clone.querySelector("head")?.appendChild(styleEl);
    clone.setAttribute("style", `${clone.getAttribute("style") || ""};width:${width}px;min-height:${height}px;`);
    const serialized = new XMLSerializer().serializeToString(clone);
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}"><foreignObject width="100%" height="100%">${serialized}</foreignObject></svg>`;
    const image = await loadImage(`data:image/svg+xml;base64,${window.btoa(unescape(encodeURIComponent(svg)))}`);
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(rect.width));
    canvas.height = Math.max(1, Math.round(rect.height));
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    ctx.drawImage(image, rect.x, rect.y, rect.width, rect.height, 0, 0, canvas.width, canvas.height);
    return canvas.toDataURL("image/png");
  } catch {
    return null;
  }
}

function collectStyleText() {
  return Array.from(document.styleSheets).map((sheet) => {
    try {
      return Array.from(sheet.cssRules).map((rule) => rule.cssText).join("\n");
    } catch {
      return "";
    }
  }).join("\n");
}

async function buildCompositeDataUrl(rect, strokeList, screenshotDataUrl) {
  const image = await loadImage(screenshotDataUrl);
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, image.naturalWidth || Math.round(rect.width));
  canvas.height = Math.max(1, image.naturalHeight || Math.round(rect.height));
  const ctx = canvas.getContext("2d");
  if (!ctx) return screenshotDataUrl;
  ctx.drawImage(image, 0, 0, canvas.width, canvas.height);
  ctx.strokeStyle = "#ef4444";
  ctx.lineWidth = Math.max(3, Math.round(canvas.width / Math.max(120, rect.width) * 4));
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  const scaleX = canvas.width / rect.width;
  const scaleY = canvas.height / rect.height;
  strokeList.forEach((stroke) => {
    if (stroke.length < 2) return;
    ctx.beginPath();
    ctx.moveTo(stroke[0].x * scaleX, stroke[0].y * scaleY);
    stroke.slice(1).forEach((p) => ctx.lineTo(p.x * scaleX, p.y * scaleY));
    ctx.stroke();
  });
  return canvas.toDataURL("image/png");
}

function loadImage(src) {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("Image load failed"));
    image.src = src;
  });
}

function waitForVideoFrame(video) {
  if (typeof video.requestVideoFrameCallback === "function") {
    return new Promise((resolve) => video.requestVideoFrameCallback(() => resolve()));
  }
  return new Promise((resolve) => window.setTimeout(resolve, 160));
}

// ----------------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------------

function escapeHtml(value) {
  return String(value ?? "").replace(/[&<>"']/g, (ch) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  }[ch]));
}

// auto-init on import
initBugReporter();
