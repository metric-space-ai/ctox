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

// Editor and Document Server messages are HTML fragments such as
// "Fonts are not loaded.<br>Please contact …". Toasts show plain text only.
export function toPlainText(value) {
  return String(value ?? '')
    .replace(/<br\s*\/?>/gi, ' ')
    .replace(/<[^>]*>/g, '')
    .replace(/&nbsp;/gi, ' ')
    .replace(/&quot;/gi, '"')
    .replace(/&#39;|&apos;/gi, "'")
    .replace(/&lt;/gi, '<')
    .replace(/&gt;/gi, '>')
    .replace(/&amp;/gi, '&')
    .replace(/\s+/g, ' ')
    .trim();
}

// Shell lookups return the key itself for untranslated labels, so a key that
// comes back unchanged means "no translation" and the fallback wins.
function translatedLabel(translate, key, fallback) {
  const text = translate(key, fallback);
  return text && text !== key ? text : fallback;
}

export function createNotifications({ container, t }) {
  if (!container) {
    throw new Error('notifications: container is required');
  }
  const translate = typeof t === 'function' ? t : (key, fallback) => fallback ?? key;
  let counter = 0;

  function show(options = {}) {
    const type = options.type && DEFAULT_ICONS[options.type] ? options.type : 'info';
    const time = Number.isFinite(options.time) ? options.time : DEFAULT_TIME_MS;
    const titleText = toPlainText(options.title) || translatedLabel(translate, 'notificationsTitle', 'Benachrichtigung');
    const messageText = toPlainText(options.message);
    // The same message raised repeatedly (for example once per failed font
    // load) shows once while it is on screen.
    const fingerprint = `${type}\n${titleText}\n${messageText}`;
    const duplicate = Array.from(container.children).find((element) => (
      element.dataset?.fingerprint === fingerprint && !element.classList.contains('is-fading')
    ));
    if (duplicate) return duplicate.id;

    const id = `shell-toast-${Date.now()}-${++counter}`;

    const toast = document.createElement('div');
    toast.className = `shell-toast shell-toast--${type}`;
    toast.id = id;
    toast.dataset.fingerprint = fingerprint;
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
    titleEl.textContent = titleText;
    const bodyEl = document.createElement('div');
    bodyEl.className = 'shell-toast-body';
    bodyEl.textContent = messageText;
    content.appendChild(titleEl);
    content.appendChild(bodyEl);
    toast.appendChild(content);

    if (options.action && typeof options.action.callback === 'function') {
      const actionBtn = document.createElement('button');
      actionBtn.type = 'button';
      actionBtn.className = 'shell-toast-action';
      actionBtn.textContent = options.action.label || translatedLabel(translate, 'openInModule', 'Öffnen');
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

  function showSystem(options = {}) {
    const payload = normalizeSystemNotification(options, translate);
    if (!payload) return false;

    // Workjet Mobile owns the system-notification permission and delivery.
    // The signed Business OS shell can only pass this bounded, non-secret
    // presentation payload through the native surface bridge.
    if (typeof globalThis.workjetBusinessOsNotify === 'function') {
      try {
        return globalThis.workjetBusinessOsNotify(payload) !== false;
      } catch (error) {
        console.error('[notifications] mobile system notification failed:', error);
        return false;
      }
    }

    // Electron grants the isolated CTOX instance session access to the Web
    // Notification API. Ordinary browsers remain fail-closed until the user
    // has explicitly granted notification permission.
    if (typeof globalThis.Notification !== 'function'
      || globalThis.Notification.permission !== 'granted') return false;
    try {
      const notice = new globalThis.Notification(payload.title, {
        body: payload.body,
        tag: payload.tag,
      });
      notice.onclick = () => {
        globalThis.focus?.();
        try { options.action?.callback?.(); } catch (error) {
          console.error('[notifications] system notification action failed:', error);
        }
        notice.close?.();
      };
      return true;
    } catch (error) {
      console.error('[notifications] desktop system notification failed:', error);
      return false;
    }
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

  return { show, showSystem, close, clearAll, destroy };
}

export function normalizeSystemNotification(options = {}, translate = (_key, fallback) => fallback) {
  const title = boundedText(
    toPlainText(options.title) || translatedLabel(translate, 'notificationsTitle', 'Benachrichtigung'),
    160,
  );
  const body = boundedText(toPlainText(options.message || options.body || ''), 240);
  const tag = boundedToken(options.tag || '', 180);
  const kind = options.kind === 'decision_hub' ? 'decision_hub' : 'business_os';
  const urgency = ['normal', 'high', 'critical'].includes(options.urgency)
    ? options.urgency
    : 'normal';
  if (!title || !body) return null;
  return {
    kind,
    title,
    body,
    ...(tag ? { tag } : {}),
    urgency,
    ...(boundedToken(options.recordId || '', 180)
      ? { recordId: boundedToken(options.recordId, 180) }
      : {}),
  };
}

function boundedText(value, maxLength) {
  return String(value || '').replace(/\s+/g, ' ').trim().slice(0, maxLength).trim();
}

function boundedToken(value, maxLength) {
  const token = String(value || '').trim();
  return /^[A-Za-z0-9._:-]+$/.test(token) ? token.slice(0, maxLength) : '';
}

function cssEscape(value) {
  if (typeof CSS !== 'undefined' && typeof CSS.escape === 'function') {
    return CSS.escape(value);
  }
  return String(value).replace(/[^a-zA-Z0-9_-]/g, (ch) => `\\${ch}`);
}
