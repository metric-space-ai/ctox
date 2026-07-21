const DEFAULT_TIME_MS = 4500;
const MAX_TOASTS = 5;
// Keep in sync with the .shell-toast fade in app.css (transition: opacity/
// transform var(--motion-base) = 160ms) plus a small buffer so the element
// is only removed after the transition has actually finished.
const TOAST_FADE_MS = 200;

// Same matchMedia guard pattern as shared/window-manager.js.
function prefersReducedMotion() {
  try {
    return typeof globalThis.matchMedia === 'function'
      && globalThis.matchMedia('(prefers-reduced-motion: reduce)').matches;
  } catch {
    return false;
  }
}

const DEFAULT_ICONS = {
  info: 'ℹ',
  success: '✓',
  warning: '!',
  error: '×',
};

export function createNotifications({ container, t }) {
  if (!container) {
    throw new Error('notifications: container is required');
  }
  const translate = typeof t === 'function' ? t : (key, fallback) => fallback ?? key;
  let counter = 0;

  function show(options = {}) {
    const id = `shell-toast-${Date.now()}-${++counter}`;
    const type = options.type && DEFAULT_ICONS[options.type] ? options.type : 'info';
    const time = Number.isFinite(options.time) ? options.time : DEFAULT_TIME_MS;

    const toast = document.createElement('div');
    toast.className = `shell-toast shell-toast--${type}`;
    toast.id = id;
    toast.setAttribute('role', type === 'error' || type === 'warning' ? 'alert' : 'status');
    toast.setAttribute('aria-live', 'polite');

    const iconEl = document.createElement('div');
    iconEl.className = 'shell-toast-icon';
    iconEl.textContent = options.icon || DEFAULT_ICONS[type];
    toast.appendChild(iconEl);

    const content = document.createElement('div');
    content.className = 'shell-toast-content';
    const titleEl = document.createElement('div');
    titleEl.className = 'shell-toast-title';
    titleEl.textContent = options.title || translate('notificationsTitle', 'Benachrichtigung');
    const bodyEl = document.createElement('div');
    bodyEl.className = 'shell-toast-body';
    bodyEl.textContent = options.message || '';
    content.appendChild(titleEl);
    content.appendChild(bodyEl);
    toast.appendChild(content);

    if (options.action && typeof options.action.callback === 'function') {
      const actionBtn = document.createElement('button');
      actionBtn.type = 'button';
      actionBtn.className = 'shell-toast-action';
      actionBtn.textContent = options.action.label || translate('openInModule', 'Öffnen');
      actionBtn.addEventListener('click', (clickEvent) => {
        clickEvent.stopPropagation();
        try {
          options.action.callback();
        } catch (error) {
          console.error('[desktop] notification action threw:', error);
        }
        close(id);
      });
      toast.appendChild(actionBtn);
    }

    while (container.childElementCount >= MAX_TOASTS) {
      const oldest = container.firstElementChild;
      if (!oldest) break;
      close(oldest.id);
    }
    container.appendChild(toast);
    if (time > 0) {
      setTimeout(() => close(id), time);
    }
    return id;
  }

  function close(id) {
    if (!id) return;
    const toast = container.querySelector(`#${cssEscape(id)}`) || document.getElementById(id);
    if (!toast || toast.classList.contains('is-fading')) return;
    toast.classList.add('is-fading');
    // The global reduced-motion block nukes the CSS fade to ~0ms; skip the
    // wait instead of leaving an invisible toast in the DOM for 200ms.
    const fadeMs = prefersReducedMotion() ? 0 : TOAST_FADE_MS;
    setTimeout(() => {
      if (toast.isConnected) toast.remove();
    }, fadeMs);
  }

  function clearAll() {
    for (const toast of Array.from(container.children)) {
      close(toast.id);
    }
  }

  function destroy() {
    clearAll();
  }

  return { show, close, clearAll, destroy };
}

function cssEscape(value) {
  if (typeof CSS !== 'undefined' && typeof CSS.escape === 'function') {
    return CSS.escape(value);
  }
  return String(value).replace(/[^a-zA-Z0-9_-]/g, (ch) => `\\${ch}`);
}
