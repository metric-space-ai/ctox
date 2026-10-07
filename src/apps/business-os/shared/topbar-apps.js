// SPDX-License-Identifier: MIT OR AGPL-3.0-only
const controllers = new WeakMap();

// Keep labels whole. An app that cannot fit remains reachable in the menu.
export function chooseVisibleTopbarApps(widths, available, { gap = 6, overflowWidth = 38, priority = -1 } = {}) {
  const totalWidth = widths.reduce((sum, width) => sum + width, 0) + Math.max(0, widths.length - 1) * gap;
  if (totalWidth <= available) return widths.map((_, index) => index);
  const budget = Math.max(0, available - overflowWidth);
  const chosen = [];
  let used = 0;
  const order = widths.map((_, index) => index);
  if (priority >= 0 && priority < widths.length) order.unshift(...order.splice(priority, 1));
  for (const index of order) {
    const next = used + widths[index] + gap;
    if (next > budget) continue;
    chosen.push(index);
    used = next;
  }
  return chosen.sort((a, b) => a - b);
}

function createController(container) {
  const doc = container.ownerDocument;
  const view = doc.defaultView;
  const overflow = doc.createElement('details');
  overflow.className = 'module-overflow';
  const trigger = doc.createElement('summary');
  trigger.className = 'module-overflow-trigger';
  const menu = doc.createElement('div');
  menu.className = 'module-overflow-menu';
  menu.setAttribute('role', 'group');
  overflow.append(trigger, menu);
  const measure = doc.createElement('div');
  measure.className = 'module-tabs-measure';
  measure.setAttribute('aria-hidden', 'true');
  let items = [];
  let frame = null;
  let previousWidth = -1;
  let disposed = false;
  const de = () => doc.documentElement.lang !== 'en';
  function layout() {
    frame = null;
    if (disposed) return;
    previousWidth = container.clientWidth;
    const focused = doc.activeElement;
    const wasOpen = overflow.open;
    if (!items.length) {
      container.replaceChildren();
      return;
    }
    measure.replaceChildren(...items, overflow);
    container.replaceChildren(measure);
    overflow.open = false;
    trigger.textContent = '+' + items.length;
    const triggerWidth = trigger.getBoundingClientRect().width;
    const widths = items.map((item) => item.getBoundingClientRect().width);
    const gap = Number.parseFloat(view.getComputedStyle(container).columnGap) || 0;
    let priority = items.indexOf(focused);
    if (priority < 0) priority = items.findIndex((item) => item.getAttribute('aria-current') === 'page' || item.dataset.running === 'focused');
    const visible = new Set(chooseVisibleTopbarApps(widths, container.clientWidth, { gap, overflowWidth: triggerWidth, priority }));
    const hidden = items.filter((_, index) => !visible.has(index));
    menu.replaceChildren(...hidden);
    const nodes = items.filter((_, index) => visible.has(index));
    if (hidden.length) {
      trigger.textContent = '+' + hidden.length;
      trigger.setAttribute('aria-label', de() ? hidden.length + ' weitere Apps' : hidden.length + ' more apps');
      menu.setAttribute('aria-label', de() ? 'Weitere Apps' : 'More apps');
      nodes.push(overflow);
    }
    container.replaceChildren(...nodes);
    overflow.open = hidden.length > 0 && (wasOpen || hidden.includes(focused));
    if (focused === trigger || items.includes(focused)) {
      if (focused.isConnected) focused.focus({ preventScroll: true });
      else if (hidden.length) trigger.focus({ preventScroll: true });
    }
  }
  function refresh() {
    if (disposed || frame !== null) return;
    frame = view.requestAnimationFrame(layout);
  }
  const resize = new view.ResizeObserver(() => {
    if (container.clientWidth !== previousWidth) refresh();
  });
  resize.observe(container);
  const outside = (event) => {
    if (!overflow.contains(event.target)) overflow.open = false;
  };
  const escape = (event) => {
    if (event.key === 'Escape' && overflow.open) {
      overflow.open = false;
      trigger.focus({ preventScroll: true });
      event.preventDefault();
    }
  };
  const closeAfterLaunch = (event) => {
    if (event.target.closest('button')) overflow.open = false;
  };
  doc.addEventListener('click', outside);
  overflow.addEventListener('keydown', escape);
  menu.addEventListener('click', closeAfterLaunch);
  function dispose() {
    disposed = true;
    if (frame !== null) view.cancelAnimationFrame(frame);
    resize.disconnect();
    doc.removeEventListener('click', outside);
    overflow.removeEventListener('keydown', escape);
    menu.removeEventListener('click', closeAfterLaunch);
    controllers.delete(container);
  }
  view.addEventListener('pagehide', dispose, { once: true });
  return { set(next) { items = [...next]; if (!items.length) { if (frame !== null) view.cancelAnimationFrame(frame); layout(); } else refresh(); }, refresh, dispose };
}

export function setTopbarAppItems(container, items) {
  if (!container) return;
  let controller = controllers.get(container);
  if (!controller) {
    controller = createController(container);
    controllers.set(container, controller);
  }
  controller.set(items);
}

export function refreshTopbarAppItems(container) {
  controllers.get(container)?.refresh();
}

const avatars = new WeakSet();
export function installTopbarAvatar(button) {
  if (!button || avatars.has(button)) return;
  avatars.add(button);
  const label = button.querySelector('[data-account-label]');
  const view = button.ownerDocument.defaultView;
  function update() {
    const authenticated = button.dataset.authenticated === 'true';
    const name = String(label?.textContent || '').trim();
    const words = name.split(/\s+/).filter(Boolean);
    button.dataset.avatar = authenticated ? words.slice(0, 2).map((word) => [...word][0]).join('').toLocaleUpperCase() : '';
    if (authenticated && name) button.setAttribute('aria-label', name);
  }
  const observer = new view.MutationObserver(update);
  observer.observe(button, { childList: true, subtree: true, attributes: true, attributeFilter: ['data-authenticated'] });
  update();
  view.addEventListener('pagehide', () => observer.disconnect(), { once: true });
}
