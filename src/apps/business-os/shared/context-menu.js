export function createContextMenu({ host, viewportEl }) {
  const container = host || document.body;
  const viewport = viewportEl || document.documentElement;
  let activePointerListener = null;
  let activeKeyListener = null;
  let attachTimer = null;
  let selectedIndex = -1;
  let activeMenu = null;
  let activeItems = [];

  function show(event, items) {
    if (event) {
      event.preventDefault();
      event.stopPropagation();
    }
    hide();
    if (!items?.length) return;

    activeItems = items;
    const menu = document.createElement('div');
    menu.className = 'shell-context-menu';
    menu.setAttribute('role', 'menu');
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
      el.dataset.index = String(index);
      if (item.disabled) el.setAttribute('aria-disabled', 'true');
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
        if (item.disabled) return;
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
        if (item.disabled) return;
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

    const rect = menu.getBoundingClientRect();
    const viewportRect = viewport.getBoundingClientRect();
    let x = event ? event.clientX : viewportRect.left + 20;
    let y = event ? event.clientY : viewportRect.top + 20;
    const maxX = viewportRect.right - rect.width - 8;
    const maxY = viewportRect.bottom - rect.height - 8;
    if (x > maxX) x = Math.max(viewportRect.left + 8, maxX);
    if (y > maxY) y = Math.max(viewportRect.top + 8, maxY);
    menu.style.left = `${x}px`;
    menu.style.top = `${y}px`;
    requestAnimationFrame(() => menu.classList.add('is-active'));

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
        hide();
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

  function hide() {
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
  }

  function destroy() {
    hide();
  }

  function setSelectedIndex(menu, items, index) {
    if (index < 0 || index >= items.length) return;
    if (items[index]?.type === 'separator') return;
    selectedIndex = index;
    for (const el of menu.querySelectorAll('.shell-context-menu-item')) {
      el.classList.toggle('is-selected', Number(el.dataset.index) === index);
    }
  }

  function nextSelectableIndex(items, current, direction) {
    const indices = items.map((_, i) => i).filter((i) => items[i].type !== 'separator' && !items[i].disabled);
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
