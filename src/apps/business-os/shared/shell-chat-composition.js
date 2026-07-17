export const SHELL_CHAT_LAYOUT_EVENT = 'ctox-business-os-chat-layout';

export function deriveShellChatInsets({ detail } = {}) {
  const payload = detail || {};
  const expanded = payload.present !== false && payload.expanded === true;
  return {
    expanded,
    side: false,
    compact: false,
    top: 0,
    right: 0,
    // The complete chat composition, including the persistent dock, floats
    // above the desktop. It must never participate in window work-area
    // geometry: opening, collapsing, or adding chats cannot reflow maximized,
    // snapped, mobile-sheet, or floating app windows.
    bottom: 0,
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
      // Chat is viewport chrome layered above every app. Keep the window
      // manager's geometry independent from all chat state changes.
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
