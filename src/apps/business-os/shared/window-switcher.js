export function createWindowSwitcher({ overlay, panel, windowManager, ownerLabelFor, t }) {
  if (!overlay || !panel || !windowManager) {
    throw new Error('windowSwitcher: overlay, panel, and windowManager are required');
  }
  const translate = typeof t === 'function' ? t : (_, fallback) => fallback;
  const labelFor = typeof ownerLabelFor === 'function' ? ownerLabelFor : null;

  let active = false;
  let currentIndex = 0;
  let candidates = [];
  let keyListener = null;
  let keyUpListener = null;

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

  function buildCandidates() {
    const wins = windowManager.listWindows();
    return wins.sort((a, b) => Number(b.isFocused) - Number(a.isFocused));
  }

  function render() {
    panel.innerHTML = '';
    candidates.forEach((win, index) => {
      const card = document.createElement('button');
      card.type = 'button';
      card.className = 'shell-window-switcher-card';
      if (index === currentIndex) card.classList.add('is-current');
      card.dataset.windowId = win.id;
      card.innerHTML = `
        <div class="shell-window-switcher-card-icon"></div>
        <div class="shell-window-switcher-card-title"></div>
        <div class="shell-window-switcher-card-owner"></div>
      `;
      card.querySelector('.shell-window-switcher-card-icon').textContent = win.icon || '◳';
      card.querySelector('.shell-window-switcher-card-title').textContent =
        win.title || translate('windowDefaultTitle', 'Fenster');
      card.querySelector('.shell-window-switcher-card-owner').textContent = deriveOwnerLabel(win.ownerId);
      card.addEventListener('mouseenter', () => {
        currentIndex = index;
        refreshSelection();
      });
      card.addEventListener('click', (event) => {
        event.preventDefault();
        currentIndex = index;
        commit();
      });
      panel.appendChild(card);
    });
  }

  function refreshSelection() {
    for (const card of panel.querySelectorAll('.shell-window-switcher-card')) {
      const idx = Number(card.dataset.index);
      card.classList.toggle('is-current', idx === currentIndex);
    }
    panel.children[currentIndex]?.classList.add('is-current');
    panel.children[currentIndex]?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }

  function open(initialDirection = 1) {
    candidates = buildCandidates();
    if (candidates.length < 2) return;
    active = true;
    currentIndex = clampIndex(initialDirection > 0 ? 1 : candidates.length - 1);
    render();
    overlay.hidden = false;
    overlay.setAttribute('aria-hidden', 'false');
    overlay.classList.add('is-active');
  }

  function cycle(direction) {
    if (!active || candidates.length < 2) return;
    currentIndex = clampIndex(currentIndex + direction);
    for (const card of panel.children) card.classList.remove('is-current');
    panel.children[currentIndex]?.classList.add('is-current');
    panel.children[currentIndex]?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }

  function clampIndex(index) {
    if (!candidates.length) return 0;
    const n = candidates.length;
    return ((index % n) + n) % n;
  }

  function commit() {
    if (!active) return;
    const target = candidates[currentIndex];
    cancel();
    if (target) windowManager.focus(target.id);
  }

  function cancel() {
    active = false;
    overlay.classList.remove('is-active');
    overlay.hidden = true;
    overlay.setAttribute('aria-hidden', 'true');
    candidates = [];
    currentIndex = 0;
    panel.innerHTML = '';
  }

  function isSwitcherCombo(event) {
    if (event.key !== 'Tab') return false;
    if (!(event.ctrlKey || event.metaKey)) return false;
    if (!event.altKey) return false;
    return true;
  }

  keyListener = (event) => {
    if (isSwitcherCombo(event)) {
      event.preventDefault();
      event.stopPropagation();
      if (!active) {
        open(event.shiftKey ? -1 : 1);
      } else {
        cycle(event.shiftKey ? -1 : 1);
      }
      return;
    }
    if (!active) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      cancel();
    } else if (event.key === 'Enter') {
      event.preventDefault();
      commit();
    } else if (event.key === 'ArrowRight') {
      event.preventDefault();
      cycle(1);
    } else if (event.key === 'ArrowLeft') {
      event.preventDefault();
      cycle(-1);
    }
  };

  keyUpListener = (event) => {
    if (!active) return;
    if (event.key === 'Control' || event.key === 'Meta') {
      commit();
    }
  };

  document.addEventListener('keydown', keyListener, true);
  document.addEventListener('keyup', keyUpListener, true);

  return {
    open,
    cancel,
    destroy() {
      document.removeEventListener('keydown', keyListener, true);
      document.removeEventListener('keyup', keyUpListener, true);
      cancel();
    },
  };
}
