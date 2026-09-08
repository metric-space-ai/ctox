import { resolveWindowLayout } from './window-layout-resolver.js';
import { createWindowCloseGate } from './window-close-gate.js';

const CONST = {
  CASCADE_STEP: 22,
  SNAP_EDGE: 30,
  SNAP_CORNER: 60,
  MIN_WIDTH: 320,
  MIN_HEIGHT: 200,
  ALWAYS_ON_TOP_Z: 9000,
  MOBILE_SHEET_MAX_WIDTH: 600,
  WORKSPACE_BOTTOM_GAP: 8,
};

// Motion timing mirror. The single source of truth is --motion-base in
// app.css; read it once and fall back to the token's value when computed
// styles are unavailable (non-DOM test environments).
const MOTION_BASE_FALLBACK_MS = 160;
const SHELL_V2_MORPH_DURATION_MS = 560;
// At 120 Hz this keeps each quadratic-spline sample below one display frame.
// The former 25-point path exposed its straight subsegments during the large
// close/open travel and read as discrete stages on high-refresh displays.
const SHELL_V2_MORPH_FRAME_COUNT = 121;
let motionBaseMsCache = null;

function motionBaseMs() {
  if (motionBaseMsCache != null) return motionBaseMsCache;
  let value = MOTION_BASE_FALLBACK_MS;
  try {
    const raw = getComputedStyle(document.documentElement).getPropertyValue('--motion-base').trim();
    const parsed = raw.endsWith('ms')
      ? parseFloat(raw)
      : raw.endsWith('s')
        ? parseFloat(raw) * 1000
        : NaN;
    if (Number.isFinite(parsed) && parsed >= 0) value = parsed;
  } catch {
    // Keep the fallback.
  }
  motionBaseMsCache = value;
  return value;
}

function prefersReducedMotion() {
  try {
    return typeof globalThis.matchMedia === 'function'
      && globalThis.matchMedia('(prefers-reduced-motion: reduce)').matches;
  } catch {
    return false;
  }
}

// Every app window gets this exact action set from the shared shell chrome.
// The visual order may follow the selected desktop style, but no app may
// remove an action or provide a second, partial control strip.
export const SHELL_WINDOW_CONTROL_ACTIONS = Object.freeze([
  'minimize',
  'maximize',
  'close',
]);

export const SHELL_WINDOW_CHROME_VERSION = 'shared-v1';
export const SHELL_WINDOW_V2_CHROME_VERSION = 'shared-v2';

const CONTROL_KINDS_BY_STYLE = {
  windows: SHELL_WINDOW_CONTROL_ACTIONS,
  macos: ['close', 'minimize', 'maximize'],
};

const CONTROL_GLYPHS = {
  minimize: '−',
  maximize: '□',
  restore: '❐',
  close: '×',
};

const RESIZE_HANDLES = ['n', 's', 'e', 'w', 'nw', 'ne', 'sw', 'se'];
const V2_RESIZE_HANDLES = ['nw', 'ne', 'sw', 'se'];

const SNAP_ZONES = ['left', 'right', 'top', 'bottom', 'top-left', 'top-right', 'bottom-left', 'bottom-right'];

export function clampNormalWindowPosition({ left, top, width, height }, viewport) {
  const vp = viewport || {};
  const minLeft = Math.max(0, Number(vp.left) || 0);
  const minTop = Math.max(0, Number(vp.top) || 0);
  const rightEdge = Math.max(minLeft, (Number(vp.w) || 0) - (Number(vp.right) || 0));
  const bottomEdge = Math.max(minTop, (Number(vp.h) || 0) - (Number(vp.bottom) || 0));
  const safeWidth = Math.max(0, Number(width) || 0);
  const safeHeight = Math.max(0, Number(height) || 0);
  const maxLeft = Math.max(minLeft, rightEdge - safeWidth);
  const maxTop = Math.max(minTop, bottomEdge - safeHeight);
  const requestedLeft = Number.isFinite(Number(left)) ? Number(left) : minLeft;
  const requestedTop = Number.isFinite(Number(top)) ? Number(top) : minTop;
  return {
    left: Math.max(minLeft, Math.min(maxLeft, requestedLeft)),
    top: Math.max(minTop, Math.min(maxTop, requestedTop)),
  };
}

export function detectSnapZone(x, y, viewport) {
  const vp = viewport || {};
  const leftEdge = Number(vp.left) || 0;
  const rightEdge = (Number(vp.w) || 0) - (Number(vp.right) || 0);
  const topEdge = Number(vp.top) || 0;
  const bottomEdge = (Number(vp.h) || 0) - (Number(vp.bottom) || 0);
  const edge = CONST.SNAP_EDGE;
  // A corner must fit inside both edge bands. SNAP_CORNER is intentionally
  // capped because its larger interaction radius must not consume an edge.
  const corner = Math.min(CONST.SNAP_CORNER, edge);
  const xc = Math.min(Math.max(x, leftEdge), rightEdge);
  const yc = Math.min(Math.max(y, topEdge), bottomEdge);

  if (yc < topEdge + corner && xc < leftEdge + corner) return 'top-left';
  if (yc < topEdge + corner && xc > rightEdge - corner) return 'top-right';
  if (yc > bottomEdge - corner && xc < leftEdge + corner) return 'bottom-left';
  if (yc > bottomEdge - corner && xc > rightEdge - corner) return 'bottom-right';
  if (yc < topEdge + edge) return 'top';
  if (yc > bottomEdge - edge) return 'bottom';
  if (xc < leftEdge + edge) return 'left';
  if (xc > rightEdge - edge) return 'right';
  return null;
}

export function defaultWindowPosition({ shellContract = 'v1', width, height, cascadeOffset = 0 }, viewport) {
  const vp = viewport || {};
  const leftInset = Math.max(0, Number(vp.left) || 0);
  const rightInset = Math.max(0, Number(vp.right) || 0);
  const topInset = Math.max(0, Number(vp.top) || 0);
  const bottomInset = Math.max(0, Number(vp.bottom) || 0);
  if (shellContract !== 'v2') {
    return { left: 80 + cascadeOffset, top: 60 + cascadeOffset };
  }
  const workWidth = Math.max(0, (Number(vp.w) || 0) - leftInset - rightInset);
  const workHeight = Math.max(0, (Number(vp.h) || 0) - topInset - bottomInset);
  return {
    left: leftInset + Math.max(0, (workWidth - Math.max(0, Number(width) || 0)) / 2),
    top: topInset + Math.max(0, (workHeight - Math.max(0, Number(height) || 0)) / 2),
  };
}

export function shellV2RenderedIconSizeFromAnchor(anchor) {
  const width = Number(anchor?.width);
  const height = Number(anchor?.height);
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return null;
  if (Math.abs(width - height) > 1) return null;
  return width;
}

function rgbToHex([r, g, b]) {
  return `#${[r, g, b].map((value) => Math.max(0, Math.min(255, Math.round(value))).toString(16).padStart(2, '0')).join('')}`;
}

function colorLuma([r, g, b]) {
  return (0.2126 * r) + (0.7152 * g) + (0.0722 * b);
}

function colorSaturation([r, g, b]) {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  return max === 0 ? 0 : (max - min) / max;
}

/**
 * Derive the complete shell palette from the rendered app artwork. Pixels are
 * quantised before ranking so anti-aliasing and JPEG noise cannot make the
 * frame flicker between nearly-identical colours. The manifest palette is a
 * loading/error fallback only; successful raster analysis is authoritative.
 */
export function shellV2FramePaletteFromRgba(rgba, width = 0, height = 0) {
  const buckets = new Map();
  const edgeBuckets = { topRight: new Map(), bottomLeft: new Map() };
  const luminous = [];
  const pixelWidth = Number(width) > 0 ? Math.floor(Number(width)) : 0;
  const pixelHeight = Number(height) > 0 ? Math.floor(Number(height)) : 0;
  const addBucket = (target, rgb, alpha, saturation) => {
    const quantised = rgb.map((value) => Math.min(255, Math.round(value / 24) * 24));
    const key = quantised.join(',');
    const entry = target.get(key) || { rgb: [0, 0, 0], weight: 0, count: 0 };
    const weight = (alpha / 255) * (0.42 + saturation);
    entry.rgb = entry.rgb.map((value, channel) => value + (rgb[channel] * weight));
    entry.weight += weight;
    entry.count += 1;
    target.set(key, entry);
  };
  for (let index = 0; index + 3 < rgba.length; index += 4) {
    const alpha = rgba[index + 3];
    if (alpha < 48) continue;
    const rgb = [rgba[index], rgba[index + 1], rgba[index + 2]];
    const luma = colorLuma(rgb);
    const saturation = colorSaturation(rgb);
    if (luma >= 46 && luma <= 250) luminous.push({ rgb, score: luma + (saturation * 22) + (alpha / 255) });
    // Ignore white/black glyph ink and transparent padding. The frame should
    // continue the icon's coloured body, not its foreground pictogram.
    if (luma < 14 || luma > 244 || (luma > 214 && saturation < 0.18)) continue;
    addBucket(buckets, rgb, alpha, saturation);
    if (pixelWidth && pixelHeight) {
      const pixel = index / 4;
      const x = pixel % pixelWidth;
      const y = Math.floor(pixel / pixelWidth);
      // The 6px frame meets the artwork at two corners, not across the entire
      // right/bottom edge. Sampling the local corner patches avoids averaging
      // unrelated icon colours into a visibly discontinuous joint.
      const edgeX = Math.max(2, Math.round(pixelWidth * 0.12));
      const edgeY = Math.max(2, Math.round(pixelHeight * 0.12));
      if (x >= pixelWidth - edgeX && y < edgeY) addBucket(edgeBuckets.topRight, rgb, alpha, saturation);
      if (x < edgeX && y >= pixelHeight - edgeY) addBucket(edgeBuckets.bottomLeft, rgb, alpha, saturation);
    }
  }
  const ranked = [...buckets.values()]
    .filter((entry) => entry.weight > 0)
    .map((entry) => ({
      rgb: entry.rgb.map((value) => value / entry.weight),
      score: entry.weight * Math.sqrt(entry.count),
    }))
    .sort((a, b) => b.score - a.score)
    .slice(0, 12);
  if (ranked.length < 2) return null;

  // Retain dominant colours, then order them from luminous/energetic at the
  // icon corner to calm/dark at the opposite frame corner.
  const candidates = ranked.slice(0, Math.min(8, ranked.length));
  const start = [...candidates].sort((a, b) => (
    (colorLuma(b.rgb) + colorSaturation(b.rgb) * 48)
    - (colorLuma(a.rgb) + colorSaturation(a.rgb) * 48)
  ))[0];
  const end = [...candidates].sort((a, b) => colorLuma(a.rgb) - colorLuma(b.rgb))[0];
  const middle = candidates.find((entry) => entry !== start && entry !== end) || candidates[0];
  const mix = (from, to, amount) => from.map((value, channel) => value + ((to[channel] - value) * amount));
  const dominantEdge = (target, fallback) => {
    const entry = [...target.values()]
      .filter((item) => item.weight > 0)
      .map((item) => ({ ...item, score: item.weight * Math.sqrt(item.count) }))
      .sort((a, b) => b.score - a.score)[0];
    return entry ? entry.rgb.map((value) => value / entry.weight) : fallback;
  };
  const topJoint = dominantEdge(edgeBuckets.topRight, mix(start.rgb, middle.rgb, 0.72));
  const leftJoint = dominantEdge(edgeBuckets.bottomLeft, mix(middle.rgb, end.rgb, 0.42));
  const contentAccent = luminous.sort((a, b) => b.score - a.score)[0]?.rgb || start.rgb;
  const contentForeground = colorLuma(contentAccent) >= 154 ? [7, 16, 21] : [248, 250, 252];
  return {
    start: rgbToHex(start.rgb),
    middle: rgbToHex(middle.rgb),
    top_joint: rgbToHex(topJoint),
    left_joint: rgbToHex(leftJoint),
    end: rgbToHex(end.rgb),
    accent: rgbToHex(topJoint),
    content_accent: rgbToHex(contentAccent),
    content_foreground: rgbToHex(contentForeground),
  };
}

export function shellV2FrameSampleAt(x, y, iconWidthRatio, iconHeightRatio) {
  const clamp01 = (value) => Math.max(0, Math.min(1, Number(value) || 0));
  const px = clamp01(x);
  const py = clamp01(y);
  const horizontalBreak = Math.max(0.001, clamp01(iconWidthRatio));
  const verticalBreak = Math.max(0.001, clamp01(iconHeightRatio));
  const distances = [py, 1 - px, 1 - py, px];
  const edge = distances.indexOf(Math.min(...distances));
  if (edge === 0) {
    return px <= horizontalBreak
      ? { from: 'start', to: 'topJoint', amount: px / horizontalBreak }
      : { from: 'topJoint', to: 'end', amount: (px - horizontalBreak) / (1 - horizontalBreak) };
  }
  if (edge === 1) return { from: 'end', to: 'end', amount: 1 };
  if (edge === 2) return { from: 'leftJoint', to: 'end', amount: px };
  return py <= verticalBreak
    ? { from: 'start', to: 'leftJoint', amount: py / verticalBreak }
    : { from: 'leftJoint', to: 'leftJoint', amount: 1 };
}

export function shellV2MorphFrameData(finalRect, anchor, frameCount = SHELL_V2_MORPH_FRAME_COUNT) {
  const count = Math.max(3, Math.floor(Number(frameCount) || SHELL_V2_MORPH_FRAME_COUNT));
  const mix = (from, to, amount) => from + (to - from) * amount;
  const clamp01 = (value) => Math.max(0, Math.min(1, value));
  const smootherstep = (value) => {
    const clamped = clamp01(value);
    return clamped * clamped * clamped * (clamped * (clamped * 6 - 15) + 10);
  };
  const deltaX = anchor.left - finalRect.left;
  const deltaY = anchor.top - finalRect.top;
  const control = {
    // Keep the quadratic control point near the launcher. Reversing the same
    // path for close therefore pulls the upper-left window corner toward its
    // icon early, instead of shrinking in place and travelling only at the
    // end. A small perpendicular offset makes the trajectory spline-like.
    left: mix(anchor.left, finalRect.left, 0.28)
      - Math.sign(deltaY || 1) * Math.min(42, Math.abs(deltaY) * 0.12),
    top: mix(anchor.top, finalRect.top, 0.28)
      + Math.sign(deltaX || 1) * Math.min(42, Math.abs(deltaX) * 0.12),
  };
  const splinePoint = (amount) => {
    const inverse = 1 - amount;
    return {
      left: inverse * inverse * anchor.left + 2 * inverse * amount * control.left + amount * amount * finalRect.left,
      top: inverse * inverse * anchor.top + 2 * inverse * amount * control.top + amount * amount * finalRect.top,
    };
  };
  return Array.from({ length: count }, (_, index) => {
    const amount = index / (count - 1);
    const geometryAmount = smootherstep(amount);
    const point = splinePoint(amount);
    const scaleX = mix(anchor.width / finalRect.width, 1, geometryAmount);
    const scaleY = mix(anchor.height / finalRect.height, 1, geometryAmount);
    // Fusion is deliberately confined to the first 14% of opening (and,
    // because close reverses these frames, the last 14% of closing). The
    // window therefore stays square while it travels and rounds only once it
    // has actually reached the desktop icon.
    const fusionAmount = smootherstep(amount / 0.14);
    const contentAmount = smootherstep((amount - 0.72) / 0.28);
    // Corner brackets do not merely fade. During the final close phase their
    // existing transform origins collapse each L into its corresponding frame
    // corner, where the rounded 6px frame geometrically absorbs it. Reversing
    // the same samples grows the brackets back out of those exact corners.
    const cornerAmount = smootherstep(amount / 0.14);
    return {
      amount,
      point,
      scaleX,
      scaleY,
      width: finalRect.width * scaleX,
      height: finalRect.height * scaleY,
      radius: anchor.radius * (1 - fusionAmount),
      iconInset: mix(0, -6, fusionAmount),
      iconRadius: anchor.radius * (1 - fusionAmount),
      contentOpacity: contentAmount,
      cornerScale: cornerAmount,
      cornerOpacity: cornerAmount * cornerAmount,
    };
  });
}

export function createWindowManager({
  windowLayer,
  surfaceEl,
  rootEl,
  snapPreviewEl,
  eventBus,
  t,
  getSvgIcon = null,
  zBase = 10,
  persistence = null,
}) {
  if (!windowLayer || !surfaceEl) {
    throw new Error('windowManager: windowLayer and surfaceEl are required');
  }
  const translate = typeof t === 'function' ? t : (_, fallback) => fallback;
  const svgIconFor = typeof getSvgIcon === 'function' ? getSvgIcon : () => '';
  const bus = eventBus || stubBus();

  const windows = [];
  const stack = [];
  let focusedId = null;
  let activeLayoutCandidate = null;
  let chromeLayout = rootEl?.dataset?.desktopStyle === 'macos' ? 'macos' : 'windows';
  let insets = { top: 0, right: 0, bottom: 0, left: 0 };
  let affectNormalInsets = true;
  let transientInsets = false;
  const onViewportResize = () => reflowWindowsForInsets();
  globalThis.addEventListener?.('resize', onViewportResize, { passive: true });
  globalThis.visualViewport?.addEventListener?.('resize', onViewportResize, { passive: true });
  const surfaceResizeObserver = typeof ResizeObserver === 'function'
    ? new ResizeObserver(onViewportResize)
    : null;
  surfaceResizeObserver?.observe(surfaceEl);
  surfaceResizeObserver?.observe(windowLayer);

  function setChromeLayout(layout) {
    const next = layout === 'macos' ? 'macos' : 'windows';
    if (next === chromeLayout) return;
    chromeLayout = next;
    for (const win of windows) {
      renderControls(win.element.querySelector('.shell-window-controls'), chromeLayout, translate, win.shellContract);
      assertShellWindowChrome(win.element, win.shellContract);
      updateMaximizeControl(win, translate);
    }
  }

  function setInsets(next, options = {}) {
    const normalized = {
      top: Math.max(0, Number(next?.top) || 0),
      right: Math.max(0, Number(next?.right) || 0),
      bottom: Math.max(0, Number(next?.bottom) || 0),
      left: Math.max(0, Number(next?.left) || 0),
    };
    const nextAffectNormal = options?.affectNormal !== false;
    const nextTransient = options?.transient === true && nextAffectNormal;
    const unchanged = Object.keys(normalized).every((key) => normalized[key] === insets[key]);
    if (unchanged && nextTransient === transientInsets && nextAffectNormal === affectNormalInsets) return;

    const enteringTransient = nextTransient && !transientInsets;
    const leavingTransient = !nextTransient && transientInsets;
    if (enteringTransient) {
      for (const win of windows) captureInsetRestore(win);
    } else if (leavingTransient) {
      for (const win of windows) restoreInsetGeometry(win, { clear: true });
    } else if (nextTransient) {
      for (const win of windows) restoreInsetGeometry(win, { clear: false });
    }

    insets = normalized;
    affectNormalInsets = nextAffectNormal;
    transientInsets = nextTransient;
    reflowWindowsForInsets();
    bus.emit('window:insets_changed', {
      ...insets,
      transient: transientInsets,
      affectNormal: affectNormalInsets,
    });
  }

  function getViewport({ includeInsets = true } = {}) {
    const rect = surfaceEl.getBoundingClientRect();
    const layerRect = windowLayer.getBoundingClientRect();
    const activeInsets = includeInsets ? insets : { top: 0, right: 0, bottom: 0, left: 0 };
    return {
      w: layerRect.width,
      h: layerRect.height,
      originLeft: layerRect.left,
      originTop: layerRect.top,
      top: Math.max(0, rect.top - layerRect.top) + activeInsets.top,
      right: Math.max(0, layerRect.right - rect.right) + activeInsets.right,
      bottom: Math.max(0, layerRect.bottom - rect.bottom) + activeInsets.bottom + CONST.WORKSPACE_BOTTOM_GAP,
      left: Math.max(0, rect.left - layerRect.left) + activeInsets.left,
    };
  }

  function getNormalViewport() {
    return getViewport({ includeInsets: affectNormalInsets });
  }

  function getMinimumWorkArea() {
    const visible = windows.filter((win) => win.state !== 'minimized' && win.element.style.display !== 'none');
    return {
      width: Math.max(CONST.MIN_WIDTH, ...visible.map((win) => win.minWidth || CONST.MIN_WIDTH)),
      height: Math.max(CONST.MIN_HEIGHT, ...visible.map((win) => win.minHeight || CONST.MIN_HEIGHT)),
    };
  }

  function captureInsetRestore(win) {
    if (!win || win.state !== 'normal' || win.element.classList.contains('is-snapped') || win._insetStored) return;
    win._insetStored = geometryStyles(win.element);
  }

  function restoreInsetGeometry(win, { clear = false } = {}) {
    if (!win?._insetStored || win.state !== 'normal' || win.element.classList.contains('is-snapped')) return;
    Object.assign(win.element.style, win._insetStored);
    if (clear) win._insetStored = null;
  }

  function clearInsetRestore(win) {
    if (win) win._insetStored = null;
  }

  function reflowWindowsForInsets() {
    for (const win of windows) {
      if (win.state === 'minimized' || win.element.style.display === 'none') continue;
      if (win.state === 'maximized') {
        applyMaximizedBounds(win);
      } else if (win.element.classList.contains('is-snapped')) {
        applySnapBounds(win, win.element.dataset.snapZone);
      } else {
        constrainNormalWindow(win);
      }
    }
    for (const win of windows) {
      if (win.state !== 'minimized' && win.dockRelation) restoreAppDock(win);
    }
  }

  function isMobileViewport(vp = getViewport()) {
    return vp.w <= CONST.MOBILE_SHEET_MAX_WIDTH;
  }

  function applyMobileSheetBounds(win, vp = getViewport()) {
    if (!win._mobileStored) win._mobileStored = geometryStyles(win.element);
    win.element.classList.add('is-mobile-sheet');
    const left = 0;
    const right = 0;
    Object.assign(win.element.style, {
      left: `${left}px`,
      top: `${vp.top}px`,
      width: `${Math.max(0, vp.w - left - right)}px`,
      height: `${Math.max(0, vp.h - vp.top - vp.bottom)}px`,
    });
    updateDynamicShadow(win.element);
  }

  function restoreFromMobileSheet(win) {
    if (!win.element.classList.contains('is-mobile-sheet')) return;
    win.element.classList.remove('is-mobile-sheet');
    if (win._mobileStored) Object.assign(win.element.style, win._mobileStored);
    win._mobileStored = null;
  }

  function constrainNormalWindow(win) {
    if (!win) return;
    const vp = getNormalViewport();
    const el = win.element;
    if (isMobileViewport(vp)) {
      applyMobileSheetBounds(win, vp);
      return;
    }
    restoreFromMobileSheet(win);
    const minWidth = win.minWidth || CONST.MIN_WIDTH;
    const minHeight = win.minHeight || CONST.MIN_HEIGHT;
    const usableWidth = Math.max(minWidth, vp.w - vp.left - vp.right);
    const usableHeight = Math.max(minHeight, vp.h - vp.top - vp.bottom);
    let width = Math.min(usableWidth, parsePx(el.style.width) || el.offsetWidth || minWidth);
    let height = Math.min(usableHeight, parsePx(el.style.height) || el.offsetHeight || minHeight);
    width = Math.max(minWidth, width);
    height = Math.max(minHeight, height);
    let left = parsePx(el.style.left);
    let top = parsePx(el.style.top);
    // Normal desktop windows must remain completely reachable. Allowing only
    // a sliver of the title bar to remain visible made restored or dragged
    // windows look detached from the Business OS canvas and could cover the
    // fixed chat composition controls. Width and height have already been
    // clamped to the usable viewport above, so full-window clamping is safe.
    ({ left, top } = clampNormalWindowPosition({ left, top, width, height }, vp));
    Object.assign(el.style, {
      left: `${left}px`,
      top: `${top}px`,
      width: `${width}px`,
      height: `${height}px`,
    });
    updateDynamicShadow(el);
  }

  function applyMaximizedBounds(win) {
    const vp = getViewport();
    if (isMobileViewport(vp)) win.element.classList.add('is-mobile-sheet');
    else win.element.classList.remove('is-mobile-sheet');
    const left = isMobileViewport(vp) ? 0 : vp.left;
    const right = isMobileViewport(vp) ? 0 : vp.right;
    Object.assign(win.element.style, {
      top: `${vp.top}px`,
      left: `${left}px`,
      width: `calc(100% - ${left + right}px)`,
      height: `calc(100% - ${vp.top + vp.bottom}px)`,
    });
  }

  function snapTargetStyles(zone, win = null) {
    const vp = getViewport();
    if (isMobileViewport(vp)) {
      const left = 0;
      const right = 0;
      return {
        top: `${vp.top}px`,
        left: `${left}px`,
        width: `${Math.max(0, vp.w - left - right)}px`,
        height: `${Math.max(0, vp.h - vp.top - vp.bottom)}px`,
      };
    }
    const minWidth = win?.minWidth || CONST.MIN_WIDTH;
    const minHeight = win?.minHeight || CONST.MIN_HEIGHT;
    const usableWidthPx = Math.max(minWidth, vp.w - vp.left - vp.right);
    const usableHeightPx = Math.max(minHeight, vp.h - vp.top - vp.bottom);
    const halfWidthPx = Math.min(usableWidthPx, Math.max(minWidth, usableWidthPx / 2));
    const halfHeightPx = Math.min(usableHeightPx, Math.max(minHeight, usableHeightPx / 2));
    const top = `${vp.top}px`;
    const left = `${vp.left}px`;
    const usableWidth = `${usableWidthPx}px`;
    const usableHeight = `${usableHeightPx}px`;
    const halfWidth = `${halfWidthPx}px`;
    const halfHeight = `${halfHeightPx}px`;
    const rightLeft = `${vp.left + usableWidthPx - halfWidthPx}px`;
    const bottomTop = `${vp.top + usableHeightPx - halfHeightPx}px`;
    return {
      left: { top, left, width: halfWidth, height: usableHeight },
      right: { top, left: rightLeft, width: halfWidth, height: usableHeight },
      top: { top, left, width: usableWidth, height: halfHeight },
      bottom: { top: bottomTop, left, width: usableWidth, height: halfHeight },
      'top-left': { top, left, width: halfWidth, height: halfHeight },
      'top-right': { top, left: rightLeft, width: halfWidth, height: halfHeight },
      'bottom-left': { top: bottomTop, left, width: halfWidth, height: halfHeight },
      'bottom-right': { top: bottomTop, left: rightLeft, width: halfWidth, height: halfHeight },
    }[zone] || null;
  }

  function applySnapBounds(win, zone) {
    const target = snapTargetStyles(zone, win);
    if (target) Object.assign(win.element.style, target);
  }

  function create(options = {}, legacyOwnerId) {
    const id = `desk_win_${secureToken()}`;
    const ownerId = options.ownerId || legacyOwnerId || null;
    const vp = getViewport();
    const minWidth = Math.max(CONST.MIN_WIDTH, parseInt(options.minWidth ?? options.min_width, 10) || CONST.MIN_WIDTH);
    const minHeight = Math.max(CONST.MIN_HEIGHT, parseInt(options.minHeight ?? options.min_height, 10) || CONST.MIN_HEIGHT);

    const winEl = document.createElement('section');
    // The shared tenant shell is v2 by default. A remaining explicit v1 value
    // is accepted only until the unreachable legacy renderer is deleted after
    // the fleet migration gate.
    const shellContract = options.shellContract === 'v1' ? 'v1' : 'v2';
    const resizeHandles = shellContract === 'v2' ? V2_RESIZE_HANDLES : RESIZE_HANDLES;
    winEl.className = 'shell-window';
    winEl.id = id;
    winEl.dataset.shellWindow = 'true';
    winEl.dataset.shellContract = shellContract;
    if (shellContract === 'v2') {
      winEl.dataset.shellHeaderRows = String(Math.max(2, Number.parseInt(options.shellHeaderRows, 10) || 2));
      winEl.dataset.shellIconRows = String(Math.max(2, Number.parseInt(options.shellIconRows, 10) || 2));
    }
    winEl.dataset.shellWindowChrome = shellContract === 'v2'
      ? SHELL_WINDOW_V2_CHROME_VERSION
      : SHELL_WINDOW_CHROME_VERSION;
    if (ownerId) winEl.dataset.ownerId = ownerId;
    winEl.style.transition = 'none';

    const shellGeometryContract = String(options.shellGeometryContract || '').trim();
    const persisted = ownerId && persistence?.load
      ? persistence.load(ownerId, { shellContract, shellGeometryContract })
      : null;
    const restored = persisted && (persisted.width || persisted.height || persisted.x != null || persisted.y != null);

    const maxInitialWidth = Math.max(minWidth, vp.w - vp.left - vp.right);
    const maxInitialHeight = Math.max(minHeight, vp.h - vp.top - vp.bottom);
    const width = Math.min(maxInitialWidth, Math.max(minWidth, parseInt(persisted?.width ?? options.width, 10) || 520));
    const height = Math.min(maxInitialHeight, Math.max(minHeight, parseInt(persisted?.height ?? options.height, 10) || 360));
    winEl.style.width = `${width}px`;
    winEl.style.height = `${height}px`;

    const cascadeOffset = (windows.length * CONST.CASCADE_STEP) % Math.max(80, Math.floor(vp.h / 3));
    // A fresh v2 surface is the focal workspace, not another cascading utility
    // window.  Centre its final rectangle like the approved shell reference;
    // persisted operator geometry remains authoritative on later launches.
    const defaultPosition = defaultWindowPosition({ shellContract, width, height, cascadeOffset }, vp);
    const defaultX = defaultPosition.left;
    const defaultY = defaultPosition.top;
    let baseX = parseInt(persisted?.x ?? options.x ?? defaultX, 10);
    let baseY = parseInt(persisted?.y ?? options.y ?? defaultY, 10);
    const maxX = Math.max(vp.left, vp.w - vp.right - 100);
    const maxY = Math.max(vp.top, vp.h - vp.bottom - 100);
    if (!Number.isFinite(baseX) || baseX < vp.left || baseX > maxX) baseX = Math.max(vp.left, 24);
    if (!Number.isFinite(baseY) || baseY < vp.top || baseY > maxY) baseY = Math.max(vp.top, 24);
    winEl.style.left = `${baseX}px`;
    winEl.style.top = `${baseY}px`;

    winEl.innerHTML = shellContract === 'v2' ? `
      <div class="shell-window-v2-icon" data-window-drag-region role="button" tabindex="0" aria-label="${escapeAttribute(`${options.title || 'App'} verschieben`)}">
        <img data-window-app-icon alt="" draggable="false" />
        <span data-window-app-label></span>
      </div>
      <header class="shell-window-header" data-window-header data-window-drag-region data-shell-v2-header-row="1">
        <div class="shell-window-title" data-window-title></div>
        <div class="shell-window-meta" data-window-meta></div>
        <div class="shell-window-actions" data-window-actions></div>
        <div class="shell-window-controls" data-window-controls data-window-control-strip></div>
      </header>
      <div class="shell-window-content" data-window-content></div>
      ${resizeHandles.map((dir) => `<div class="shell-window-resize shell-window-resize--${dir}" data-window-resize="${dir}" role="button" tabindex="0" aria-keyshortcuts="ArrowUp ArrowDown ArrowLeft ArrowRight" aria-label="${escapeAttribute(`${options.title || 'App'} an Ecke ${dir.toUpperCase()} skalieren`)}"></div>`).join('')}
    ` : `
      <header class="shell-window-header" data-window-header data-window-drag-region>
        <div class="shell-window-title" data-window-title></div>
        <div class="shell-window-meta" data-window-meta></div>
        <div class="shell-window-actions" data-window-actions></div>
        <div class="shell-window-controls" data-window-controls data-window-control-strip></div>
      </header>
      <div class="shell-window-content" data-window-content></div>
      ${resizeHandles.map((dir) => `<div class="shell-window-resize shell-window-resize--${dir}" data-window-resize="${dir}"></div>`).join('')}
    `;

    const titleEl = winEl.querySelector('[data-window-title]');
    const winIconKey = ownerId ? ownerId.replace(/^(desktop-app|module):/, '') : '';
    const svgHtml = svgIconFor(winIconKey, 14, 1.8);
    const escapeHtml = (str) => String(str).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
    titleEl.innerHTML = `${svgHtml ? `<span class="shell-window-title-icon" aria-hidden="true">${svgHtml}</span>` : ''}<span class="shell-window-title-text">${escapeHtml(options.title || composeTitle(options, translate))}</span>`;
    if (shellContract === 'v2') {
      const iconEl = winEl.querySelector('[data-window-app-icon]');
      const iconHost = iconEl?.closest('.shell-window-v2-icon');
      if (iconEl && options.iconAsset) {
        iconEl.crossOrigin = 'anonymous';
        iconEl.src = String(options.iconAsset);
        if (options.iconSrcSet) iconEl.srcset = String(options.iconSrcSet);
      } else {
        iconEl?.remove();
        iconHost?.classList.add('is-fallback');
      }
      const labelEl = winEl.querySelector('[data-window-app-label]');
      if (labelEl) {
        const label = options.title || composeTitle(options, translate);
        labelEl.textContent = String(label).trim().slice(0, 1).toUpperCase() || '•';
        labelEl.setAttribute('aria-label', String(label));
      }
      applyFramePalette(winEl, options.framePalette);
      if (iconEl) {
        const applyIconPalette = () => {
          const palette = framePaletteFromIcon(iconEl);
          if (!palette) return;
          applyFramePalette(winEl, palette);
          winEl.dataset.shellV2PaletteSource = 'icon';
          const record = windows.find((entry) => entry.element === winEl);
          if (record) refreshV2Chrome(record);
        };
        iconEl.addEventListener('load', applyIconPalette, { once: true });
        if (iconEl.complete && iconEl.naturalWidth > 0) queueMicrotask(applyIconPalette);
      }
    }
    renderHeaderItems(winEl.querySelector('[data-window-meta]'), options.headerBadges, 'meta');
    renderHeaderItems(winEl.querySelector('[data-window-actions]'), options.headerActions, 'action');
    const controlsEl = winEl.querySelector('[data-window-controls]');
    renderControls(controlsEl, chromeLayout, translate, shellContract);
    assertShellWindowChrome(winEl, shellContract);

    setTimeout(() => { winEl.style.transition = ''; }, 50);
    windowLayer.appendChild(winEl);

    const win = {
      id,
      ownerId,
      icon: options.icon || '',
      shellContract,
      shellGeometryContract,
      element: winEl,
      state: 'normal',
      minWidth,
      minHeight,
      stored: persisted?.stored
        ? { ...persisted.stored }
        : null,
      alwaysOnTop: !!persisted?.alwaysOnTop,
      dockRelation: persisted?.dockRelation || null,
      appMode: 'window',
      _destroying: false,
      _restored: restored,
      _onHostFileDrop: typeof options.onHostFileDrop === 'function' ? options.onHostFileDrop : null,
      _onHeaderAction: typeof options.onHeaderAction === 'function' ? options.onHeaderAction : null,
      _iconAnchorRect: typeof options.iconAnchorRect === 'function' ? options.iconAnchorRect : null,
    };
    windows.push(win);
    if (shellContract === 'v2') {
      if (typeof ResizeObserver === 'function') {
        win._v2ResizeObserver = new ResizeObserver(() => refreshV2Chrome(win));
        win._v2ResizeObserver.observe(winEl);
      }
      if (typeof MutationObserver === 'function') {
        win._v2MutationObserver = new MutationObserver(() => refreshV2Chrome(win));
        win._v2MutationObserver.observe(winEl, { childList: true, subtree: true });
      }
      queueMicrotask(() => refreshV2Chrome(win));
    }
    constrainNormalWindow(win);

    makeDraggable(win);
    for (const dir of resizeHandles) {
      makeResizable(win, dir);
    }
    if (shellContract === 'v2') bindShellV2KeyboardGeometry(win);
    setupFocus(win);
    bindControls(win);
    bindHeaderActions(win);
    bindHeaderGestures(win);
    bindHostFileDrop(win);
    updateDynamicShadow(winEl);

    if (options.content instanceof Node) {
      winEl.querySelector('[data-window-content]').appendChild(options.content);
    } else if (typeof options.content === 'string') {
      winEl.querySelector('[data-window-content]').innerHTML = options.content;
    }

    if (win.alwaysOnTop) {
      winEl.classList.add('is-always-on-top');
    }

    focus(id);

    if (persisted?.state === 'maximized') {
      win.stored = persisted.stored || null;
      toggleMaximize(id, { skipStore: true });
    } else if (persisted?.snapZone && SNAP_ZONES.includes(persisted.snapZone)) {
      snapTo(id, persisted.snapZone, { skipStore: true });
    } else if (win.dockRelation) {
      // Session windows are opened sequentially. Keep a valid-looking
      // relation pending until its target has had a chance to mount; the app
      // calls finalizeDockRestore once the restore batch is complete.
      restoreAppDock(win, { failClosed: false });
    }

    // Window-open animation: plays once from the final geometry (after any
    // maximized/snap restore above) and is purely visual — it never drives
    // geometry persistence; the 50 ms transition suppression at create time
    // still guards restored geometry from animating. Skipped for reduced
    // motion and for windows restored into maximized/snapped state, where a
    // scale-in reads as a glitch on an edge-docked surface.
    if (shellContract === 'v2') {
      prepareShellV2IconGeometry(win);
      animateShellV2Morph(win, 'open');
    } else if (!prefersReducedMotion() && win.state !== 'maximized' && !winEl.classList.contains('is-snapped')) {
      winEl.classList.add('is-opening');
      const clearOpening = () => winEl.classList.remove('is-opening');
      winEl.addEventListener('animationend', clearOpening, { once: true });
      setTimeout(clearOpening, motionBaseMs() + 60);
    }

    // A target opened later in the restore sequence can now resolve any
    // pending dependent without waiting for the final batch pass.
    reflowDockedDependents(win);

    bus.emit('window:opened', {
      id,
      ownerId: win.ownerId,
      title: titleEl.textContent,
      icon: win.icon,
      state: win.state,
      alwaysOnTop: win.alwaysOnTop,
    });

    return {
      id,
      ownerId,
      container: winEl.querySelector('[data-window-content]'),
      element: winEl,
      close: () => destroy(id),
      setTitle: (next) => {
        const text = String(next ?? '');
        const textEl = titleEl.querySelector('.shell-window-title-text');
        if (textEl) textEl.textContent = text;
        bus.emit('window:title_changed', { id, ownerId: win.ownerId, title: text });
      },
      setAlwaysOnTop: (flag) => setAlwaysOnTop(id, flag),
      snapTo: (zone) => snapTo(id, zone),
      minimize: () => minimize(id),
      maximize: () => {
        const current = windows.find((entry) => entry.id === id);
        if (current && current.state !== 'maximized') toggleMaximize(id);
      },
      restore: () => {
        const current = windows.find((entry) => entry.id === id);
        if (!current) return;
        if (current.state === 'minimized') focus(id);
        else if (current.state === 'maximized') toggleMaximize(id);
      },
    };
  }

  function focus(id) {
    const win = windows.find((w) => w.id === id);
    if (!win) return;
    if (focusedId === id) {
      restackZ();
      return;
    }
    if (focusedId) {
      const prev = windows.find((w) => w.id === focusedId);
      prev?.element.classList.remove('is-focused');
    }
    win.element.classList.add('is-focused');
    const wasMinimized = win.state === 'minimized' || win.element.style.display === 'none';
    if (wasMinimized) {
      win.element.classList.remove('is-minimizing');
      win.element.style.display = '';
      win.element.style.transform = '';
      win.element.style.opacity = '';
      win.state = 'normal';
      constrainNormalWindow(win);
    }
    focusedId = id;
    const without = stack.filter((winId) => winId !== id);
    stack.length = 0;
    stack.push(...without, id);
    restackZ();
    if (wasMinimized) bus.emit('window:restored', { id, ownerId: win.ownerId });
    bus.emit('window:focused', { id, ownerId: win.ownerId });
  }

  function restackZ() {
    stack.forEach((winId, index) => {
      const w = windows.find((entry) => entry.id === winId);
      if (!w) return;
      const z = w.alwaysOnTop
        ? CONST.ALWAYS_ON_TOP_Z + index * 2
        : zBase + index * 2;
      w.element.style.zIndex = String(z);
    });
  }

  function focusNextAfter(id) {
    if (focusedId !== id) return;
    focusedId = null;
    const candidate = [...stack].reverse().find(
      (winId) => winId !== id && windows.find(
        (w) => w.id === winId && w.state !== 'minimized' && !w.element.classList.contains('is-closing')
      )
    );
    if (candidate) focus(candidate);
  }

  function minimize(id) {
    const win = windows.find((w) => w.id === id);
    if (!win || win._minimizing || win.state === 'minimized') return;
    win._minimizing = true;
    const reduced = prefersReducedMotion();
    if (!reduced) win.element.classList.add('is-minimizing');
    setTimeout(() => {
      win.element.classList.remove('is-minimizing');
      win.element.style.display = 'none';
      win.state = 'minimized';
      win._minimizing = false;
      focusNextAfter(id);
      bus.emit('window:minimized', { id, ownerId: win.ownerId });
      persistFor(win);
    }, reduced ? 0 : motionBaseMs());
  }

  function minimizeAll() {
    const visible = windows.filter((win) => (
      win.state !== 'minimized'
      && win.element.style.display !== 'none'
      && !win._destroying
      && !win._minimizing
    ));
    for (const win of visible) minimize(win.id);
    return visible.length;
  }

  function toggleMaximize(id, { skipStore = false } = {}) {
    const win = windows.find((w) => w.id === id);
    if (!win) return;
    if (win.state === 'maximized') {
      restoreSize(win);
      updateMaximizeControl(win, translate);
      bus.emit('window:restored', { id, ownerId: win.ownerId });
      persistFor(win);
      return;
    }
    if (!skipStore && !win.element.classList.contains('is-snapped')) {
      win.stored = {
        width: win.element.style.width,
        height: win.element.style.height,
        top: win.element.style.top,
        left: win.element.style.left,
      };
    }
    clearInsetRestore(win);
    clearDockRelation(win);
    applyMaximizedBounds(win);
    win.element.classList.remove('is-snapped');
    win.element.removeAttribute('data-snap-zone');
    win.state = 'maximized';
    updateMaximizeControl(win, translate);
    bus.emit('window:maximized', { id, ownerId: win.ownerId });
    persistFor(win);
    reflowDockedDependents(win);
  }

  function restoreSize(win) {
    if (!win.stored) {
      win.state = 'normal';
      win.element.classList.remove('is-snapped');
      win.element.classList.remove('is-maximized');
      return;
    }
    win.element.style.width = win.stored.width || '520px';
    win.element.style.height = win.stored.height || '360px';
    win.element.style.top = win.stored.top || '60px';
    win.element.style.left = win.stored.left || '80px';
    win.element.classList.remove('is-snapped');
    clearDockRelation(win);
    win.element.removeAttribute('data-snap-zone');
    win.element.classList.remove('is-maximized');
    win.state = 'normal';
    constrainNormalWindow(win);
  }

  function snapTo(id, zone, { skipStore = false } = {}) {
    if (!SNAP_ZONES.includes(zone)) return;
    const win = windows.find((w) => w.id === id);
    if (!win) return;
    if (!skipStore && !win.element.classList.contains('is-snapped') && win.state !== 'maximized') {
      win.stored = {
        width: win.element.style.width,
        height: win.element.style.height,
        top: win.element.style.top,
        left: win.element.style.left,
      };
    }
    clearInsetRestore(win);
    clearDockRelation(win);
    applySnapBounds(win, zone);
    win.element.classList.add('is-snapped');
    win.element.dataset.snapZone = zone;
    win.state = 'normal';
    bus.emit('window:snapped', { id, ownerId: win.ownerId, zone });
    persistFor(win);
    reflowDockedDependents(win);
  }

  function setAlwaysOnTop(id, flag) {
    const win = windows.find((w) => w.id === id);
    if (!win) return;
    const next = !!flag;
    if (win.alwaysOnTop === next) return;
    win.alwaysOnTop = next;
    win.element.classList.toggle('is-always-on-top', next);
    restackZ();
    bus.emit('window:always_on_top_changed', { id, ownerId: win.ownerId, alwaysOnTop: next });
    persistFor(win);
  }

  function setAppMode(id, mode = 'window') {
    const win = windows.find((w) => w.id === id);
    if (!win) return;
    const next = ['window', 'maximized', 'focus'].includes(mode) ? mode : 'window';
    // Mode changes resize the complete app container. Suppress decorative
    // module transitions for the short geometry hand-off so complex apps do
    // not animate hundreds of descendants while the user is waiting for the
    // window itself to react. Functional state and mount identity stay intact.
    win.element.classList.add('is-layout-switching');
    clearTimeout(win._layoutSwitchTimer);
    win._layoutSwitchTimer = setTimeout(() => {
      win.element?.classList?.remove('is-layout-switching');
      win._layoutSwitchTimer = null;
    }, 140);
    win.element.classList.toggle('is-focus-mode', next === 'focus');
    win.element.dataset.appMode = next;
    if (next === 'window' && win.state === 'maximized') {
      toggleMaximize(id);
    } else if ((next === 'maximized' || next === 'focus') && win.state !== 'maximized') {
      toggleMaximize(id);
    }
    win.appMode = next;
    bus.emit('window:app_mode_changed', { id, ownerId: win.ownerId, mode: next });
  }

  function refreshV2Chrome(idOrWindow) {
    const win = typeof idOrWindow === 'string'
      ? windows.find((entry) => entry.id === idOrWindow)
      : idOrWindow;
    if (!win || win.shellContract !== 'v2' || !win.element?.isConnected) return;
    const windowRect = win.element.getBoundingClientRect();
    if (windowRect.width <= 0 || windowRect.height <= 0) return;
    const style = getComputedStyle(win.element);
    const parseColor = (token, fallback) => {
      const raw = style.getPropertyValue(token).trim();
      const match = raw.match(/^#([0-9a-f]{6})$/i);
      if (!match) return fallback;
      const value = Number.parseInt(match[1], 16);
      return [(value >> 16) & 255, (value >> 8) & 255, value & 255];
    };
    const start = parseColor('--shell-v2-frame-start', [240, 181, 111]);
    const topJoint = parseColor('--shell-v2-frame-top-joint', [147, 107, 99]);
    const leftJoint = parseColor('--shell-v2-frame-left-joint', [113, 83, 101]);
    const end = parseColor('--shell-v2-frame-end', [23, 52, 92]);
    const colors = { start, topJoint, leftJoint, end };
    const blend = (from, to, amount) => from.map((value, index) => Math.round(value + (to[index] - value) * amount));
    const iconSize = parseFloat(style.getPropertyValue('--shell-v2-icon-size')) || 80;
    const iconWidthRatio = Math.max(0.001, Math.min(1, iconSize / windowRect.width));
    const iconHeightRatio = Math.max(0.001, Math.min(1, iconSize / windowRect.height));
    const colorAt = (sample) => {
      const amount = Math.max(0, Math.min(1, Number(sample?.amount) || 0));
      const rgb = blend(colors[sample?.from] || start, colors[sample?.to] || end, amount);
      return `rgb(${rgb.join(' ')})`;
    };
    const selector = [
      '[data-window-drag-region]',
      '[data-window-control]',
      '[data-window-resize]',
    ].join(',');
    for (const element of win.element.querySelectorAll(selector)) {
      const rect = element.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) continue;
      const x = (rect.left + rect.width / 2 - windowRect.left) / windowRect.width;
      const y = (rect.top + rect.height / 2 - windowRect.top) / windowRect.height;
      element.style.setProperty(
        '--shell-v2-local-accent',
        colorAt(shellV2FrameSampleAt(x, y, iconWidthRatio, iconHeightRatio)),
      );
    }
  }

  function destroy(id) {
    const win = windows.find((w) => w.id === id);
    if (!win || win._destroying) return;
    if (win._closeGate) return win._closeGate.request(() => finishWindowDestroy(win));
    return finishWindowDestroy(win);
  }

  function registerCloseGuard(host, guard) {
    const win = windows.find((entry) => host && entry.element.contains(host));
    if (!win || win._destroying) return () => {};
    win._closeGate ||= createWindowCloseGate(() => {
      bus.emit('window:close_blocked', { id: win.id, ownerId: win.ownerId });
    });
    return win._closeGate.add(guard);
  }

  function finishWindowDestroy(win) {
    if (win._destroying) return;
    const id = win.id;
    win._destroying = true;
    bus.emit('window:closing', { id, ownerId: win.ownerId });
    for (const dependent of windows) {
      if (dependent.dockRelation?.targetOwnerId !== win.ownerId) continue;
      clearDockRelation(dependent);
      persistFor(dependent);
      bus.emit('window:undocked', {
        id: dependent.id,
        ownerId: dependent.ownerId,
        reason: 'dock-target-closed',
      });
    }
    clearTimeout(win._layoutSwitchTimer);
    win._v2ResizeObserver?.disconnect?.();
    win._v2MutationObserver?.disconnect?.();
    const stackIndex = stack.indexOf(id);
    if (stackIndex !== -1) stack.splice(stackIndex, 1);
    const finishDestroy = () => {
      win.element.remove();
      const idx = windows.findIndex((w) => w.id === id);
      if (idx !== -1) windows.splice(idx, 1);
      // Keep the closing v2 window focused until its corner brackets have
      // reached and fused with the desktop icon. Moving focus earlier applies
      // the inactive-window rule and makes those brackets vanish before the
      // morph even starts.
      focusNextAfter(id);
      bus.emit('window:closed', { id, ownerId: win.ownerId });
    };
    if (win.shellContract === 'v2') {
      animateShellV2Morph(win, 'close').then(finishDestroy);
      return;
    }
    const reduced = prefersReducedMotion();
    if (!reduced) win.element.classList.add('is-closing');
    setTimeout(finishDestroy, reduced ? 0 : motionBaseMs());
  }

  function shellV2IconAnchor(win) {
    if (win?.shellContract !== 'v2' || typeof win._iconAnchorRect !== 'function') return null;
    let viewportRect = null;
    try {
      viewportRect = win._iconAnchorRect();
    } catch {
      return null;
    }
    const width = Number(viewportRect?.width);
    const height = Number(viewportRect?.height);
    const left = Number(viewportRect?.left);
    const top = Number(viewportRect?.top);
    if (![width, height, left, top].every(Number.isFinite) || width <= 0 || height <= 0) return null;
    const layerRect = windowLayer.getBoundingClientRect();
    return {
      left: left - layerRect.left,
      top: top - layerRect.top,
      width,
      height,
      radius: Math.max(0, Number(viewportRect?.radius) || Math.min(width, height) * 0.28),
    };
  }

  function prepareShellV2IconGeometry(win) {
    const anchor = shellV2IconAnchor(win);
    if (!anchor) return null;
    // The shell and launcher deliberately share one rendered glyph size. The
    // measured launcher (64px default, 56px compact, 64 CSS px in Workjet)
    // remains authoritative for both the endpoint and the open-window glyph.
    // Never promote the 128px source canvas to a rendered size here.
    const renderedSize = shellV2RenderedIconSizeFromAnchor(anchor);
    if (renderedSize !== null) {
      win.element.style.setProperty('--shell-v2-icon-size', `${renderedSize}px`);
    }
    return anchor;
  }

  function animateShellV2Morph(win, direction) {
    const el = win?.element;
    if (!el || win.shellContract !== 'v2') return Promise.resolve();
    const anchor = prepareShellV2IconGeometry(win);
    const reduced = prefersReducedMotion();
    const canAnimate = typeof el.animate === 'function';
    if (!anchor || reduced || !canAnimate) {
      if (direction === 'close' && canAnimate && !reduced) {
        const fallback = el.animate(
          [{ opacity: 1, transform: 'scale(1)' }, { opacity: 0, transform: 'scale(0.985)' }],
          { duration: motionBaseMs(), easing: 'ease-out' },
        );
        return fallback.finished.catch(() => undefined);
      }
      return Promise.resolve();
    }

    const finalRect = windowRectFor(win);
    const morphData = shellV2MorphFrameData(finalRect, anchor);
    const openFrames = morphData.map((frame) => {
      return {
        left: `${frame.point.left}px`,
        top: `${frame.point.top}px`,
        width: `${frame.width}px`,
        height: `${frame.height}px`,
        transform: 'none',
        borderRadius: `${frame.radius}px`,
        offset: frame.amount,
      };
    });
    const frames = direction === 'close'
      ? openFrames.slice().reverse().map((frame, index) => ({ ...frame, offset: index / (openFrames.length - 1) }))
      : openFrames;

    el.classList.add('is-shell-v2-morphing');
    el.dataset.shellV2Morph = direction;
    const geometryAnimation = el.animate(frames, {
      duration: SHELL_V2_MORPH_DURATION_MS,
      easing: 'linear',
      fill: 'both',
    });
    const icon = el.querySelector('[data-window-app-icon]')?.closest('.shell-window-v2-icon');
    const openIconFrames = morphData.map((frame) => {
      return {
        transform: 'none',
        top: `${frame.iconInset}px`,
        left: `${frame.iconInset}px`,
        borderRadius: `${frame.iconRadius}px`,
        offset: frame.amount,
      };
    });
    const iconFrames = direction === 'close'
      ? openIconFrames.slice().reverse().map((frame, index) => ({ ...frame, offset: index / (openIconFrames.length - 1) }))
      : openIconFrames;
    const iconAnimation = icon?.animate(iconFrames, {
      duration: SHELL_V2_MORPH_DURATION_MS,
      easing: 'linear',
      fill: 'both',
    });
    const content = el.querySelector('[data-window-content]');
    const header = el.querySelector('[data-window-header]');
    const openFadeFrames = morphData.map((frame) => ({ opacity: frame.contentOpacity, offset: frame.amount }));
    const fadeFrames = direction === 'close'
      ? openFadeFrames.slice().reverse().map((frame, index) => ({ ...frame, offset: index / (openFadeFrames.length - 1) }))
      : openFadeFrames;
    const fades = [content, header].filter(Boolean).map((node) => node.animate(
      fadeFrames,
      { duration: SHELL_V2_MORPH_DURATION_MS, easing: 'ease-out', fill: 'both' },
    ));
    const corners = Array.from(el.querySelectorAll('[data-window-resize]'));
    const openCornerFrames = morphData.map((frame) => ({
      opacity: frame.cornerOpacity,
      transform: `scale(${frame.cornerScale})`,
      offset: frame.amount,
    }));
    const cornerFrames = direction === 'close'
      ? openCornerFrames.slice().reverse().map((frame, index) => ({ ...frame, offset: index / (openCornerFrames.length - 1) }))
      : openCornerFrames;
    const cornerAnimations = corners.map((node) => node.animate(
      cornerFrames,
      { duration: SHELL_V2_MORPH_DURATION_MS, easing: 'ease-out', fill: 'both' },
    ));
    return Promise.allSettled([
      geometryAnimation.finished,
      ...(iconAnimation ? [iconAnimation.finished] : []),
      ...fades.map((animation) => animation.finished),
      ...cornerAnimations.map((animation) => animation.finished),
    ]).then(() => {
      if (direction !== 'close' && el.isConnected) {
        geometryAnimation.cancel();
        iconAnimation?.cancel();
        fades.forEach((animation) => animation.cancel());
        cornerAnimations.forEach((animation) => animation.cancel());
        el.classList.remove('is-shell-v2-morphing');
        delete el.dataset.shellV2Morph;
      }
    });
  }

  function destroyAll() {
    for (const win of [...windows]) destroy(win.id);
  }

  function closeOthersOfOwner(id) {
    const win = windows.find((w) => w.id === id);
    if (!win || !win.ownerId) return;
    for (const other of [...windows]) {
      if (other.id !== id && other.ownerId === win.ownerId) destroy(other.id);
    }
  }

  function listWindows() {
    return windows.map((w) => ({
      id: w.id,
      ownerId: w.ownerId,
      icon: w.icon,
      state: w.state,
      alwaysOnTop: !!w.alwaysOnTop,
      appMode: w.appMode || 'window',
      minWidth: w.minWidth,
      minHeight: w.minHeight,
      title: w.element.querySelector('[data-window-title]')?.textContent || '',
      isFocused: focusedId === w.id,
    }));
  }

  function describe(id) {
    return listWindows().find((w) => w.id === id) || null;
  }

  function bindControls(win) {
    win.element.querySelector('[data-window-controls]').addEventListener('click', (event) => {
      const btn = event.target.closest('[data-window-control]');
      if (!btn) return;
      event.stopPropagation();
      const action = btn.dataset.windowControl;
      if (action === 'close') destroy(win.id);
      else if (action === 'minimize') minimize(win.id);
      else if (action === 'maximize') toggleMaximize(win.id);
    });
  }

  function bindHeaderActions(win) {
    win.element.querySelector('[data-window-header]')?.addEventListener('click', (event) => {
      const button = event.target.closest('[data-window-header-action]');
      if (!button || !win.element.contains(button)) return;
      event.preventDefault();
      event.stopPropagation();
      win._onHeaderAction?.(button.dataset.windowHeaderAction, {
        id: win.id,
        ownerId: win.ownerId,
        event,
      });
    });
  }

  function bindHeaderGestures(win) {
    if (win.shellContract === 'v2') return;
    const header = win.element.querySelector('[data-window-header]');
    if (!header) return;
    header.addEventListener('dblclick', (event) => {
      if (event.target.closest('[data-window-controls], [data-window-header-action]')) return;
      event.preventDefault();
      toggleMaximize(win.id);
    });
    header.addEventListener('contextmenu', (event) => {
      if (event.target.closest('[data-window-controls], [data-window-header-action]')) return;
      event.preventDefault();
      event.stopPropagation();
      bus.emit('window:context_request', {
        id: win.id,
        ownerId: win.ownerId,
        target: 'header',
        clientX: event.clientX,
        clientY: event.clientY,
        state: win.state,
        alwaysOnTop: !!win.alwaysOnTop,
      });
    });
  }

  function bindHostFileDrop(win) {
    const content = win.element.querySelector('[data-window-content]');
    if (!content) return;
    const ownsHandler = typeof win._onHostFileDrop === 'function';
    content.addEventListener('dragover', (event) => {
      const dt = event.dataTransfer;
      if (!dt || !Array.from(dt.types || []).includes('Files')) return;
      if (!ownsHandler && !bus) return;
      event.preventDefault();
      event.stopPropagation();
      dt.dropEffect = 'copy';
      win.element.classList.add('is-host-drop-target');
    });
    content.addEventListener('dragleave', (event) => {
      if (event.relatedTarget && content.contains(event.relatedTarget)) return;
      win.element.classList.remove('is-host-drop-target');
    });
    content.addEventListener('drop', (event) => {
      const dt = event.dataTransfer;
      if (!dt || !Array.from(dt.types || []).includes('Files')) return;
      event.preventDefault();
      event.stopPropagation();
      win.element.classList.remove('is-host-drop-target');
      const files = Array.from(dt.files || []);
      if (!files.length) return;
      const payload = {
        id: win.id,
        ownerId: win.ownerId,
        files,
        clientX: event.clientX,
        clientY: event.clientY,
      };
      if (ownsHandler) {
        try { win._onHostFileDrop(payload); } catch (error) { console.error('[windowManager] host file drop handler threw:', error); }
      }
      bus.emit('window:host_file_drop', payload);
    });
  }

  function setupFocus(win) {
    win.element.addEventListener('mousedown', () => focus(win.id));
  }

  function makeDraggable(win) {
    if (win.shellContract === 'v2') {
      makePointerDraggable(win);
      return;
    }
    const header = win.element.querySelector('[data-window-drag-region]');
    if (!header) return;
    header.addEventListener('mousedown', (downEvent) => {
      if (downEvent.button !== 0) return;
      if (win.element.classList.contains('is-mobile-sheet')) return;
      if (downEvent.target.closest('[data-window-controls], [data-window-header-action]')) return;
      clearInsetRestore(win);
      const el = win.element;
      let initialX = downEvent.clientX;
      let initialY = downEvent.clientY;
      let currentX = initialX;
      let currentY = initialY;
      let dragging = true;
      let rAFQueued = false;
      let dragFrame = 0;
      // Shell style can only change between drags (settings select), so
      // cache the ctox check once per drag instead of per frame.
      const trackDynamicShadow = dynamicShadowActive();

      function update() {
        const dx = initialX - currentX;
        const dy = initialY - currentY;
        initialX = currentX;
        initialY = currentY;
        const vp = getNormalViewport();
        const { top, left } = clampNormalWindowPosition({
          left: el.offsetLeft - dx,
          top: el.offsetTop - dy,
          width: el.offsetWidth,
          height: el.offsetHeight,
        }, vp);
        el.style.top = `${top}px`;
        el.style.left = `${left}px`;
        applySnapPreview(currentX, currentY);
        if (trackDynamicShadow) updateDynamicShadow(el);
      }

      function onMouseMove(moveEvent) {
        if (!dragging) return;
        currentX = moveEvent.clientX;
        currentY = moveEvent.clientY;
        if (win.state === 'maximized' || el.classList.contains('is-snapped')) {
          const ratio = (currentX - el.offsetLeft) / Math.max(1, el.offsetWidth);
          if (win.state === 'maximized') toggleMaximize(win.id);
          else {
            el.classList.remove('is-snapped');
            el.removeAttribute('data-snap-zone');
            if (win.stored?.width) el.style.width = win.stored.width;
            if (win.stored?.height) el.style.height = win.stored.height;
            constrainNormalWindow(win);
          }
          const newWidth = el.offsetWidth;
          initialX = currentX;
          el.style.left = `${currentX - newWidth * ratio}px`;
        }
        if (!rAFQueued) {
          rAFQueued = true;
          dragFrame = requestAnimationFrame(() => {
            dragFrame = 0;
            rAFQueued = false;
            if (dragging) update();
          });
        }
      }

      function onMouseUp() {
        // A fast pointer release can arrive before the last animation frame.
        // Evaluate that final position synchronously so snapping never depends
        // on how slowly the operator drags the title bar.
        if (dragFrame) cancelAnimationFrame(dragFrame);
        dragFrame = 0;
        if (rAFQueued) {
          rAFQueued = false;
          update();
        }
        dragging = false;
        document.removeEventListener('mousemove', onMouseMove);
        document.removeEventListener('mouseup', onMouseUp);
        commitSnap(win);
        bus.emit('window:moved', {
          id: win.id,
          ownerId: win.ownerId,
          top: el.style.top,
          left: el.style.left,
          width: el.style.width,
          height: el.style.height,
        });
        persistFor(win);
        reflowDockedDependents(win);
      }

      document.addEventListener('mousemove', onMouseMove);
      document.addEventListener('mouseup', onMouseUp);
    });
  }

  function makeResizable(win, direction) {
    const handle = win.element.querySelector(`[data-window-resize="${direction}"]`);
    if (!handle) return;
    if (win.shellContract === 'v2') {
      makePointerResizable(win, handle, direction);
      return;
    }
    handle.addEventListener('mousedown', (event) => {
      if (event.button !== 0) return;
      if (win.element.classList.contains('is-mobile-sheet')) return;
      if (win.state === 'maximized') return;
      clearInsetRestore(win);
      const el = win.element;
      const startWidth = el.offsetWidth;
      const startHeight = el.offsetHeight;
      const startLeft = el.offsetLeft;
      const startTop = el.offsetTop;
      const startX = event.clientX;
      const startY = event.clientY;
      event.stopPropagation();
      event.preventDefault();
      focus(win.id);
      let resizeRaf = 0;
      let pendingDX = 0;
      let pendingDY = 0;
      let hasPendingResize = false;
      let resizing = true;
      const vp = getNormalViewport();

      function applyResize() {
        resizeRaf = 0;
        if (!hasPendingResize) return;
        hasPendingResize = false;
        const dX = pendingDX;
        const dY = pendingDY;
        let newWidth = startWidth;
        let newHeight = startHeight;
        let newLeft = startLeft;
        let newTop = startTop;
        const minWidth = win.minWidth || CONST.MIN_WIDTH;
        const minHeight = win.minHeight || CONST.MIN_HEIGHT;
        if (direction.includes('e')) newWidth = Math.max(minWidth, startWidth + dX);
        if (direction.includes('w')) {
          const candidateWidth = Math.max(minWidth, startWidth - dX);
          newLeft = startLeft + (startWidth - candidateWidth);
          newWidth = candidateWidth;
        }
        if (direction.includes('s')) newHeight = Math.max(minHeight, startHeight + dY);
        if (direction.includes('n')) {
          const candidateHeight = Math.max(minHeight, startHeight - dY);
          newTop = Math.max(vp.top, startTop + (startHeight - candidateHeight));
          newHeight = candidateHeight;
        }
        const maxHeightFromTop = Math.max(minHeight, vp.h - vp.bottom - newTop);
        const maxWidthFromLeft = Math.max(minWidth, vp.w - vp.right - newLeft);
        newHeight = Math.min(newHeight, maxHeightFromTop);
        newWidth = Math.min(newWidth, maxWidthFromLeft);
        el.style.width = `${newWidth}px`;
        el.style.height = `${newHeight}px`;
        if (direction.includes('w')) el.style.left = `${newLeft}px`;
        if (direction.includes('n')) el.style.top = `${newTop}px`;
        if (el.classList.contains('is-snapped')) {
          el.classList.remove('is-snapped');
          el.removeAttribute('data-snap-zone');
          if (win.stored) {
            win.stored.width = null;
            win.stored.height = null;
          }
        }
      }

      function onMouseMove(moveEvent) {
        if (!resizing) return;
        pendingDX = moveEvent.clientX - startX;
        pendingDY = moveEvent.clientY - startY;
        hasPendingResize = true;
        if (!resizeRaf) {
          resizeRaf = requestAnimationFrame(applyResize);
        }
      }

      function onMouseUp() {
        if (resizeRaf) cancelAnimationFrame(resizeRaf);
        applyResize();
        resizing = false;
        document.removeEventListener('mousemove', onMouseMove);
        document.removeEventListener('mouseup', onMouseUp);
        bus.emit('window:resized', {
          id: win.id,
          ownerId: win.ownerId,
          width: el.style.width,
          height: el.style.height,
          top: el.style.top,
          left: el.style.left,
        });
        persistFor(win);
        reflowDockedDependents(win);
      }

      document.addEventListener('mousemove', onMouseMove);
      document.addEventListener('mouseup', onMouseUp);
    });
  }

  function makePointerDraggable(win) {
    const dragHandles = Array.from(win.element.querySelectorAll('[data-window-drag-region]'));
    if (!dragHandles.length) return;
    const interactiveSelector = [
      'button', 'a', 'input', 'select', 'textarea', '[contenteditable="true"]',
      '[role="button"]', '[role="tab"]', '[data-window-controls]',
      '[data-window-actions]', '[data-window-header-action]', '[data-resizer]',
    ].join(', ');
    const beginDrag = (event, handle, borderOnly = false) => {
      if (event.pointerType === 'mouse' && event.button !== 0) return;
      if (win.element.classList.contains('is-mobile-sheet')) return;
      if (handle.matches?.('[data-window-header]') && event.target.closest?.(interactiveSelector)) return;
      if (borderOnly) {
        if (event.target !== win.element) return;
        const rect = win.element.getBoundingClientRect();
        const borderWidth = Math.max(6, parseFloat(getComputedStyle(win.element).borderTopWidth) || 0);
        const onFrame = event.clientX <= rect.left + borderWidth
          || event.clientX >= rect.right - borderWidth
          || event.clientY <= rect.top + borderWidth
          || event.clientY >= rect.bottom - borderWidth;
        if (!onFrame) return;
      }
      event.preventDefault();
      focus(win.id);
      clearInsetRestore(win);
      const el = win.element;
      if (win.state === 'maximized') toggleMaximize(win.id);
      if (el.classList.contains('is-snapped') || win.dockRelation) {
        el.classList.remove('is-snapped', 'is-docked-to-app');
        el.removeAttribute('data-snap-zone');
        clearDockRelation(win);
        if (win.stored?.width) el.style.width = win.stored.width;
        if (win.stored?.height) el.style.height = win.stored.height;
        constrainNormalWindow(win);
      }
      const start = windowRectFor(win);
      const startX = event.clientX;
      const startY = event.clientY;
      let latestX = startX;
      let latestY = startY;
      let frame = 0;
      let active = true;
      activeLayoutCandidate = null;
      try { handle.setPointerCapture?.(event.pointerId); } catch {}

      const apply = () => {
        frame = 0;
        if (!active) return;
        const vp = getNormalViewport();
        const position = clampNormalWindowPosition({
          left: start.left + latestX - startX,
          top: start.top + latestY - startY,
          width: start.width,
          height: start.height,
        }, vp);
        el.style.left = `${position.left}px`;
        el.style.top = `${position.top}px`;
        activeLayoutCandidate = resolveLayoutForWindow(win, event.pointerType, activeLayoutCandidate);
        showResolvedLayoutPreview(activeLayoutCandidate);
        updateDynamicShadow(el);
      };
      const move = (moveEvent) => {
        if (!active || moveEvent.pointerId !== event.pointerId) return;
        latestX = moveEvent.clientX;
        latestY = moveEvent.clientY;
        if (!frame) frame = requestAnimationFrame(apply);
      };
      const finish = (finishEvent, { cancelled = false } = {}) => {
        if (!active || finishEvent.pointerId !== event.pointerId) return;
        if (frame) cancelAnimationFrame(frame);
        frame = 0;
        apply();
        active = false;
        try {
          if (handle.hasPointerCapture?.(event.pointerId)) handle.releasePointerCapture?.(event.pointerId);
        } catch {}
        handle.removeEventListener('pointermove', move);
        handle.removeEventListener('pointerup', up);
        handle.removeEventListener('pointercancel', cancel);
        handle.removeEventListener('lostpointercapture', lost);
        if (cancelled) clearResolvedLayoutPreview();
        else commitResolvedLayout(win, activeLayoutCandidate);
        activeLayoutCandidate = null;
        bus.emit('window:moved', {
          id: win.id,
          ownerId: win.ownerId,
          top: el.style.top,
          left: el.style.left,
          width: el.style.width,
          height: el.style.height,
        });
        persistFor(win);
        reflowDockedDependents(win);
      };
      const up = (upEvent) => finish(upEvent);
      const cancel = (cancelEvent) => finish(cancelEvent, { cancelled: true });
      const lost = (lostEvent) => finish(lostEvent, { cancelled: true });
      handle.addEventListener('pointermove', move);
      handle.addEventListener('pointerup', up);
      handle.addEventListener('pointercancel', cancel);
      handle.addEventListener('lostpointercapture', lost);
    };
    for (const handle of dragHandles) {
      handle.addEventListener('pointerdown', (event) => beginDrag(event, handle));
    }
    win.element.addEventListener('pointerdown', (event) => {
      const moduleHeader = event.target.closest?.('[data-shell-v2-header-row="1"]');
      if (moduleHeader && win.element.contains(moduleHeader)) {
        if (event.target.closest?.(interactiveSelector)) return;
        beginDrag(event, win.element);
        return;
      }
      beginDrag(event, win.element, true);
    });
  }

  function makePointerResizable(win, handle, direction) {
    handle.addEventListener('pointerdown', (event) => {
      if (event.pointerType === 'mouse' && event.button !== 0) return;
      if (win.element.classList.contains('is-mobile-sheet') || win.state === 'maximized') return;
      event.preventDefault();
      event.stopPropagation();
      focus(win.id);
      clearInsetRestore(win);
      clearDockRelation(win);
      win.element.classList.remove('is-snapped', 'is-docked-to-app');
      win.element.removeAttribute('data-snap-zone');
      const start = windowRectFor(win);
      const startX = event.clientX;
      const startY = event.clientY;
      const vp = getNormalViewport();
      let latestX = startX;
      let latestY = startY;
      let frame = 0;
      let active = true;
      try { handle.setPointerCapture?.(event.pointerId); } catch {}

      const apply = () => {
        frame = 0;
        if (!active) return;
        const dx = latestX - startX;
        const dy = latestY - startY;
        const right = start.right;
        const bottom = start.bottom;
        let left = direction.includes('w') ? Math.min(start.left + dx, right - win.minWidth) : start.left;
        let top = direction.includes('n') ? Math.min(start.top + dy, bottom - win.minHeight) : start.top;
        left = Math.max(vp.left, left);
        top = Math.max(vp.top, top);
        let width = direction.includes('e') ? Math.max(win.minWidth, start.width + dx) : right - left;
        let height = direction.includes('s') ? Math.max(win.minHeight, start.height + dy) : bottom - top;
        width = Math.min(width, vp.w - vp.right - left);
        height = Math.min(height, vp.h - vp.bottom - top);
        Object.assign(win.element.style, {
          left: `${left}px`, top: `${top}px`, width: `${width}px`, height: `${height}px`,
        });
      };
      const move = (moveEvent) => {
        if (!active || moveEvent.pointerId !== event.pointerId) return;
        latestX = moveEvent.clientX;
        latestY = moveEvent.clientY;
        if (!frame) frame = requestAnimationFrame(apply);
      };
      const finish = (finishEvent) => {
        if (!active || finishEvent.pointerId !== event.pointerId) return;
        if (frame) cancelAnimationFrame(frame);
        frame = 0;
        apply();
        active = false;
        try {
          if (handle.hasPointerCapture?.(event.pointerId)) handle.releasePointerCapture?.(event.pointerId);
        } catch {}
        handle.removeEventListener('pointermove', move);
        handle.removeEventListener('pointerup', finish);
        handle.removeEventListener('pointercancel', finish);
        handle.removeEventListener('lostpointercapture', finish);
        bus.emit('window:resized', {
          id: win.id,
          ownerId: win.ownerId,
          width: win.element.style.width,
          height: win.element.style.height,
          top: win.element.style.top,
          left: win.element.style.left,
        });
        persistFor(win);
        reflowDockedDependents(win);
      };
      handle.addEventListener('pointermove', move);
      handle.addEventListener('pointerup', finish);
      handle.addEventListener('pointercancel', finish);
      handle.addEventListener('lostpointercapture', finish);
    });
  }

  function bindShellV2KeyboardGeometry(win) {
    const dragHandle = win.element.querySelector('[data-window-drag-region]');
    dragHandle?.setAttribute('aria-keyshortcuts', 'ArrowUp ArrowDown ArrowLeft ArrowRight');
    dragHandle?.addEventListener('keydown', (event) => {
      if (!['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(event.key)) return;
      event.preventDefault();
      const step = event.shiftKey ? 1 : 16;
      const current = windowRectFor(win);
      const vp = getNormalViewport();
      const requested = {
        left: current.left + (event.key === 'ArrowLeft' ? -step : event.key === 'ArrowRight' ? step : 0),
        top: current.top + (event.key === 'ArrowUp' ? -step : event.key === 'ArrowDown' ? step : 0),
        width: current.width,
        height: current.height,
      };
      const position = clampNormalWindowPosition(requested, vp);
      clearDockRelation(win);
      win.element.classList.remove('is-snapped');
      win.element.removeAttribute('data-snap-zone');
      win.element.style.left = `${position.left}px`;
      win.element.style.top = `${position.top}px`;
      bus.emit('window:moved', { id: win.id, ownerId: win.ownerId, ...position });
      persistFor(win);
      reflowDockedDependents(win);
    });

    for (const handle of win.element.querySelectorAll('[data-window-resize]')) {
      handle.addEventListener('keydown', (event) => {
        if (!['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(event.key)) return;
        event.preventDefault();
        const direction = handle.dataset.windowResize || '';
        const step = event.shiftKey ? 1 : 16;
        const dx = event.key === 'ArrowLeft' ? -step : event.key === 'ArrowRight' ? step : 0;
        const dy = event.key === 'ArrowUp' ? -step : event.key === 'ArrowDown' ? step : 0;
        const current = windowRectFor(win);
        const vp = getNormalViewport();
        let left = current.left;
        let top = current.top;
        let width = current.width;
        let height = current.height;
        if (direction.includes('w') && dx) {
          const nextLeft = Math.max(vp.left, Math.min(current.right - win.minWidth, current.left + dx));
          left = nextLeft;
          width = current.right - nextLeft;
        } else if (direction.includes('e') && dx) {
          width = Math.max(win.minWidth, Math.min(vp.w - vp.right - current.left, current.width + dx));
        }
        if (direction.includes('n') && dy) {
          const nextTop = Math.max(vp.top, Math.min(current.bottom - win.minHeight, current.top + dy));
          top = nextTop;
          height = current.bottom - nextTop;
        } else if (direction.includes('s') && dy) {
          height = Math.max(win.minHeight, Math.min(vp.h - vp.bottom - current.top, current.height + dy));
        }
        clearDockRelation(win);
        win.element.classList.remove('is-snapped');
        win.element.removeAttribute('data-snap-zone');
        Object.assign(win.element.style, {
          left: `${left}px`, top: `${top}px`, width: `${width}px`, height: `${height}px`,
        });
        bus.emit('window:resized', { id: win.id, ownerId: win.ownerId, left, top, width, height });
        persistFor(win);
        reflowDockedDependents(win);
      });
    }
  }

  function windowRectFor(win) {
    const el = win.element;
    const left = el.offsetLeft;
    const top = el.offsetTop;
    const width = el.offsetWidth;
    const height = el.offsetHeight;
    return { left, top, right: left + width, bottom: top + height, width, height };
  }

  function resolverWorkRect() {
    const vp = getNormalViewport();
    return {
      left: vp.left,
      top: vp.top,
      width: Math.max(0, vp.w - vp.left - vp.right),
      height: Math.max(0, vp.h - vp.top - vp.bottom),
    };
  }

  function resolveLayoutForWindow(win, pointerType, previousCandidate) {
    const targetRects = win.ownerId === 'desktop-app:knowledge'
      ? windows
          .filter((target) => target.id !== win.id && target.ownerId === 'desktop-app:tickets' && target.state !== 'minimized')
          .map((target) => ({ id: target.ownerId, rect: windowRectFor(target) }))
      : [];
    return resolveWindowLayout({
      sourceRect: windowRectFor(win),
      workRect: resolverWorkRect(),
      targetRects,
      pointerType,
      previousCandidate,
    });
  }

  function showResolvedLayoutPreview(candidate) {
    if (!snapPreviewEl) return;
    if (!candidate?.rect) {
      clearResolvedLayoutPreview();
      return;
    }
    snapPreviewEl.dataset.snap = candidate.id;
    Object.assign(snapPreviewEl.style, {
      left: `${candidate.rect.left}px`,
      top: `${candidate.rect.top}px`,
      width: `${candidate.rect.width}px`,
      height: `${candidate.rect.height}px`,
    });
    snapPreviewEl.hidden = false;
    requestAnimationFrame(() => snapPreviewEl.classList.add('is-visible'));
  }

  function clearResolvedLayoutPreview() {
    if (!snapPreviewEl) return;
    snapPreviewEl.classList.remove('is-visible');
    snapPreviewEl.hidden = true;
    snapPreviewEl.removeAttribute('data-snap');
  }

  function commitResolvedLayout(win, candidate) {
    clearResolvedLayoutPreview();
    if (!candidate?.rect) return;
    if (!win.element.classList.contains('is-snapped') && !win.dockRelation) {
      win.stored = {
        width: win.element.style.width,
        height: win.element.style.height,
        top: win.element.style.top,
        left: win.element.style.left,
      };
    }
    Object.assign(win.element.style, {
      left: `${candidate.rect.left}px`,
      top: `${candidate.rect.top}px`,
      width: `${candidate.rect.width}px`,
      height: `${candidate.rect.height}px`,
    });
    if (candidate.kind === 'workspace') {
      clearDockRelation(win);
      win.element.classList.add('is-snapped');
      win.element.classList.remove('is-docked-to-app');
      win.element.dataset.snapZone = candidate.zone;
      bus.emit('window:snapped', { id: win.id, ownerId: win.ownerId, zone: candidate.zone });
      return;
    }
    win.element.classList.remove('is-snapped');
    win.element.classList.add('is-docked-to-app');
    win.element.removeAttribute('data-snap-zone');
    win.dockRelation = {
      targetOwnerId: candidate.targetId,
      sourceEdge: candidate.sourceEdge,
      targetEdge: candidate.targetEdge,
    };
    bus.emit('window:docked', { id: win.id, ownerId: win.ownerId, ...win.dockRelation });
  }

  function clearDockRelation(win) {
    win.dockRelation = null;
    win.element?.classList?.remove('is-docked-to-app');
  }

  function restoreAppDock(win, { failClosed = true } = {}) {
    const relation = win.dockRelation;
    const target = windows.find((entry) => entry.ownerId === relation?.targetOwnerId && entry.id !== win.id);
    if (!target) {
      win.element?.classList?.remove('is-docked-to-app');
      if (failClosed) {
        clearDockRelation(win);
        persistFor(win);
      }
      return false;
    }
    const source = windowRectFor(win);
    const targetRect = windowRectFor(target);
    const rect = { left: source.left, top: source.top, width: source.width, height: source.height };
    if (relation.sourceEdge === 'right' && relation.targetEdge === 'left') rect.left = targetRect.left - source.width;
    else if (relation.sourceEdge === 'left' && relation.targetEdge === 'right') rect.left = targetRect.right;
    else if (relation.sourceEdge === 'bottom' && relation.targetEdge === 'top') rect.top = targetRect.top - source.height;
    else if (relation.sourceEdge === 'top' && relation.targetEdge === 'bottom') rect.top = targetRect.bottom;
    else {
      clearDockRelation(win);
      persistFor(win);
      return false;
    }
    commitResolvedLayout(win, { kind: 'app', rect, targetId: relation.targetOwnerId, ...relation });
    return true;
  }

  function finalizeDockRestore() {
    for (const win of windows) {
      if (!win.dockRelation) continue;
      restoreAppDock(win, { failClosed: true });
    }
  }

  function reflowDockedDependents(target) {
    if (!target?.ownerId) return;
    for (const dependent of windows) {
      if (dependent.id === target.id || dependent.dockRelation?.targetOwnerId !== target.ownerId) continue;
      restoreAppDock(dependent);
      persistFor(dependent);
    }
  }

  function applySnapPreview(clientX, clientY, { dragStartX = clientX, dragStartY = clientY } = {}) {
    if (!snapPreviewEl) return;
    const layerRect = windowLayer.getBoundingClientRect();
    const vp = getViewport();
    const x = clientX - layerRect.left;
    const y = clientY - layerRect.top;
    // A pointer past an inset boundary (chat dock, menubar, taskbar) means
    // "dock at that edge", not "abort" — clamp into the usable area instead of
    // cancelling. Zone selection depends only on the pointer's edge bands;
    // drag direction must not suppress left or right docking.
    const zone = detectSnapZone(x, y, vp);

    if (!zone) {
      snapPreviewEl.removeAttribute('data-snap');
      snapPreviewEl.classList.remove('is-visible');
      snapPreviewEl.hidden = true;
      return;
    }
    snapPreviewEl.dataset.snap = zone;
    Object.assign(snapPreviewEl.style, snapTargetStyles(zone));
    snapPreviewEl.hidden = false;
    requestAnimationFrame(() => snapPreviewEl.classList.add('is-visible'));
  }

  function commitSnap(win) {
    if (!snapPreviewEl || snapPreviewEl.hidden || !snapPreviewEl.dataset.snap) {
      snapPreviewEl?.classList.remove('is-visible');
      if (snapPreviewEl) snapPreviewEl.hidden = true;
      return;
    }
    const zone = snapPreviewEl.dataset.snap;
    snapPreviewEl.classList.remove('is-visible');
    snapPreviewEl.hidden = true;
    if (zone) snapTo(win.id, zone);
  }

  // The ctox chrome pins window elevation to var(--win-elev*) !important, so
  // the per-frame shadow vars are dead work there; the base windows/macos
  // chrome still consumes --win-shadow-y/--win-shadow-blur. makeDraggable
  // caches this check at drag start so the hot path never re-reads it.
  function dynamicShadowActive() {
    return document.documentElement.dataset.shellStyle !== 'ctox';
  }

  function updateDynamicShadow(el) {
    if (!dynamicShadowActive()) return;
    const surfaceRect = surfaceEl.getBoundingClientRect();
    const rect = el.getBoundingClientRect();
    const centerY = rect.top + rect.height / 2;
    const offY = (centerY - (surfaceRect.top + surfaceRect.height / 2)) / 18 + 8;
    const blur = 28 + Math.abs(offY) / 2;
    el.style.setProperty('--win-shadow-y', `${offY.toFixed(1)}px`);
    el.style.setProperty('--win-shadow-blur', `${blur.toFixed(1)}px`);
  }

  function persistFor(win) {
    if (!persistence?.save || !win.ownerId) return;
    try {
      persistence.save(win.ownerId, snapshotFor(win));
    } catch (error) {
      console.error('[windowManager] persistence.save failed:', error);
    }
  }

  function snapshotFor(win) {
    const el = win.element;
    return {
      ownerId: win.ownerId,
      shellContract: win.shellContract,
      shellGeometryContract: win.shellGeometryContract,
      title: el.querySelector('[data-window-title]')?.textContent || '',
      icon: win.icon || '',
      x: parsePx(el.style.left),
      y: parsePx(el.style.top),
      width: parsePx(el.style.width),
      height: parsePx(el.style.height),
      state: win.state,
      snapZone: el.dataset.snapZone || '',
      dockRelation: win.dockRelation
        ? {
            targetOwnerId: win.dockRelation.targetOwnerId,
            sourceEdge: win.dockRelation.sourceEdge,
            targetEdge: win.dockRelation.targetEdge,
          }
        : null,
      alwaysOnTop: !!win.alwaysOnTop,
      stored: win.stored
        ? {
            width: win.stored.width || '',
            height: win.stored.height || '',
            top: win.stored.top || '',
            left: win.stored.left || '',
          }
        : null,
    };
  }

  return {
    create,
    focus,
    minimize,
    minimizeAll,
    toggleMaximize,
    restore: (id) => {
      const win = windows.find((w) => w.id === id);
      if (!win) return;
      if (win.state === 'minimized') focus(id);
      else if (win.state === 'maximized') toggleMaximize(id);
    },
    destroy,
    registerCloseGuard,
    destroyAll,
    closeOthersOfOwner,
    listWindows,
    describe,
    setChromeLayout,
    setInsets,
    setAlwaysOnTop,
    setAppMode,
    refreshV2Chrome,
    finalizeDockRestore,
    snapTo,
    getViewport,
    getMinimumWorkArea,
  };
}

function assertShellWindowChrome(winEl, shellContract = 'v1') {
  const dragRegion = winEl?.querySelector('[data-window-drag-region]');
  const controls = winEl?.querySelectorAll('[data-window-control]') || [];
  const actions = new Set(Array.from(controls).map((control) => control.dataset.windowControl));
  const expected = shellContract === 'v2' ? ['close'] : SHELL_WINDOW_CONTROL_ACTIONS;
  const complete = controls.length === expected.length
    && expected.every((action) => actions.has(action));
  const operable = Array.from(controls).every((control) => (
    control.tagName === 'BUTTON'
    && control.type === 'button'
    && String(control.getAttribute('aria-label') || '').trim().length > 0
  ));
  if (!dragRegion || !winEl?.querySelector('[data-window-control-strip]') || !complete || !operable) {
    throw new Error(`windowManager: ${shellContract} shell chrome is incomplete`);
  }
}

function renderControls(controlsEl, layout, translate, shellContract = 'v1') {
  if (!controlsEl) return;
  controlsEl.innerHTML = '';
  const kinds = shellContract === 'v2'
    ? ['close']
    : (CONTROL_KINDS_BY_STYLE[layout] || CONTROL_KINDS_BY_STYLE.windows);
  for (const kind of kinds) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.dataset.action = kind;
    btn.dataset.windowControl = kind;
    btn.className = `shell-window-control shell-window-control--${kind}`;
    const labelKey = `window${kind[0].toUpperCase()}${kind.slice(1)}`;
    btn.setAttribute('aria-label', translate(labelKey, kind));
    btn.textContent = CONTROL_GLYPHS[kind] || '';
    controlsEl.appendChild(btn);
  }
}

function updateMaximizeControl(win, translate) {
  if (!win?.element) return;
  const maximized = win.state === 'maximized';
  win.element.classList.toggle('is-maximized', maximized);
  const button = win.element.querySelector('[data-window-controls] [data-window-control="maximize"]');
  if (!button) return;
  button.textContent = maximized ? CONTROL_GLYPHS.restore : CONTROL_GLYPHS.maximize;
  button.setAttribute(
    'aria-label',
    maximized
      ? translate('windowRestore', 'restore')
      : translate('windowMaximize', 'maximize'),
  );
}

function renderHeaderItems(container, items, kind) {
  if (!container) return;
  container.replaceChildren();
  for (const item of Array.isArray(items) ? items : []) {
    if (!item || item.hidden === true) continue;
    const actionable = Boolean(item.id);
    const node = document.createElement(actionable ? 'button' : 'span');
    if (actionable) {
      node.type = 'button';
      node.dataset.windowHeaderAction = String(item.id);
    }
    node.className = `shell-window-header-${kind}`;
    if (item.state) node.dataset.state = String(item.state);
    if (item.title) node.title = String(item.title);
    if (item.ariaLabel || item.label) node.setAttribute('aria-label', String(item.ariaLabel || item.label));
    if (item.icon) {
      const icon = document.createElement('span');
      icon.className = 'shell-window-header-item-icon';
      icon.setAttribute('aria-hidden', 'true');
      icon.textContent = String(item.icon);
      node.appendChild(icon);
    }
    if (item.label) {
      const label = document.createElement('span');
      label.className = 'shell-window-header-item-label';
      label.textContent = String(item.label);
      node.appendChild(label);
    }
    container.appendChild(node);
  }
  container.hidden = container.childElementCount === 0;
}

function composeTitle(options, translate) {
  const icon = options.icon ? `${options.icon} ` : '';
  const title = options.title || translate('defaultWindowTitle', 'Fenster');
  return `${icon}${title}`;
}

function escapeAttribute(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function applyFramePalette(element, palette = {}) {
  const tokens = {
    '--shell-v2-frame-start': palette?.start,
    '--shell-v2-frame-middle': palette?.middle,
    '--shell-v2-frame-top-joint': palette?.top_joint ?? palette?.topJoint ?? palette?.middle,
    '--shell-v2-frame-left-joint': palette?.left_joint ?? palette?.leftJoint ?? palette?.middle,
    '--shell-v2-frame-end': palette?.end,
    '--shell-v2-surface': palette?.surface,
    '--shell-v2-surface-alt': palette?.surface_alt ?? palette?.surfaceAlt,
    '--shell-v2-accent': palette?.accent,
    '--shell-v2-content-accent': palette?.content_accent ?? palette?.contentAccent ?? palette?.accent,
    '--shell-v2-content-foreground': palette?.content_foreground ?? palette?.contentForeground,
  };
  for (const [token, value] of Object.entries(tokens)) {
    if (typeof value === 'string' && /^#[0-9a-f]{6}$/i.test(value)) {
      element.style.setProperty(token, value);
    }
  }
}

function framePaletteFromIcon(image) {
  try {
    if (!image?.naturalWidth || !image?.naturalHeight) return null;
    const canvas = document.createElement('canvas');
    canvas.width = 48;
    canvas.height = 48;
    const context = canvas.getContext('2d', { willReadFrequently: true });
    if (!context) return null;
    context.drawImage(image, 0, 0, canvas.width, canvas.height);
    return shellV2FramePaletteFromRgba(
      context.getImageData(0, 0, canvas.width, canvas.height).data,
      canvas.width,
      canvas.height,
    );
  } catch {
    // Cross-origin/runtime-installed artwork may be unreadable. The manifest
    // palette already applied above remains the explicit fail-closed fallback.
    return null;
  }
}

function parsePx(value) {
  if (typeof value !== 'string') return null;
  const m = value.match(/^(-?\d+(?:\.\d+)?)px$/);
  if (!m) return null;
  const n = Number(m[1]);
  return Number.isFinite(n) ? n : null;
}

function geometryStyles(element) {
  return {
    width: element?.style?.width || '',
    height: element?.style?.height || '',
    top: element?.style?.top || '',
    left: element?.style?.left || '',
  };
}

function secureToken() {
  if (typeof crypto !== 'undefined' && crypto.getRandomValues) {
    const buf = new Uint32Array(2);
    crypto.getRandomValues(buf);
    return `${buf[0].toString(36)}${buf[1].toString(36)}`;
  }
  return `${Date.now().toString(36)}_${Math.random().toString(36).slice(2)}`;
}

function stubBus() {
  return { emit: () => {}, on: () => ({ id: '' }), off: () => {} };
}
