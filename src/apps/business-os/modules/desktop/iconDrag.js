const DRAG_THRESHOLD_PX = 3;
const TOUCH_REORDER_HOLD_MS = 360;
const TOUCH_REORDER_CANCEL_PX = 12;

export function reorderedIconIds(iconIds, draggedId, targetId) {
  const ordered = [...new Set((iconIds || []).filter(Boolean))];
  const sourceIndex = ordered.indexOf(draggedId);
  const targetIndex = ordered.indexOf(targetId);
  if (sourceIndex < 0 || targetIndex < 0 || sourceIndex === targetIndex) return ordered;
  const [dragged] = ordered.splice(sourceIndex, 1);
  ordered.splice(targetIndex, 0, dragged);
  return ordered;
}

export function reorderTargetAtPoint(parent, draggedElement, clientX, clientY) {
  if (!parent || !Number.isFinite(clientX) || !Number.isFinite(clientY)) return null;
  const candidates = [...parent.querySelectorAll('.desktop-icon[data-icon-id]')]
    .filter((element) => element !== draggedElement && element.parentElement === parent)
    .map((element) => ({ element, rect: element.getBoundingClientRect() }));
  const containing = candidates.find(({ rect }) => (
    clientX >= rect.left && clientX <= rect.right
    && clientY >= rect.top && clientY <= rect.bottom
  ));
  if (containing) return containing.element;

  let nearest = null;
  for (const candidate of candidates) {
    const centerX = candidate.rect.left + candidate.rect.width / 2;
    const centerY = candidate.rect.top + candidate.rect.height / 2;
    const distance = Math.hypot(clientX - centerX, clientY - centerY);
    const reach = Math.max(candidate.rect.width, candidate.rect.height) * 0.65;
    if (distance <= reach && (!nearest || distance < nearest.distance)) {
      nearest = { element: candidate.element, distance };
    }
  }
  return nearest?.element || null;
}

export function nearestFreeGridPosition({
  rawX,
  rawY,
  grid,
  maxX,
  maxY,
  iconWidth,
  iconHeight,
  occupied = [],
  blockedRects = [],
}) {
  const offset = Number(grid?.offset) || 0;
  const cellW = Math.max(1, Number(grid?.cellW) || Math.max(1, Number(iconWidth) || 1));
  const cellH = Math.max(1, Number(grid?.cellH) || Math.max(1, Number(iconHeight) || 1));
  const boundedMaxX = Math.max(offset, Number(maxX) || offset);
  const boundedMaxY = Math.max(offset, Number(maxY) || offset);
  const columns = Math.max(1, Math.floor((boundedMaxX - offset) / cellW) + 1);
  const rows = Math.max(1, Math.floor((boundedMaxY - offset) / cellH) + 1);
  const cellKey = (x, y) => `${Math.round((x - offset) / cellW)}:${Math.round((y - offset) / cellH)}`;
  const occupiedCells = new Set(occupied.map((position) => cellKey(position.x, position.y)));
  const width = Math.max(1, Number(iconWidth) || cellW);
  const height = Math.max(1, Number(iconHeight) || cellH);
  const intersectsWidget = (x, y) => blockedRects.some((rect) => (
    x < rect.right && x + width > rect.left && y < rect.bottom && y + height > rect.top
  ));
  const candidates = [];
  for (let row = 0; row < rows; row += 1) {
    for (let column = 0; column < columns; column += 1) {
      const x = Math.min(boundedMaxX, offset + column * cellW);
      const y = Math.min(boundedMaxY, offset + row * cellH);
      if (occupiedCells.has(`${column}:${row}`) || intersectsWidget(x, y)) continue;
      candidates.push({ x, y, distance: Math.hypot(x - rawX, y - rawY) });
    }
  }
  candidates.sort((a, b) => a.distance - b.distance || a.y - b.y || a.x - b.x);
  return candidates[0] || {
    x: Math.max(offset, Math.min(Math.round(rawX), boundedMaxX)),
    y: Math.max(offset, Math.min(Math.round(rawY), boundedMaxY)),
  };
}

export function makeIconDraggable(iconEl, {
  surface,
  iconId,
  grid = { offset: 24 },
  onSelect,
  onOpen,
  onMoved,
  onDragToTopbar,
  onReorder,
}) {
  if (!iconEl) throw new Error('makeIconDraggable: iconEl is required');
  const surfaceEl = surface || iconEl.parentElement;
  let suppressNextClick = false;

  function shouldIgnoreActivationEvent(event) {
    return event?.target?.closest?.('button, a, input, select, textarea');
  }

  function suppressClickOnce(resetAfterMs = 0) {
    suppressNextClick = true;
    setTimeout(() => {
      suppressNextClick = false;
    }, resetAfterMs);
  }

  function activateIcon(event) {
    if (shouldIgnoreActivationEvent(event)) return;
    onSelect?.(iconId, iconEl);
    onOpen?.(iconId, iconEl);
  }

  function onMouseDown(downEvent) {
    if (downEvent.button !== 0) return;
    if (shouldIgnoreActivationEvent(downEvent)) return;
    // Compact (touch) grid mode lays icons out in a scrollable flow grid and
    // CSS pins left/top with !important — positional dragging is meaningless
    // there and would persist scrambled coordinates. Fall through to the
    // plain click handler, which still selects/activates the icon.
    if (grid.flow) return;
    downEvent.preventDefault();

    let dragging = false;
    const startX = downEvent.clientX;
    const startY = downEvent.clientY;
    const initialX = iconEl.offsetLeft;
    const initialY = iconEl.offsetTop;
    const previousUserSelect = document.body.style.userSelect;
    const previousWebkitUserSelect = document.body.style.webkitUserSelect;
    document.body.style.userSelect = 'none';
    document.body.style.webkitUserSelect = 'none';

    onSelect?.(iconId, iconEl);
    iconEl.style.zIndex = '1000';

    function onMouseMove(moveEvent) {
      moveEvent.preventDefault();
      const diffX = moveEvent.clientX - startX;
      const diffY = moveEvent.clientY - startY;
      if (!dragging && (Math.abs(diffX) > DRAG_THRESHOLD_PX || Math.abs(diffY) > DRAG_THRESHOLD_PX)) {
        dragging = true;
        iconEl.classList.add('dragging');
      }
      if (dragging) {
        iconEl.style.left = `${initialX + diffX}px`;
        iconEl.style.top = `${initialY + diffY}px`;
      }
    }

    function onMouseUp(upEvent) {
      document.removeEventListener('mousemove', onMouseMove);
      document.removeEventListener('mouseup', onMouseUp);
      document.body.style.userSelect = previousUserSelect;
      document.body.style.webkitUserSelect = previousWebkitUserSelect;
      iconEl.style.zIndex = '';
      if (!dragging) {
        activateIcon(upEvent);
        suppressClickOnce();
        return;
      }
      suppressClickOnce(1000);
      dragging = false;
      iconEl.classList.remove('dragging');

      // Check if dropped inside the topbar
      const topbar = document.querySelector('.topbar');
      if (topbar && upEvent) {
        const rect = topbar.getBoundingClientRect();
        if (
          upEvent.clientX >= rect.left &&
          upEvent.clientX <= rect.right &&
          upEvent.clientY >= rect.top &&
          upEvent.clientY <= rect.bottom
        ) {
          // Trigger the pinning callback
          onDragToTopbar?.(iconId);
          // Snap back to initial position!
          iconEl.style.left = `${initialX}px`;
          iconEl.style.top = `${initialY}px`;
          return;
        }
      }

      const surfaceRect = surfaceEl?.getBoundingClientRect();
      const maxX = (surfaceRect?.width ?? globalThis.innerWidth) - iconEl.offsetWidth - 8;
      const maxY = (surfaceRect?.height ?? globalThis.innerHeight) - iconEl.offsetHeight - 8;

      const rawX = iconEl.offsetLeft;
      const rawY = iconEl.offsetTop;
      const surfaceLeft = surfaceRect?.left || 0;
      const surfaceTop = surfaceRect?.top || 0;
      const occupied = [...(iconEl.parentElement?.querySelectorAll('.desktop-icon[data-icon-id]') || [])]
        .filter((element) => element !== iconEl)
        .map((element) => ({ x: element.offsetLeft, y: element.offsetTop }));
      const blockedRects = [...(surfaceEl?.querySelectorAll('.desktop-widget-container') || [])]
        .map((element) => element.getBoundingClientRect())
        .filter((rect) => rect.width > 0 && rect.height > 0)
        .map((rect) => ({
          left: rect.left - surfaceLeft,
          top: rect.top - surfaceTop,
          right: rect.right - surfaceLeft,
          bottom: rect.bottom - surfaceTop,
        }));
      const snapped = nearestFreeGridPosition({
        rawX,
        rawY,
        grid,
        maxX,
        maxY,
        iconWidth: iconEl.offsetWidth,
        iconHeight: iconEl.offsetHeight,
        occupied,
        blockedRects,
      });
      const finalX = snapped.x;
      const finalY = snapped.y;
      iconEl.style.left = `${finalX}px`;
      iconEl.style.top = `${finalY}px`;
      onMoved?.(iconId, { x: finalX, y: finalY }, iconEl);
    }

    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);
  }

  function onPointerDown(downEvent) {
    if (!grid.flow || downEvent.pointerType !== 'pen') return;
    if (downEvent.isPrimary === false || shouldIgnoreActivationEvent(downEvent)) return;

    const parent = iconEl.parentElement;
    if (!parent) return;
    const startX = downEvent.clientX;
    const startY = downEvent.clientY;
    let reordering = false;
    let dropTarget = null;
    let holdTimer = setTimeout(() => {
      holdTimer = null;
      reordering = true;
      suppressClickOnce();
      onSelect?.(iconId, iconEl);
      parent.classList.add('is-touch-reordering');
      iconEl.classList.add('touch-reordering');
      iconEl.setAttribute('aria-grabbed', 'true');
      try { iconEl.setPointerCapture?.(downEvent.pointerId); } catch {}
      try { globalThis.navigator?.vibrate?.(12); } catch {}
    }, TOUCH_REORDER_HOLD_MS);

    const clearDropTarget = () => {
      dropTarget?.classList.remove('touch-drop-target');
      dropTarget = null;
    };

    const finish = (upEvent, { cancelled = false } = {}) => {
      if (holdTimer) clearTimeout(holdTimer);
      holdTimer = null;
      document.removeEventListener('pointermove', onPointerMove);
      document.removeEventListener('pointerup', onPointerUp);
      document.removeEventListener('pointercancel', onPointerCancel);
      if (!reordering) return;

      suppressClickOnce(1000);

      const targetId = String(dropTarget?.dataset?.iconId || '');
      clearDropTarget();
      parent.classList.remove('is-touch-reordering');
      iconEl.classList.remove('touch-reordering');
      iconEl.removeAttribute('aria-grabbed');
      iconEl.style.removeProperty('--touch-drag-x');
      iconEl.style.removeProperty('--touch-drag-y');
      try { iconEl.releasePointerCapture?.(upEvent?.pointerId); } catch {}
      if (!cancelled && targetId && targetId !== iconId) {
        const ids = [...parent.querySelectorAll('.desktop-icon[data-icon-id]')]
          .map((node) => String(node.dataset.iconId || ''));
        const orderedIds = reorderedIconIds(ids, iconId, targetId);
        Promise.resolve(onReorder?.(orderedIds, { iconId, targetId, element: iconEl }))
          .catch((error) => console.error('[desktop] touch reorder failed:', error));
        try { globalThis.navigator?.vibrate?.(20); } catch {}
      }
      reordering = false;
    };

    function onPointerMove(moveEvent) {
      if (moveEvent.pointerId !== downEvent.pointerId) return;
      const diffX = moveEvent.clientX - startX;
      const diffY = moveEvent.clientY - startY;
      if (!reordering) {
        if (Math.hypot(diffX, diffY) > TOUCH_REORDER_CANCEL_PX && holdTimer) {
          clearTimeout(holdTimer);
          holdTimer = null;
        }
        return;
      }
      moveEvent.preventDefault();
      iconEl.style.setProperty('--touch-drag-x', `${diffX}px`);
      iconEl.style.setProperty('--touch-drag-y', `${diffY}px`);
      const candidate = reorderTargetAtPoint(parent, iconEl, moveEvent.clientX, moveEvent.clientY);
      if (!candidate || candidate === iconEl || candidate.parentElement !== parent) return;
      if (candidate !== dropTarget) {
        clearDropTarget();
        dropTarget = candidate;
        dropTarget.classList.add('touch-drop-target');
        try { globalThis.navigator?.vibrate?.(5); } catch {}
      }
    }

    function onPointerUp(upEvent) {
      if (upEvent.pointerId !== downEvent.pointerId) return;
      finish(upEvent);
    }

    function onPointerCancel(cancelEvent) {
      if (cancelEvent.pointerId !== downEvent.pointerId) return;
      finish(cancelEvent, { cancelled: true });
    }

    document.addEventListener('pointermove', onPointerMove, { passive: false });
    document.addEventListener('pointerup', onPointerUp);
    document.addEventListener('pointercancel', onPointerCancel);
  }

  function onTouchStart(startEvent) {
    if (!grid.flow || startEvent.touches?.length !== 1) return;
    if (shouldIgnoreActivationEvent(startEvent)) return;

    const parent = iconEl.parentElement;
    const firstTouch = startEvent.changedTouches?.[0];
    if (!parent || !firstTouch) return;
    const touchId = firstTouch.identifier;
    const startX = firstTouch.clientX;
    const startY = firstTouch.clientY;
    let reordering = false;
    let dropTarget = null;
    let holdTimer = setTimeout(() => {
      holdTimer = null;
      reordering = true;
      suppressClickOnce();
      onSelect?.(iconId, iconEl);
      parent.classList.add('is-touch-reordering');
      iconEl.classList.add('touch-reordering');
      iconEl.setAttribute('aria-grabbed', 'true');
      try { globalThis.navigator?.vibrate?.(12); } catch {}
    }, TOUCH_REORDER_HOLD_MS);

    const touchFrom = (list) => [...(list || [])].find((touch) => touch.identifier === touchId);
    const clearDropTarget = () => {
      dropTarget?.classList.remove('touch-drop-target');
      dropTarget = null;
    };
    const finish = ({ cancelled = false, clientX, clientY } = {}) => {
      if (holdTimer) clearTimeout(holdTimer);
      holdTimer = null;
      document.removeEventListener('touchmove', onTouchMove);
      document.removeEventListener('touchend', onTouchEnd);
      document.removeEventListener('touchcancel', onTouchCancel);
      if (!reordering) return;

      suppressClickOnce(1000);
      const pointTarget = reorderTargetAtPoint(parent, iconEl, clientX, clientY);
      const effectiveTarget = dropTarget
        || (pointTarget && pointTarget !== iconEl && pointTarget.parentElement === parent ? pointTarget : null);
      const targetId = String(effectiveTarget?.dataset?.iconId || '');
      clearDropTarget();
      parent.classList.remove('is-touch-reordering');
      iconEl.classList.remove('touch-reordering');
      iconEl.removeAttribute('aria-grabbed');
      iconEl.style.removeProperty('--touch-drag-x');
      iconEl.style.removeProperty('--touch-drag-y');
      if (!cancelled && targetId && targetId !== iconId) {
        const ids = [...parent.querySelectorAll('.desktop-icon[data-icon-id]')]
          .map((node) => String(node.dataset.iconId || ''));
        const orderedIds = reorderedIconIds(ids, iconId, targetId);
        Promise.resolve(onReorder?.(orderedIds, { iconId, targetId, element: iconEl }))
          .catch((error) => console.error('[desktop] touch reorder failed:', error));
        try { globalThis.navigator?.vibrate?.(20); } catch {}
      }
      reordering = false;
    };

    function onTouchMove(moveEvent) {
      const touch = touchFrom(moveEvent.changedTouches) || touchFrom(moveEvent.touches);
      if (!touch) return;
      const diffX = touch.clientX - startX;
      const diffY = touch.clientY - startY;
      if (!reordering) {
        if (Math.hypot(diffX, diffY) > TOUCH_REORDER_CANCEL_PX && holdTimer) {
          clearTimeout(holdTimer);
          holdTimer = null;
        }
        return;
      }
      moveEvent.preventDefault();
      iconEl.style.setProperty('--touch-drag-x', `${diffX}px`);
      iconEl.style.setProperty('--touch-drag-y', `${diffY}px`);
      const candidate = reorderTargetAtPoint(parent, iconEl, touch.clientX, touch.clientY);
      if (!candidate || candidate === iconEl || candidate.parentElement !== parent) return;
      if (candidate !== dropTarget) {
        clearDropTarget();
        dropTarget = candidate;
        dropTarget.classList.add('touch-drop-target');
        try { globalThis.navigator?.vibrate?.(5); } catch {}
      }
    }

    function onTouchEnd(endEvent) {
      const touch = touchFrom(endEvent.changedTouches);
      if (!touch) return;
      finish({ clientX: touch.clientX, clientY: touch.clientY });
    }

    function onTouchCancel(cancelEvent) {
      if (!touchFrom(cancelEvent.changedTouches)) return;
      finish({ cancelled: true });
    }

    document.addEventListener('touchmove', onTouchMove, { passive: false });
    document.addEventListener('touchend', onTouchEnd);
    document.addEventListener('touchcancel', onTouchCancel);
  }

  function onClick(clickEvent) {
    if (suppressNextClick) {
      suppressNextClick = false;
      return;
    }
    activateIcon(clickEvent);
  }

  iconEl.addEventListener('mousedown', onMouseDown);
  iconEl.addEventListener('pointerdown', onPointerDown);
  iconEl.addEventListener('touchstart', onTouchStart, { passive: true });
  iconEl.addEventListener('click', onClick);
  return () => {
    iconEl.removeEventListener('mousedown', onMouseDown);
    iconEl.removeEventListener('pointerdown', onPointerDown);
    iconEl.removeEventListener('touchstart', onTouchStart);
    iconEl.removeEventListener('click', onClick);
  };
}
