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

const CONTROL_KINDS_BY_STYLE = {
  windows: ['minimize', 'maximize', 'close'],
  macos: ['close', 'minimize', 'maximize'],
};

const CONTROL_GLYPHS = {
  minimize: '−',
  maximize: '□',
  restore: '❐',
  close: '×',
};

const RESIZE_HANDLES = ['n', 's', 'e', 'w', 'nw', 'ne', 'sw', 'se'];

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
      renderControls(win.element.querySelector('.shell-window-controls'), chromeLayout, translate);
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
    winEl.className = 'shell-window';
    winEl.id = id;
    if (ownerId) winEl.dataset.ownerId = ownerId;
    winEl.style.transition = 'none';

    const persisted = ownerId && persistence?.load ? persistence.load(ownerId) : null;
    const restored = persisted && (persisted.width || persisted.height || persisted.x != null || persisted.y != null);

    const maxInitialWidth = Math.max(minWidth, vp.w - vp.left - vp.right);
    const maxInitialHeight = Math.max(minHeight, vp.h - vp.top - vp.bottom);
    const width = Math.min(maxInitialWidth, Math.max(minWidth, parseInt(persisted?.width ?? options.width, 10) || 520));
    const height = Math.min(maxInitialHeight, Math.max(minHeight, parseInt(persisted?.height ?? options.height, 10) || 360));
    winEl.style.width = `${width}px`;
    winEl.style.height = `${height}px`;

    const cascadeOffset = (windows.length * CONST.CASCADE_STEP) % Math.max(80, Math.floor(vp.h / 3));
    let baseX = parseInt(persisted?.x ?? options.x ?? 80 + cascadeOffset, 10);
    let baseY = parseInt(persisted?.y ?? options.y ?? 60 + cascadeOffset, 10);
    const maxX = Math.max(vp.left, vp.w - vp.right - 100);
    const maxY = Math.max(vp.top, vp.h - vp.bottom - 100);
    if (!Number.isFinite(baseX) || baseX < vp.left || baseX > maxX) baseX = Math.max(vp.left, 24);
    if (!Number.isFinite(baseY) || baseY < vp.top || baseY > maxY) baseY = Math.max(vp.top, 24);
    winEl.style.left = `${baseX}px`;
    winEl.style.top = `${baseY}px`;

    winEl.innerHTML = `
      <header class="shell-window-header" data-window-header>
        <div class="shell-window-title" data-window-title></div>
        <div class="shell-window-meta" data-window-meta></div>
        <div class="shell-window-actions" data-window-actions></div>
        <div class="shell-window-controls" data-window-controls></div>
      </header>
      <div class="shell-window-content" data-window-content></div>
      ${RESIZE_HANDLES.map((dir) => `<div class="shell-window-resize shell-window-resize--${dir}" data-window-resize="${dir}"></div>`).join('')}
    `;

    const titleEl = winEl.querySelector('[data-window-title]');
    const winIconKey = ownerId ? ownerId.replace(/^(desktop-app|module):/, '') : '';
    const svgHtml = svgIconFor(winIconKey, 14, 1.8);
    const escapeHtml = (str) => String(str).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
    titleEl.innerHTML = `${svgHtml ? `<span class="shell-window-title-icon" aria-hidden="true">${svgHtml}</span>` : ''}<span class="shell-window-title-text">${escapeHtml(options.title || composeTitle(options, translate))}</span>`;
    renderHeaderItems(winEl.querySelector('[data-window-meta]'), options.headerBadges, 'meta');
    renderHeaderItems(winEl.querySelector('[data-window-actions]'), options.headerActions, 'action');
    const controlsEl = winEl.querySelector('[data-window-controls]');
    renderControls(controlsEl, chromeLayout, translate);

    setTimeout(() => { winEl.style.transition = ''; }, 50);
    windowLayer.appendChild(winEl);

    const win = {
      id,
      ownerId,
      icon: options.icon || '',
      element: winEl,
      state: 'normal',
      minWidth,
      minHeight,
      stored: persisted?.stored
        ? { ...persisted.stored }
        : null,
      alwaysOnTop: !!persisted?.alwaysOnTop,
      appMode: 'window',
      _destroying: false,
      _restored: restored,
      _onHostFileDrop: typeof options.onHostFileDrop === 'function' ? options.onHostFileDrop : null,
      _onHeaderAction: typeof options.onHeaderAction === 'function' ? options.onHeaderAction : null,
    };
    windows.push(win);
    constrainNormalWindow(win);

    makeDraggable(win);
    for (const dir of RESIZE_HANDLES) {
      makeResizable(win, dir);
    }
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
    }

    // Window-open animation: plays once from the final geometry (after any
    // maximized/snap restore above) and is purely visual — it never drives
    // geometry persistence; the 50 ms transition suppression at create time
    // still guards restored geometry from animating. Skipped for reduced
    // motion and for windows restored into maximized/snapped state, where a
    // scale-in reads as a glitch on an edge-docked surface.
    if (!prefersReducedMotion() && win.state !== 'maximized' && !winEl.classList.contains('is-snapped')) {
      winEl.classList.add('is-opening');
      const clearOpening = () => winEl.classList.remove('is-opening');
      winEl.addEventListener('animationend', clearOpening, { once: true });
      setTimeout(clearOpening, motionBaseMs() + 60);
    }

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
    if (!win) return;
    const reduced = prefersReducedMotion();
    if (!reduced) win.element.classList.add('is-minimizing');
    setTimeout(() => {
      win.element.classList.remove('is-minimizing');
      win.element.style.display = 'none';
      win.state = 'minimized';
      focusNextAfter(id);
      bus.emit('window:minimized', { id, ownerId: win.ownerId });
      persistFor(win);
    }, reduced ? 0 : motionBaseMs());
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
    applyMaximizedBounds(win);
    win.element.classList.remove('is-snapped');
    win.element.removeAttribute('data-snap-zone');
    win.state = 'maximized';
    updateMaximizeControl(win, translate);
    bus.emit('window:maximized', { id, ownerId: win.ownerId });
    persistFor(win);
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
    applySnapBounds(win, zone);
    win.element.classList.add('is-snapped');
    win.element.dataset.snapZone = zone;
    win.state = 'normal';
    bus.emit('window:snapped', { id, ownerId: win.ownerId, zone });
    persistFor(win);
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

  function destroy(id) {
    const win = windows.find((w) => w.id === id);
    if (!win || win._destroying) return;
    win._destroying = true;
    clearTimeout(win._layoutSwitchTimer);
    const reduced = prefersReducedMotion();
    if (!reduced) win.element.classList.add('is-closing');
    const stackIndex = stack.indexOf(id);
    if (stackIndex !== -1) stack.splice(stackIndex, 1);
    focusNextAfter(id);
    setTimeout(() => {
      win.element.remove();
      const idx = windows.findIndex((w) => w.id === id);
      if (idx !== -1) windows.splice(idx, 1);
      bus.emit('window:closed', { id, ownerId: win.ownerId });
    }, reduced ? 0 : motionBaseMs());
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
      const btn = event.target.closest('[data-action]');
      if (!btn) return;
      event.stopPropagation();
      const action = btn.dataset.action;
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
    const header = win.element.querySelector('[data-window-header]');
    if (!header) return;
    header.addEventListener('mousedown', (downEvent) => {
      if (downEvent.button !== 0) return;
      if (win.element.classList.contains('is-mobile-sheet')) return;
      if (downEvent.target.closest('[data-window-controls], [data-window-header-action]')) return;
      clearInsetRestore(win);
      const el = win.element;
      let initialX = downEvent.clientX;
      let initialY = downEvent.clientY;
      const dragStartX = downEvent.clientX;
      const dragStartY = downEvent.clientY;
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
        applySnapPreview(currentX, currentY, { dragStartX, dragStartY });
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
      }

      document.addEventListener('mousemove', onMouseMove);
      document.addEventListener('mouseup', onMouseUp);
    });
  }

  function makeResizable(win, direction) {
    const handle = win.element.querySelector(`[data-window-resize="${direction}"]`);
    if (!handle) return;
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
      }

      document.addEventListener('mousemove', onMouseMove);
      document.addEventListener('mouseup', onMouseUp);
    });
  }

  function applySnapPreview(clientX, clientY, { dragStartX = clientX, dragStartY = clientY } = {}) {
    if (!snapPreviewEl) return;
    const layerRect = windowLayer.getBoundingClientRect();
    const vp = getViewport();
    const x = clientX - layerRect.left;
    const y = clientY - layerRect.top;
    const leftEdge = vp.left;
    const rightEdge = vp.w - vp.right;
    const topEdge = vp.top;
    const bottomEdge = vp.h - vp.bottom;
    const edge = CONST.SNAP_EDGE;
    const corner = CONST.SNAP_CORNER;
    const horizontalDrag = Math.abs(clientX - dragStartX) > Math.abs(clientY - dragStartY) + 12;
    let zone = null;
    // A pointer past an inset boundary (chat dock, menubar, taskbar) means
    // "dock at that edge", not "abort" — clamp into the usable area instead of
    // cancelling. Cancelling here is why left/right docking never triggered
    // whenever a side inset was present: the natural drag to the physical
    // screen edge overshoots the inner edge band and killed the preview.
    const xc = Math.min(Math.max(x, leftEdge), rightEdge);
    const yc = Math.min(Math.max(y, topEdge), bottomEdge);
    if (horizontalDrag && xc < leftEdge + edge) zone = 'left';
    else if (horizontalDrag && xc > rightEdge - edge) zone = 'right';
    else if (yc < topEdge + corner && xc < leftEdge + corner) zone = 'top-left';
    else if (yc < topEdge + corner && xc > rightEdge - corner) zone = 'top-right';
    else if (yc > bottomEdge - corner && xc < leftEdge + corner) zone = 'bottom-left';
    else if (yc > bottomEdge - corner && xc > rightEdge - corner) zone = 'bottom-right';
    else if (yc < topEdge + edge) zone = 'top';
    else if (yc > bottomEdge - edge) zone = 'bottom';
    else if (xc < leftEdge + edge) zone = 'left';
    else if (xc > rightEdge - edge) zone = 'right';

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
      title: el.querySelector('[data-window-title]')?.textContent || '',
      icon: win.icon || '',
      x: parsePx(el.style.left),
      y: parsePx(el.style.top),
      width: parsePx(el.style.width),
      height: parsePx(el.style.height),
      state: win.state,
      snapZone: el.dataset.snapZone || '',
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
    toggleMaximize,
    restore: (id) => {
      const win = windows.find((w) => w.id === id);
      if (!win) return;
      if (win.state === 'minimized') focus(id);
      else if (win.state === 'maximized') toggleMaximize(id);
    },
    destroy,
    destroyAll,
    closeOthersOfOwner,
    listWindows,
    describe,
    setChromeLayout,
    setInsets,
    setAlwaysOnTop,
    setAppMode,
    snapTo,
    getViewport,
    getMinimumWorkArea,
  };
}

function renderControls(controlsEl, layout, translate) {
  if (!controlsEl) return;
  controlsEl.innerHTML = '';
  const kinds = CONTROL_KINDS_BY_STYLE[layout] || CONTROL_KINDS_BY_STYLE.windows;
  for (const kind of kinds) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.dataset.action = kind;
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
  const button = win.element.querySelector('[data-window-controls] [data-action="maximize"]');
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
