export const SHELL_CHAT_LAYOUT_EVENT = 'ctox-business-os-chat-layout';

const DEFAULTS = Object.freeze({
  chatDockGap: 8,
  minWindowHeight: 220,
});

export function deriveShellChatInsets({
  detail,
  viewport,
  minimumWorkArea,
  options = {},
} = {}) {
  const config = { ...DEFAULTS, ...options };
  const payload = detail || {};
  const expanded = payload.present !== false && payload.expanded === true;
  const viewportHeight = Math.max(0, Number(viewport?.h) || 0);
  const dockTop = Number(payload.dock_top);
  const dockTopInWindowSpace = Number.isFinite(dockTop)
    ? dockTop - (Number(viewport?.originTop) || 0)
    : dockTop;
  const baseBottom = payload.present !== false && Number.isFinite(dockTop)
    ? Math.max(0, viewportHeight - dockTopInWindowSpace + config.chatDockGap)
    : 0;
  const minimumHeight = Math.max(config.minWindowHeight, Number(minimumWorkArea?.height) || 0);
  const maxBottom = Math.max(baseBottom, viewportHeight - minimumHeight);
  return {
    expanded,
    side: false,
    compact: false,
    top: 0,
    right: 0,
    // Chat windows are overlays. Only the persistent bottom dock is a shell
    // work-area inset; expanding a conversation must never move or resize app
    // windows, nor turn the dock into a right-hand rail.
    bottom: Math.min(baseBottom, maxBottom),
    left: 0,
  };
}

export function createShellChatCompositionController({
  windowManager,
  bodyEl = typeof document !== 'undefined' ? document.body : null,
  rootEl = typeof document !== 'undefined' ? document.documentElement : null,
  eventTarget = typeof window !== 'undefined' ? window : null,
  options = {},
} = {}) {
  if (!windowManager?.setInsets || !windowManager?.getViewport) {
    throw new Error('shell chat composition requires a window manager');
  }

  let started = false;
  let lastDetail = { present: false, expanded: false };

  const apply = (eventOrDetail = {}) => {
    const detail = eventOrDetail?.detail || eventOrDetail || {};
    lastDetail = detail;
    const next = deriveShellChatInsets({
      detail,
      viewport: windowManager.getViewport(),
      minimumWorkArea: windowManager.getMinimumWorkArea?.(),
      options,
    });
    bodyEl?.toggleAttribute?.('data-shell-chat-dock-expanded', next.expanded);
    bodyEl?.toggleAttribute?.('data-shell-chat-dock-side', next.side);
    bodyEl?.toggleAttribute?.('data-shell-chat-dock-compact', next.compact);
    rootEl?.style?.setProperty?.('--shell-chat-dock-inset-bottom', `${next.bottom}px`);
    rootEl?.style?.setProperty?.('--shell-chat-dock-inset-right', `${next.right}px`);
    windowManager.setInsets({
      top: next.top,
      right: next.right,
      bottom: next.bottom,
      left: next.left,
    }, {
      // The dock must constrain maximized/snapped windows, but a floating
      // desktop window remains freely movable beneath the chat overlay.
      affectNormal: false,
      transient: false,
    });
    return next;
  };

  const start = () => {
    if (started) return;
    started = true;
    eventTarget?.addEventListener?.(SHELL_CHAT_LAYOUT_EVENT, apply);
  };

  const stop = () => {
    if (!started) return;
    started = false;
    eventTarget?.removeEventListener?.(SHELL_CHAT_LAYOUT_EVENT, apply);
    apply({ present: false, expanded: false });
  };

  const refresh = () => apply(lastDetail);

  return Object.freeze({ apply, refresh, start, stop });
}
