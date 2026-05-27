import { getSvgIcon } from './icons.js?v=20260520-svg-icons2';

const ICON_FALLBACK = '◳';

export function createTaskbar({ container, windowManager, eventBus, t, ownerLabelFor }) {
  if (!container || !windowManager || !eventBus) {
    throw new Error('taskbar: container, windowManager, and eventBus are required');
  }
  const translate = typeof t === 'function' ? t : (_, fallback) => fallback;
  const labelFor = typeof ownerLabelFor === 'function' ? ownerLabelFor : null;

  const tokens = [];
  let renderPending = false;

  function scheduleRender() {
    if (renderPending) return;
    renderPending = true;
    requestAnimationFrame(() => {
      renderPending = false;
      render();
    });
  }

  function ownerKeyFor(win) {
    return win.ownerId || `__win__:${win.id}`;
  }

  function deriveOwnerLabel(ownerId) {
    if (!ownerId) return '';
    if (labelFor) {
      const explicit = labelFor(ownerId);
      if (explicit) return explicit;
    }
    if (ownerId.startsWith('desktop-app:')) return ownerId.slice('desktop-app:'.length);
    if (ownerId.startsWith('module:')) return ownerId.slice('module:'.length);
    return ownerId;
  }

  function render() {
    const wins = windowManager.listWindows();
    container.innerHTML = '';
    document.body?.toggleAttribute('data-shell-taskbar-open', wins.length > 0);
    if (!wins.length) return;

    const groups = new Map();
    for (const win of wins) {
      const key = ownerKeyFor(win);
      if (!groups.has(key)) {
        groups.set(key, { ownerId: win.ownerId || '', windows: [] });
      }
      groups.get(key).windows.push(win);
    }

    for (const [key, group] of groups) {
      const groupEl = document.createElement('div');
      groupEl.className = 'shell-taskbar-group';
      groupEl.dataset.ownerKey = key;
      groupEl.dataset.count = String(Math.min(group.windows.length, 3) === 3 && group.windows.length > 3 ? 'more' : group.windows.length);

      const primary = group.windows.find((w) => w.isFocused) || group.windows[0];
      groupEl.appendChild(buildItem(primary, group, true));

      if (group.windows.length > 1) {
        const badge = document.createElement('span');
        badge.className = 'shell-taskbar-badge';
        badge.textContent = String(group.windows.length);
        groupEl.appendChild(badge);
        groupEl.appendChild(buildPopover(group));
      }

      container.appendChild(groupEl);
    }
  }

  function buildItem(win, group, isPrimary) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'shell-taskbar-item';
    btn.dataset.windowId = win.id;
    btn.dataset.state = win.isFocused ? 'focused' : (win.state === 'minimized' ? 'minimized' : 'normal');
    if (win.alwaysOnTop) btn.dataset.alwaysOnTop = 'true';

    const ownerLabel = deriveOwnerLabel(win.ownerId);
    const groupCountSuffix = group.windows.length > 1 ? ` (${group.windows.length})` : '';
    const labelText = isPrimary && ownerLabel
      ? `${ownerLabel}${groupCountSuffix}`
      : win.title || ownerLabel || translate('windowDefaultTitle', 'Fenster');
    const titleText = win.title || ownerLabel || translate('windowDefaultTitle', 'Fenster');

    btn.title = titleText;
    btn.setAttribute('aria-label', titleText);

    const icon = document.createElement('span');
    icon.className = 'shell-taskbar-icon';
    const winIconKey = win.ownerId ? win.ownerId.replace(/^(desktop-app|module):/, '') : '';
    const svgHtml = getSvgIcon(winIconKey, 20, 1.8);
    if (svgHtml) {
      icon.innerHTML = svgHtml;
    } else {
      icon.textContent = win.icon || iconForOwner(win.ownerId) || ICON_FALLBACK;
    }
    btn.appendChild(icon);

    const label = document.createElement('span');
    label.className = 'shell-taskbar-label';
    label.textContent = labelText;
    btn.appendChild(label);

    btn.addEventListener('click', (event) => {
      event.preventDefault();
      activate(win);
    });
    btn.addEventListener('contextmenu', (event) => {
      event.preventDefault();
      eventBus.emit('taskbar:item_context', {
        windowId: win.id,
        ownerId: win.ownerId,
        clientX: event.clientX,
        clientY: event.clientY,
      });
    });
    return btn;
  }

  function buildPopover(group) {
    const popover = document.createElement('div');
    popover.className = 'shell-taskbar-popover';
    popover.setAttribute('role', 'menu');
    for (const win of group.windows) {
      const item = document.createElement('button');
      item.type = 'button';
      item.className = 'shell-taskbar-popover-item';
      item.dataset.windowId = win.id;
      item.dataset.state = win.isFocused ? 'focused' : (win.state === 'minimized' ? 'minimized' : 'normal');

      const icon = document.createElement('span');
      icon.className = 'shell-taskbar-icon';
      const winIconKey = win.ownerId ? win.ownerId.replace(/^(desktop-app|module):/, '') : '';
      const svgHtml = getSvgIcon(winIconKey, 18, 1.8);
      if (svgHtml) {
        icon.innerHTML = svgHtml;
      } else {
        icon.textContent = win.icon || iconForOwner(win.ownerId) || ICON_FALLBACK;
      }
      item.appendChild(icon);

      const title = document.createElement('span');
      title.className = 'shell-taskbar-popover-title';
      title.textContent = win.title || deriveOwnerLabel(win.ownerId) || translate('windowDefaultTitle', 'Fenster');
      item.appendChild(title);

      item.addEventListener('click', (event) => {
        event.preventDefault();
        event.stopPropagation();
        activate(win);
      });
      popover.appendChild(item);
    }
    return popover;
  }

  function iconForOwner(ownerId) {
    if (!ownerId) return '';
    if (ownerId.startsWith('desktop-app:')) return '◳';
    if (ownerId.startsWith('module:')) return '▦';
    return '';
  }

  function activate(win) {
    if (win.state === 'minimized') {
      windowManager.restore?.(win.id);
      windowManager.focus(win.id);
      return;
    }
    if (win.isFocused) {
      windowManager.minimize(win.id);
      return;
    }
    windowManager.focus(win.id);
  }

  const subscribeTo = [
    'window:opened',
    'window:closed',
    'window:focused',
    'window:minimized',
    'window:restored',
    'window:maximized',
    'window:title_changed',
    'window:always_on_top_changed',
    'window:snapped',
  ];
  for (const event of subscribeTo) {
    tokens.push({ event, token: eventBus.on(event, scheduleRender) });
  }

  scheduleRender();

  return {
    refresh: scheduleRender,
    destroy() {
      for (const { event, token } of tokens) {
        try {
          eventBus.off(event, token);
        } catch (error) {
          console.error('[taskbar] cleanup failed:', error);
        }
      }
      container.innerHTML = '';
      document.body?.removeAttribute('data-shell-taskbar-open');
    },
  };
}
