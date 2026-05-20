const DRAG_THRESHOLD_PX = 3;

export function makeIconDraggable(iconEl, {
  surface,
  iconId,
  grid = { cellW: 96, cellH: 110, offset: 24 },
  onSelect,
  onMoved,
  onDragToTopbar,
}) {
  if (!iconEl) throw new Error('makeIconDraggable: iconEl is required');
  const surfaceEl = surface || iconEl.parentElement;

  function onMouseDown(downEvent) {
    if (downEvent.button !== 0) return;
    if (downEvent.target.closest('button, a, input, select, textarea')) return;

    let dragging = false;
    const startX = downEvent.clientX;
    const startY = downEvent.clientY;
    const initialX = iconEl.offsetLeft;
    const initialY = iconEl.offsetTop;

    onSelect?.(iconId, iconEl);
    iconEl.style.zIndex = '1000';

    function onMouseMove(moveEvent) {
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
      iconEl.style.zIndex = '';
      if (!dragging) return;
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

      const offset = grid.offset ?? 24;
      const cellW = Math.max(40, grid.cellW ?? 96);
      const cellH = Math.max(40, grid.cellH ?? 110);
      const rawX = iconEl.offsetLeft;
      const rawY = iconEl.offsetTop;
      const snappedX = Math.max(offset, Math.min(Math.round((rawX - offset) / cellW) * cellW + offset, maxX));
      const snappedY = Math.max(offset, Math.min(Math.round((rawY - offset) / cellH) * cellH + offset, maxY));
      iconEl.style.left = `${snappedX}px`;
      iconEl.style.top = `${snappedY}px`;
      onMoved?.(iconId, { x: snappedX, y: snappedY }, iconEl);
    }

    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);
  }

  iconEl.addEventListener('mousedown', onMouseDown);
  return () => iconEl.removeEventListener('mousedown', onMouseDown);
}
