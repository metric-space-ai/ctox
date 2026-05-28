const CONST = {
  CASCADE_STEP: 22,
  SNAP_EDGE: 30,
  SNAP_CORNER: 60,
  MIN_WIDTH: 320,
  MIN_HEIGHT: 200,
  ALWAYS_ON_TOP_Z: 9000,
};

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
  let insets = { top: 0, bottom: 0 };

  function setChromeLayout(layout) {
    const next = layout === 'macos' ? 'macos' : 'windows';
    if (next === chromeLayout) return;
    chromeLayout = next;
    for (const win of windows) {
      renderControls(win.element.querySelector('.shell-window-controls'), chromeLayout, translate);
    }
  }

  function setInsets(next) {
    insets = {
      top: Math.max(0, next?.top ?? 0),
      bottom: Math.max(0, next?.bottom ?? 0),
    };
  }

  function getViewport() {
    const rect = surfaceEl.getBoundingClientRect();
    return { w: rect.width, h: rect.height, top: insets.top, bottom: insets.bottom };
  }

  function create(options = {}, legacyOwnerId) {
    const id = `desk_win_${secureToken()}`;
    const ownerId = options.ownerId || legacyOwnerId || null;
    const vp = getViewport();

    const winEl = document.createElement('section');
    winEl.className = 'shell-window';
    winEl.id = id;
    winEl.style.transition = 'none';

    const persisted = ownerId && persistence?.load ? persistence.load(ownerId) : null;
    const restored = persisted && (persisted.width || persisted.height || persisted.x != null || persisted.y != null);

    const maxInitialWidth = Math.max(CONST.MIN_WIDTH, vp.w);
    const maxInitialHeight = Math.max(CONST.MIN_HEIGHT, vp.h - vp.top - vp.bottom);
    const width = Math.min(maxInitialWidth, Math.max(CONST.MIN_WIDTH, parseInt(persisted?.width ?? options.width, 10) || 520));
    const height = Math.min(maxInitialHeight, Math.max(CONST.MIN_HEIGHT, parseInt(persisted?.height ?? options.height, 10) || 360));
    winEl.style.width = `${width}px`;
    winEl.style.height = `${height}px`;

    const cascadeOffset = (windows.length * CONST.CASCADE_STEP) % Math.max(80, Math.floor(vp.h / 3));
    let baseX = parseInt(persisted?.x ?? options.x ?? 80 + cascadeOffset, 10);
    let baseY = parseInt(persisted?.y ?? options.y ?? 60 + cascadeOffset, 10);
    const maxX = Math.max(0, vp.w - 100);
    const maxY = Math.max(vp.top, vp.h - vp.bottom - 100);
    if (!Number.isFinite(baseX) || baseX < 0 || baseX > maxX) baseX = 24;
    if (!Number.isFinite(baseY) || baseY < vp.top || baseY > maxY) baseY = Math.max(vp.top, 24);
    winEl.style.left = `${baseX}px`;
    winEl.style.top = `${baseY}px`;

    winEl.innerHTML = `
      <header class="shell-window-header" data-window-header>
        <div class="shell-window-title" data-window-title></div>
        <div class="shell-window-controls" data-window-controls></div>
      </header>
      <div class="shell-window-content" data-window-content></div>
      ${RESIZE_HANDLES.map((dir) => `<div class="shell-window-resize shell-window-resize--${dir}" data-window-resize="${dir}"></div>`).join('')}
    `;

    const titleEl = winEl.querySelector('[data-window-title]');
    const winIconKey = ownerId ? ownerId.replace(/^(desktop-app|module):/, '') : '';
    const svgHtml = svgIconFor(winIconKey, 14, 1.8);
    const escapeHtml = (str) => String(str).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
    if (svgHtml) {
      titleEl.innerHTML = `<span class="shell-window-title-icon" style="display:inline-flex; align-items:center; margin-right:6px; vertical-align:middle; opacity:0.85;">${svgHtml}</span><span class="shell-window-title-text" style="vertical-align:middle;">${escapeHtml(options.title || translate('defaultWindowTitle', 'Fenster'))}</span>`;
    } else {
      titleEl.textContent = composeTitle(options, translate);
    }
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
      stored: persisted?.stored
        ? { ...persisted.stored }
        : null,
      alwaysOnTop: !!persisted?.alwaysOnTop,
      _destroying: false,
      _restored: restored,
      _onHostFileDrop: typeof options.onHostFileDrop === 'function' ? options.onHostFileDrop : null,
    };
    windows.push(win);

    makeDraggable(win);
    for (const dir of RESIZE_HANDLES) {
      makeResizable(win, dir);
    }
    setupFocus(win);
    bindControls(win);
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
        const winIconKey = ownerId ? ownerId.replace(/^(desktop-app|module):/, '') : '';
        const svgHtml = svgIconFor(winIconKey, 14, 1.8);
        const escapeHtml = (str) => String(str).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
        if (svgHtml) {
          titleEl.innerHTML = `<span class="shell-window-title-icon" style="display:inline-flex; align-items:center; margin-right:6px; vertical-align:middle; opacity:0.85;">${svgHtml}</span><span class="shell-window-title-text" style="vertical-align:middle;">${escapeHtml(text)}</span>`;
        } else {
          titleEl.textContent = text;
        }
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
    if (win.state === 'minimized' || win.element.style.display === 'none') {
      win.element.style.display = '';
      win.element.style.transform = '';
      win.element.style.opacity = '';
      win.state = 'normal';
    }
    focusedId = id;
    const without = stack.filter((winId) => winId !== id);
    stack.length = 0;
    stack.push(...without, id);
    restackZ();
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
    win.element.style.transform = 'scale(0.8) translateY(20px)';
    win.element.style.opacity = '0';
    setTimeout(() => {
      win.element.style.display = 'none';
      win.state = 'minimized';
      focusNextAfter(id);
      bus.emit('window:minimized', { id, ownerId: win.ownerId });
      persistFor(win);
    }, 180);
  }

  function toggleMaximize(id, { skipStore = false } = {}) {
    const win = windows.find((w) => w.id === id);
    if (!win) return;
    if (win.state === 'maximized') {
      restoreSize(win);
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
    win.element.style.transition = 'all 200ms ease';
    win.element.style.top = `${insets.top}px`;
    win.element.style.left = '0';
    win.element.style.width = '100%';
    win.element.style.height = `calc(100% - ${insets.top + insets.bottom}px)`;
    win.element.classList.remove('is-snapped');
    win.element.removeAttribute('data-snap-zone');
    win.state = 'maximized';
    setTimeout(() => { win.element.style.transition = ''; }, 220);
    bus.emit('window:maximized', { id, ownerId: win.ownerId });
    persistFor(win);
  }

  function restoreSize(win) {
    if (!win.stored) {
      win.state = 'normal';
      win.element.classList.remove('is-snapped');
      return;
    }
    win.element.style.transition = 'all 200ms ease';
    win.element.style.width = win.stored.width || '520px';
    win.element.style.height = win.stored.height || '360px';
    win.element.style.top = win.stored.top || '60px';
    win.element.style.left = win.stored.left || '80px';
    win.element.classList.remove('is-snapped');
    win.element.removeAttribute('data-snap-zone');
    win.state = 'normal';
    setTimeout(() => { win.element.style.transition = ''; }, 220);
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
    const top = `${insets.top}px`;
    const usableHeight = `calc(100% - ${insets.top + insets.bottom}px)`;
    const halfHeight = `calc((100% - ${insets.top + insets.bottom}px) / 2)`;
    const targets = {
      left: { top, left: '0', width: '50%', height: usableHeight },
      right: { top, left: '50%', width: '50%', height: usableHeight },
      top: { top, left: '0', width: '100%', height: halfHeight },
      bottom: { top: `calc(${insets.top}px + ${halfHeight})`, left: '0', width: '100%', height: halfHeight },
      'top-left': { top, left: '0', width: '50%', height: halfHeight },
      'top-right': { top, left: '50%', width: '50%', height: halfHeight },
      'bottom-left': { top: `calc(${insets.top}px + ${halfHeight})`, left: '0', width: '50%', height: halfHeight },
      'bottom-right': { top: `calc(${insets.top}px + ${halfHeight})`, left: '50%', width: '50%', height: halfHeight },
    };
    win.element.style.transition = 'all 180ms ease';
    Object.assign(win.element.style, targets[zone]);
    win.element.classList.add('is-snapped');
    win.element.dataset.snapZone = zone;
    win.state = 'normal';
    setTimeout(() => { win.element.style.transition = ''; }, 200);
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

  function destroy(id) {
    const win = windows.find((w) => w.id === id);
    if (!win || win._destroying) return;
    win._destroying = true;
    win.element.classList.add('is-closing');
    const stackIndex = stack.indexOf(id);
    if (stackIndex !== -1) stack.splice(stackIndex, 1);
    focusNextAfter(id);
    setTimeout(() => {
      win.element.remove();
      const idx = windows.findIndex((w) => w.id === id);
      if (idx !== -1) windows.splice(idx, 1);
      bus.emit('window:closed', { id, ownerId: win.ownerId });
    }, 180);
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

  function bindHeaderGestures(win) {
    const header = win.element.querySelector('[data-window-header]');
    if (!header) return;
    header.addEventListener('dblclick', (event) => {
      if (event.target.closest('[data-window-controls]')) return;
      event.preventDefault();
      toggleMaximize(win.id);
    });
    header.addEventListener('contextmenu', (event) => {
      if (event.target.closest('[data-window-controls]')) return;
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
      if (downEvent.target.closest('[data-window-controls]')) return;
      const el = win.element;
      let initialX = downEvent.clientX;
      let initialY = downEvent.clientY;
      let currentX = initialX;
      let currentY = initialY;
      let dragging = true;
      let rAFQueued = false;

      function update() {
        const dx = initialX - currentX;
        const dy = initialY - currentY;
        initialX = currentX;
        initialY = currentY;
        const vp = getViewport();
        const top = Math.max(insets.top, Math.min(vp.h - insets.bottom - 40, el.offsetTop - dy));
        const left = Math.max(-el.offsetWidth + 80, Math.min(vp.w - 80, el.offsetLeft - dx));
        el.style.top = `${top}px`;
        el.style.left = `${left}px`;
        applySnapPreview(currentX, currentY);
        updateDynamicShadow(el);
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
          }
          const newWidth = el.offsetWidth;
          initialX = currentX;
          el.style.left = `${currentX - newWidth * ratio}px`;
        }
        if (!rAFQueued) {
          rAFQueued = true;
          requestAnimationFrame(() => { rAFQueued = false; update(); });
        }
      }

      function onMouseUp() {
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
      if (win.state === 'maximized') return;
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
      let rAFQueued = false;
      let resizing = true;
      const vp = getViewport();

      function onMouseMove(moveEvent) {
        if (!resizing) return;
        const dX = moveEvent.clientX - startX;
        const dY = moveEvent.clientY - startY;
        if (!rAFQueued) {
          rAFQueued = true;
          requestAnimationFrame(() => {
            rAFQueued = false;
            let newWidth = startWidth;
            let newHeight = startHeight;
            let newLeft = startLeft;
            let newTop = startTop;
            if (direction.includes('e')) newWidth = Math.max(CONST.MIN_WIDTH, startWidth + dX);
            if (direction.includes('w')) {
              const candidateWidth = Math.max(CONST.MIN_WIDTH, startWidth - dX);
              newLeft = startLeft + (startWidth - candidateWidth);
              newWidth = candidateWidth;
            }
            if (direction.includes('s')) newHeight = Math.max(CONST.MIN_HEIGHT, startHeight + dY);
            if (direction.includes('n')) {
              const candidateHeight = Math.max(CONST.MIN_HEIGHT, startHeight - dY);
              newTop = Math.max(insets.top, startTop + (startHeight - candidateHeight));
              newHeight = candidateHeight;
            }
            const maxHeightFromTop = Math.max(CONST.MIN_HEIGHT, vp.h - insets.bottom - newTop);
            const maxWidthFromLeft = Math.max(CONST.MIN_WIDTH, vp.w - newLeft);
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
          });
        }
      }

      function onMouseUp() {
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

  function applySnapPreview(clientX, clientY) {
    if (!snapPreviewEl) return;
    const surfaceRect = surfaceEl.getBoundingClientRect();
    const x = clientX - surfaceRect.left;
    const y = clientY - surfaceRect.top;
    const vw = surfaceRect.width;
    const vh = surfaceRect.height;
    const edge = CONST.SNAP_EDGE;
    const corner = CONST.SNAP_CORNER;
    let zone = null;
    let style = null;
    if (y < corner && x < corner) { zone = 'top-left'; style = { top: '0', left: '0', width: '50%', height: '50%' }; }
    else if (y < corner && x > vw - corner) { zone = 'top-right'; style = { top: '0', left: '50%', width: '50%', height: '50%' }; }
    else if (y > vh - corner && x < corner) { zone = 'bottom-left'; style = { top: '50%', left: '0', width: '50%', height: '50%' }; }
    else if (y > vh - corner && x > vw - corner) { zone = 'bottom-right'; style = { top: '50%', left: '50%', width: '50%', height: '50%' }; }
    else if (y < edge) { zone = 'top'; style = { top: '0', left: '0', width: '100%', height: '50%' }; }
    else if (y > vh - edge) { zone = 'bottom'; style = { top: '50%', left: '0', width: '100%', height: '50%' }; }
    else if (x < edge) { zone = 'left'; style = { top: '0', left: '0', width: '50%', height: '100%' }; }
    else if (x > vw - edge) { zone = 'right'; style = { top: '0', left: '50%', width: '50%', height: '100%' }; }

    if (!zone) {
      snapPreviewEl.removeAttribute('data-snap');
      snapPreviewEl.classList.remove('is-visible');
      snapPreviewEl.hidden = true;
      return;
    }
    snapPreviewEl.dataset.snap = zone;
    Object.assign(snapPreviewEl.style, style);
    snapPreviewEl.hidden = false;
    requestAnimationFrame(() => snapPreviewEl.classList.add('is-visible'));
  }

  function commitSnap(win) {
    if (!snapPreviewEl || !snapPreviewEl.classList.contains('is-visible')) {
      snapPreviewEl?.classList.remove('is-visible');
      if (snapPreviewEl) snapPreviewEl.hidden = true;
      return;
    }
    const zone = snapPreviewEl.dataset.snap;
    snapPreviewEl.classList.remove('is-visible');
    snapPreviewEl.hidden = true;
    if (zone) snapTo(win.id, zone);
  }

  function updateDynamicShadow(el) {
    const surfaceRect = surfaceEl.getBoundingClientRect();
    const rect = el.getBoundingClientRect();
    const centerX = rect.left + rect.width / 2;
    const centerY = rect.top + rect.height / 2;
    const offX = (centerX - (surfaceRect.left + surfaceRect.width / 2)) / 18;
    const offY = (centerY - (surfaceRect.top + surfaceRect.height / 2)) / 18 + 8;
    const blur = 28 + Math.abs(offY) / 2;
    el.style.setProperty('--win-shadow-x', `${offX.toFixed(1)}px`);
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
    snapTo,
    getViewport,
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
