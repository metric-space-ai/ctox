export function createContextMenu({ host, viewportEl }) {
  const container = host || document.body;
  const viewport = viewportEl || document.documentElement;
  let activePointerListener = null;
  let activeKeyListener = null;
  let attachTimer = null;
  let selectedIndex = -1;
  let activeMenu = null;
  let activeItems = [];
  let returnFocus = null;

  function show(event, items) {
    if (event) {
      event.preventDefault();
      event.stopPropagation();
    }
    hide();
    if (!items?.length) return;

    returnFocus = event?.target instanceof HTMLElement ? event.target : document.activeElement;
    activeItems = items;
    const menu = document.createElement('div');
    menu.className = 'shell-context-menu';
    menu.setAttribute('role', 'menu');
    menu.tabIndex = -1;
    items.forEach((item, index) => {
      if (item.type === 'separator') {
        const sep = document.createElement('div');
        sep.className = 'shell-context-menu-separator';
        menu.appendChild(sep);
        return;
      }
      const el = document.createElement('div');
      el.className = 'shell-context-menu-item';
      el.setAttribute('role', 'menuitem');
      el.tabIndex = -1;
      el.dataset.index = String(index);
      if (item.disabled) {
        el.setAttribute('aria-disabled', 'true');
        if (item.disabledReason) {
          el.setAttribute('aria-description', item.disabledReason);
          el.title = item.disabledReason;
        }
        el.style.opacity = '0.5';
        el.style.cursor = 'not-allowed';
      }
      const iconHtml = item.icon
        ? `<span class="shell-context-menu-icon">${escapeHtml(item.icon)}</span>`
        : '<span class="shell-context-menu-icon"></span>';
      const trailingHtml = item.trailingAction
        ? `<button class="shell-context-menu-trailing" type="button" aria-label="${escapeHtml(item.trailingLabel || '')}">${escapeHtml(item.trailingIcon || '')}</button>`
        : (item.trailingLabel ? `<span class="shell-context-menu-trailing-label">${escapeHtml(item.trailingLabel)}</span>` : '');
      el.innerHTML = `${iconHtml}<span class="shell-context-menu-label"></span>${trailingHtml}`;
      el.querySelector('.shell-context-menu-label').textContent = item.label || '';
      el.querySelector('.shell-context-menu-trailing')?.addEventListener('click', (trailingEvent) => {
        trailingEvent.preventDefault();
        trailingEvent.stopPropagation();
        if (item.disabled) {
          item.onDisabled?.();
          hide();
          return;
        }
        try {
          item.trailingAction?.();
        } catch (error) {
          console.error('[desktop] context menu trailing action threw:', error);
        }
        hide();
      });
      el.onclick = (clickEvent) => {
        clickEvent.stopPropagation();
        if (clickEvent.target.closest('.shell-context-menu-trailing')) return;
        if (item.disabled) {
          item.onDisabled?.();
          hide();
          return;
        }
        try {
          item.action?.();
        } catch (error) {
          console.error('[desktop] context menu action threw:', error);
        }
        hide();
      };
      el.onmouseenter = () => setSelectedIndex(menu, items, index);
      menu.appendChild(el);
    });

    container.appendChild(menu);
    activeMenu = menu;

    const viewportRect = viewport.getBoundingClientRect();
    menu.style.maxHeight = `${Math.max(0, viewportRect.height - 16)}px`;
    menu.style.maxWidth = `${Math.max(0, viewportRect.width - 16)}px`;
    menu.style.overflowY = 'auto';
    const rect = menu.getBoundingClientRect();
    let x = event ? event.clientX : viewportRect.left + 20;
    let y = event ? event.clientY : viewportRect.top + 20;
    const maxX = viewportRect.right - rect.width - 8;
    const maxY = viewportRect.bottom - rect.height - 8;
    x = Math.max(viewportRect.left + 8, Math.min(x, maxX));
    y = Math.max(viewportRect.top + 8, Math.min(y, maxY));
    menu.style.left = `${x}px`;
    menu.style.top = `${y}px`;
    requestAnimationFrame(() => menu.classList.add('is-active'));
    const firstIndex = nextSelectableIndex(items, -1, 1);
    if (firstIndex >= 0) setSelectedIndex(menu, items, firstIndex);
    else menu.focus();

    activePointerListener = (evt) => {
      if (!menu.contains(evt.target)) hide();
    };
    activeKeyListener = (evt) => {
      if (evt.key === 'ArrowDown') {
        evt.preventDefault();
        setSelectedIndex(menu, activeItems, nextSelectableIndex(activeItems, selectedIndex, 1));
      } else if (evt.key === 'ArrowUp') {
        evt.preventDefault();
        setSelectedIndex(menu, activeItems, nextSelectableIndex(activeItems, selectedIndex, -1));
      } else if (evt.key === 'Enter') {
        evt.preventDefault();
        const selected = menu.querySelector('.shell-context-menu-item.is-selected');
        selected?.click();
      } else if (evt.key === 'Escape') {
        evt.preventDefault();
        hide(true);
      }
    };
    clearTimeout(attachTimer);
    attachTimer = setTimeout(() => {
      document.addEventListener('mousedown', activePointerListener, true);
      document.addEventListener('contextmenu', activePointerListener, true);
      document.addEventListener('keydown', activeKeyListener);
      attachTimer = null;
    }, 10);
  }

  function hide(restoreFocus = false) {
    if (attachTimer) {
      clearTimeout(attachTimer);
      attachTimer = null;
    }
    if (activePointerListener) {
      document.removeEventListener('mousedown', activePointerListener, true);
      document.removeEventListener('contextmenu', activePointerListener, true);
      activePointerListener = null;
    }
    if (activeKeyListener) {
      document.removeEventListener('keydown', activeKeyListener);
      activeKeyListener = null;
    }
    selectedIndex = -1;
    if (activeMenu) {
      activeMenu.classList.remove('is-active');
      const node = activeMenu;
      setTimeout(() => node.remove(), 140);
      activeMenu = null;
    }
    activeItems = [];
    if (restoreFocus && returnFocus?.isConnected) returnFocus.focus?.();
    returnFocus = null;
  }

  function destroy() {
    hide();
  }

  function setSelectedIndex(menu, items, index) {
    if (index < 0 || index >= items.length) return;
    if (items[index]?.type === 'separator') return;
    selectedIndex = index;
    for (const el of menu.querySelectorAll('.shell-context-menu-item')) {
      const selected = Number(el.dataset.index) === index;
      el.classList.toggle('is-selected', selected);
      el.tabIndex = selected ? 0 : -1;
      if (selected) el.focus();
    }
  }

  function nextSelectableIndex(items, current, direction) {
    const indices = items.map((_, i) => i).filter((i) => items[i].type !== 'separator');
    if (!indices.length) return -1;
    if (current === -1) return direction === 1 ? indices[0] : indices[indices.length - 1];
    const pos = indices.indexOf(current);
    if (pos === -1) return direction === 1 ? indices[0] : indices[indices.length - 1];
    const nextPos = direction === 1
      ? (pos + 1) % indices.length
      : (pos - 1 + indices.length) % indices.length;
    return indices[nextPos];
  }

  return { show, hide, destroy };
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (ch) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[ch]));
}
