"use client";

import { useState, type PointerEvent } from "react";

type BugReportWidgetProps = {
  locale?: string;
  moduleId?: string;
  submoduleId?: string;
};

type Point = { x: number; y: number };
type Rect = { x: number; y: number; width: number; height: number };
type Stroke = Point[];
type MarkupAttachment = {
  rect: Rect;
  strokes: Stroke[];
  svgDataUrl: string;
  markupSvgDataUrl: string;
  screenshotDataUrl?: string;
  compositeDataUrl: string;
  captureMode: "screen" | "markup-only";
  capturedAt: string;
};
type MarkupState =
  | { mode: "idle" }
  | { mode: "selecting"; origin?: Point; rect?: Rect }
  | { mode: "drawing"; rect: Rect; strokes: Stroke[]; activeStroke?: Stroke };
type MarkupWithRect =
  | { mode: "selecting"; origin?: Point; rect: Rect }
  | { mode: "drawing"; rect: Rect; strokes: Stroke[]; activeStroke?: Stroke };

const copy = {
  de: {
    bug: "Bug",
    title: "Fehler melden",
    close: "Schließen",
    summary: "Was ist falsch?",
    expected: "Was hast du erwartet?",
    context: "Kontext",
    submit: "Melden",
    submitting: "Wird gemeldet...",
    saved: "Gemeldet",
    failed: "Konnte nicht gemeldet werden",
    markArea: "Screenshot-Bereich",
    markupTitle: "Bereich auswählen und markieren",
    markupHint: "Ziehe einen Bereich auf. Danach kannst du mit dem Stift darauf zeichnen.",
    cancelMarkup: "Abbrechen",
    saveMarkup: "Übernehmen",
    clearMarkup: "Löschen",
    attachment: "Screenshot-Markup",
    attachmentScreen: "Screenshot mit Markup",
    attachmentFallback: "Nur Markup gespeichert. Bildschirmfreigabe wurde nicht übernommen.",
    removeAttachment: "Markup entfernen",
    summaryPlaceholder: "Beschreibe den Fehler kurz.",
    expectedPlaceholder: "Beschreibe das erwartete Verhalten."
  },
  en: {
    bug: "Bug",
    title: "Report bug",
    close: "Close",
    summary: "What is wrong?",
    expected: "What did you expect?",
    context: "Context",
    submit: "Report",
    submitting: "Reporting...",
    saved: "Reported",
    failed: "Could not report",
    markArea: "Capture area",
    markupTitle: "Select area and draw",
    markupHint: "Drag over the area. Then draw on it with the pen.",
    cancelMarkup: "Cancel",
    saveMarkup: "Attach",
    clearMarkup: "Clear",
    attachment: "Screenshot markup",
    attachmentScreen: "Screenshot with markup",
    attachmentFallback: "Only markup saved. Screen capture was not attached.",
    removeAttachment: "Remove markup",
    summaryPlaceholder: "Briefly describe the issue.",
    expectedPlaceholder: "Describe the expected behavior."
  }
};

export function BugReportWidget({ locale, moduleId, submoduleId }: BugReportWidgetProps) {
  const activeLocale = locale === "de" ? "de" : "en";
  const t = copy[activeLocale];
  const [open, setOpen] = useState(false);
  const [summary, setSummary] = useState("");
  const [expected, setExpected] = useState("");
  const [status, setStatus] = useState<"idle" | "submitting" | "saved" | "failed">("idle");
  const [reportId, setReportId] = useState<string | null>(null);
  const [markup, setMarkup] = useState<MarkupState>({ mode: "idle" });
  const [attachment, setAttachment] = useState<MarkupAttachment | null>(null);
  const [savingMarkup, setSavingMarkup] = useState(false);
  const visibleMarkup: MarkupWithRect | null = markup.mode === "drawing"
    ? markup
    : markup.mode === "selecting" && markup.rect
      ? { ...markup, rect: markup.rect }
      : null;

  async function submit() {
    if (!summary.trim()) return;
    setStatus("submitting");
    const response = await fetch("/api/ctox/bug-reports", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        summary: summary.trim(),
        expected: expected.trim(),
        pageUrl: window.location.href,
        moduleId,
        submoduleId,
        viewport: {
          width: window.innerWidth,
          height: window.innerHeight,
          scrollX: window.scrollX,
          scrollY: window.scrollY,
          devicePixelRatio: window.devicePixelRatio
        },
        annotation: attachment,
        userAgent: window.navigator.userAgent
      })
    });
    const result = await response.json().catch(() => null) as { report?: { id?: string } } | null;

    if (!response.ok || !result?.report?.id) {
      setStatus("failed");
      return;
    }

    setReportId(result.report.id);
    setStatus("saved");
    setSummary("");
    setExpected("");
    setAttachment(null);
  }

  function startSelection() {
    setMarkup({ mode: "selecting" });
  }

  function beginSelect(event: PointerEvent<HTMLDivElement>) {
    if (markup.mode !== "selecting") return;
    const origin = { x: event.clientX, y: event.clientY };
    event.currentTarget.setPointerCapture(event.pointerId);
    setMarkup({ mode: "selecting", origin, rect: { ...origin, width: 0, height: 0 } });
  }

  function moveSelect(event: PointerEvent<HTMLDivElement>) {
    if (markup.mode !== "selecting" || !markup.origin) return;
    setMarkup({
      mode: "selecting",
      origin: markup.origin,
      rect: normalizeRect(markup.origin, { x: event.clientX, y: event.clientY })
    });
  }

  function endSelect(event: PointerEvent<HTMLDivElement>) {
    if (markup.mode !== "selecting" || !markup.rect) return;
    event.currentTarget.releasePointerCapture(event.pointerId);
    if (markup.rect.width < 12 || markup.rect.height < 12) {
      setMarkup({ mode: "selecting" });
      return;
    }
    setMarkup({ mode: "drawing", rect: markup.rect, strokes: [] });
  }

  function beginStroke(event: PointerEvent<HTMLDivElement>) {
    if (markup.mode !== "drawing") return;
    event.currentTarget.setPointerCapture(event.pointerId);
    const point = relativePoint(event, markup.rect);
    setMarkup({ ...markup, activeStroke: [point] });
  }

  function moveStroke(event: PointerEvent<HTMLDivElement>) {
    if (markup.mode !== "drawing" || !markup.activeStroke) return;
    setMarkup({ ...markup, activeStroke: [...markup.activeStroke, relativePoint(event, markup.rect)] });
  }

  function endStroke(event: PointerEvent<HTMLDivElement>) {
    if (markup.mode !== "drawing" || !markup.activeStroke) return;
    event.currentTarget.releasePointerCapture(event.pointerId);
    setMarkup({
      mode: "drawing",
      rect: markup.rect,
      strokes: [...markup.strokes, markup.activeStroke]
    });
  }

  async function saveMarkup() {
    if (markup.mode !== "drawing") return;
    setSavingMarkup(true);
    const rect = markup.rect;
    const strokes = markup.activeStroke ? [...markup.strokes, markup.activeStroke] : markup.strokes;
    const markupSvgDataUrl = buildSvgDataUrl(rect, strokes);
    setMarkup({ mode: "idle" });

    try {
      const screenshotDataUrl = await captureScreenRegion(rect);
      const compositeDataUrl = screenshotDataUrl
        ? await buildCompositeDataUrl(rect, strokes, screenshotDataUrl).catch(() => markupSvgDataUrl)
        : markupSvgDataUrl;

      setAttachment({
        rect,
        strokes,
        svgDataUrl: compositeDataUrl,
        markupSvgDataUrl,
        compositeDataUrl,
        captureMode: screenshotDataUrl ? "screen" : "markup-only",
        capturedAt: new Date().toISOString(),
        ...(screenshotDataUrl ? { screenshotDataUrl } : {})
      });
    } finally {
      setSavingMarkup(false);
    }
  }

  return (
    <div className="bug-report-widget" data-open={open ? "true" : "false"}>
      <button className="bug-report-trigger" onClick={() => setOpen(true)} type="button">
        {t.bug}
      </button>
      {open ? (
        <aside className="bug-report-panel" aria-label={t.title}>
          <header>
            <strong>{t.title}</strong>
            <button onClick={() => setOpen(false)} type="button">{t.close}</button>
          </header>
          <label>
            {t.summary}
            <textarea
              autoFocus
              onChange={(event) => setSummary(event.target.value)}
              placeholder={t.summaryPlaceholder}
              value={summary}
            />
          </label>
          <label>
            {t.expected}
            <textarea
              onChange={(event) => setExpected(event.target.value)}
              placeholder={t.expectedPlaceholder}
              value={expected}
            />
          </label>
          <div className="bug-report-context">
            <span>{t.context}</span>
            <strong>{[moduleId, submoduleId].filter(Boolean).join(" / ") || "workspace"}</strong>
          </div>
          <div className="bug-report-markup-row">
            <button onClick={startSelection} type="button">{t.markArea}</button>
            {attachment ? (
              <button onClick={() => setAttachment(null)} type="button">{t.removeAttachment}</button>
            ) : null}
          </div>
          {attachment ? (
            <div className="bug-report-attachment">
              <span>
                {attachment.captureMode === "screen" ? t.attachmentScreen : t.attachmentFallback}
              </span>
              <img alt={t.attachment} src={attachment.compositeDataUrl} />
            </div>
          ) : null}
          <footer>
            <span>
              {status === "saved" ? `${t.saved}${reportId ? `: ${reportId.slice(0, 8)}` : ""}` : ""}
              {status === "failed" ? t.failed : ""}
            </span>
            <button disabled={!summary.trim() || status === "submitting"} onClick={submit} type="button">
              {status === "submitting" ? t.submitting : t.submit}
            </button>
          </footer>
        </aside>
      ) : null}
      {markup.mode !== "idle" ? (
        <div
          className="bug-markup-overlay"
          onPointerDown={beginSelect}
          onPointerMove={moveSelect}
          onPointerUp={endSelect}
        >
          <div className="bug-markup-toolbar" onPointerDown={(event) => event.stopPropagation()}>
            <strong>{t.markupTitle}</strong>
            <span>{t.markupHint}</span>
            <button onClick={() => setMarkup({ mode: "idle" })} type="button">{t.cancelMarkup}</button>
            {markup.mode === "drawing" ? <button disabled={savingMarkup} onClick={saveMarkup} type="button">{savingMarkup ? t.submitting : t.saveMarkup}</button> : null}
            {markup.mode === "drawing" ? <button onClick={() => setMarkup({ mode: "drawing", rect: markup.rect, strokes: [] })} type="button">{t.clearMarkup}</button> : null}
          </div>
          {visibleMarkup ? <SelectionRect markup={visibleMarkup} onPointerDown={beginStroke} onPointerMove={moveStroke} onPointerUp={endStroke} /> : null}
        </div>
      ) : null}
    </div>
  );
}

function SelectionRect({
  markup,
  onPointerDown,
  onPointerMove,
  onPointerUp
}: {
  markup: MarkupWithRect;
  onPointerDown: (event: PointerEvent<HTMLDivElement>) => void;
  onPointerMove: (event: PointerEvent<HTMLDivElement>) => void;
  onPointerUp: (event: PointerEvent<HTMLDivElement>) => void;
}) {
  const strokes = markup.mode === "drawing"
    ? [...markup.strokes, ...(markup.activeStroke ? [markup.activeStroke] : [])]
    : [];

  return (
    <div
      className="bug-markup-selection"
      onPointerDown={markup.mode === "drawing" ? onPointerDown : undefined}
      onPointerMove={markup.mode === "drawing" ? onPointerMove : undefined}
      onPointerUp={markup.mode === "drawing" ? onPointerUp : undefined}
      style={{
        height: markup.rect.height,
        left: markup.rect.x,
        top: markup.rect.y,
        width: markup.rect.width
      }}
    >
      <svg height="100%" viewBox={`0 0 ${markup.rect.width} ${markup.rect.height}`} width="100%">
        {strokes.map((stroke, index) => (
          <polyline fill="none" key={index} points={stroke.map((point) => `${point.x},${point.y}`).join(" ")} stroke="#d14b3f" strokeLinecap="round" strokeLinejoin="round" strokeWidth="4" />
        ))}
      </svg>
    </div>
  );
}

function normalizeRect(start: Point, end: Point): Rect {
  const x = Math.min(start.x, end.x);
  const y = Math.min(start.y, end.y);
  return {
    x,
    y,
    width: Math.abs(end.x - start.x),
    height: Math.abs(end.y - start.y)
  };
}

function relativePoint(event: PointerEvent<HTMLDivElement>, rect: Rect): Point {
  return {
    x: Math.max(0, Math.min(rect.width, event.clientX - rect.x)),
    y: Math.max(0, Math.min(rect.height, event.clientY - rect.y))
  };
}

function buildSvgDataUrl(rect: Rect, strokes: Stroke[]) {
  const polylines = strokes.map((stroke) => {
    const points = stroke.map((point) => `${point.x.toFixed(1)},${point.y.toFixed(1)}`).join(" ");
    return `<polyline points="${points}" fill="none" stroke="#d14b3f" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>`;
  }).join("");
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${rect.width}" height="${rect.height}" viewBox="0 0 ${rect.width} ${rect.height}"><rect width="100%" height="100%" fill="rgba(209,75,63,0.08)" stroke="#d14b3f" stroke-width="2"/>${polylines}</svg>`;
  return `data:image/svg+xml;base64,${window.btoa(svg)}`;
}

async function captureScreenRegion(rect: Rect): Promise<string | null> {
  if (!navigator.mediaDevices?.getDisplayMedia) return null;

  let stream: MediaStream;
  try {
    stream = await navigator.mediaDevices.getDisplayMedia({
      video: {
        displaySurface: "browser"
      } as MediaTrackConstraints,
      audio: false
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
    const context = canvas.getContext("2d");
    if (!context) return null;

    context.drawImage(video, sourceX, sourceY, sourceWidth, sourceHeight, 0, 0, sourceWidth, sourceHeight);
    return canvas.toDataURL("image/png");
  } finally {
    stream.getTracks().forEach((track) => track.stop());
  }
}

async function buildCompositeDataUrl(rect: Rect, strokes: Stroke[], screenshotDataUrl: string) {
  const image = await loadImage(screenshotDataUrl);
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, image.naturalWidth || Math.round(rect.width));
  canvas.height = Math.max(1, image.naturalHeight || Math.round(rect.height));
  const context = canvas.getContext("2d");
  if (!context) return screenshotDataUrl;

  context.drawImage(image, 0, 0, canvas.width, canvas.height);
  context.strokeStyle = "#d14b3f";
  context.lineWidth = Math.max(3, Math.round(canvas.width / Math.max(120, rect.width) * 4));
  context.lineCap = "round";
  context.lineJoin = "round";

  const scaleX = canvas.width / rect.width;
  const scaleY = canvas.height / rect.height;
  strokes.forEach((stroke) => {
    if (stroke.length < 2) return;
    context.beginPath();
    context.moveTo(stroke[0]!.x * scaleX, stroke[0]!.y * scaleY);
    stroke.slice(1).forEach((point) => {
      context.lineTo(point.x * scaleX, point.y * scaleY);
    });
    context.stroke();
  });

  return canvas.toDataURL("image/png");
}

function loadImage(src: string) {
  return new Promise<HTMLImageElement>((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("Could not load screenshot"));
    image.src = src;
  });
}

function waitForVideoFrame(video: HTMLVideoElement) {
  const candidate = video as HTMLVideoElement & {
    requestVideoFrameCallback?: (callback: () => void) => void;
  };

  if (candidate.requestVideoFrameCallback) {
    return new Promise<void>((resolve) => candidate.requestVideoFrameCallback?.(() => resolve()));
  }

  return new Promise<void>((resolve) => window.setTimeout(resolve, 160));
}
