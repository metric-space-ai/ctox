/**
 * CTOX Business OS - Reusable Column Resizer Class
 * Kapselt die Event-Handler für Maus- und Touch-Drag zur reibungslosen Anpassung von Spaltenbreiten.
 */
export class CtoxResizer {
  /**
   * @param {Object} options
   * @param {HTMLElement} options.resizerEl - Das Resizer Handle Element (z.B. mit Klasse .ctox-col-resizer)
   * @param {HTMLElement} options.containerEl - Das Container-Element des Moduls (z.B. .knowledge-module)
   * @param {string} options.cssVar - Die CSS-Variable auf dem Container, die angepasst werden soll (z.B. '--left-width' oder '--right-width')
   * @param {string} [options.side='left'] - 'left' für die linke Spalte, 'right' für die rechte Spalte
   * @param {number} [options.minWidth=200] - Mindestbreite in Pixeln
   * @param {number} [options.maxWidth=600] - Maximalbreite in Pixeln
   * @param {function} [options.onResize] - Optionaler Callback bei jeder Größenänderung
   */
  constructor({ resizerEl, containerEl, cssVar, side = 'left', minWidth = 200, maxWidth = 600, onResize }) {
    if (!resizerEl || !containerEl || !cssVar) {
      console.warn('[CtoxResizer] Missing required elements or variables.');
      return;
    }

    this.resizerEl = resizerEl;
    this.containerEl = containerEl;
    this.cssVar = cssVar;
    this.side = side;
    this.minWidth = minWidth;
    this.maxWidth = maxWidth;
    this.onResize = onResize;

    this.startX = 0;
    this.startWidth = 0;
    this.resizeRaf = 0;

    // Binde Event-Methoden fest an die Instanz
    this.onPointerDown = this.onPointerDown.bind(this);
    this.onPointerMove = this.onPointerMove.bind(this);
    this.onPointerUp = this.onPointerUp.bind(this);

    this.init();
  }

  init() {
    this.resizerEl.addEventListener('pointerdown', this.onPointerDown);
    // Stelle sicher, dass die Pointer-Events auf dem Resizer-Element aktiv sind
    this.resizerEl.style.touchAction = 'none';
  }

  onPointerDown(e) {
    e.preventDefault();
    this.startX = e.clientX;

    // Hole die aktuelle Breite
    const style = window.getComputedStyle(this.containerEl);
    const rawVal = style.getPropertyValue(this.cssVar) || '';
    
    // Parsen der aktuellen Breite in Pixeln
    let parsedWidth = parseFloat(rawVal);
    if (isNaN(parsedWidth)) {
      // Fallback: Bestimme Breite über Bounding Client Rect des betroffenen Paneels
      const panelSelector = this.side === 'left' ? '.knowledge-left, .matching-left, .shiftflow-left, .outbound-left, .research-left, .documents-left, .spreadsheets-left, .notes-left, .desktop-left, .ctox-left, .reports-left, .creator-left, .app-store-left, .fibu-left' : '.knowledge-right, .matching-right, .shiftflow-right, .outbound-right, .research-right, .documents-right, .spreadsheets-right, .notes-right, .desktop-right, .ctox-right, .reports-right, .creator-right, .app-store-right, .fibu-right';
      const panel = this.containerEl.querySelector(panelSelector) || this.resizerEl.previousElementSibling;
      parsedWidth = panel ? panel.getBoundingClientRect().width : 280;
    }

    this.startWidth = parsedWidth;

    // Aktiviere globale Klassen
    document.body.classList.add('is-resizing');
    this.resizerEl.classList.add('is-active');

    // Registriere globale Listener für Move und Up, um flüssiges Ziehen außerhalb des Handles zu ermöglichen
    window.addEventListener('pointermove', this.onPointerMove);
    window.addEventListener('pointerup', this.onPointerUp);
    window.addEventListener('pointercancel', this.onPointerUp);
  }

  onPointerMove(e) {
    if (this.resizeRaf) cancelAnimationFrame(this.resizeRaf);

    this.resizeRaf = requestAnimationFrame(() => {
      this.resizeRaf = 0;
      
      const deltaX = e.clientX - this.startX;
      let newWidth = this.startWidth;

      if (this.side === 'left') {
        newWidth = this.startWidth + deltaX;
      } else {
        newWidth = this.startWidth - deltaX;
      }

      // Einhaltung der Grenzen
      if (newWidth < this.minWidth) newWidth = this.minWidth;
      if (newWidth > this.maxWidth) newWidth = this.maxWidth;

      // Setze CSS-Variable
      this.containerEl.style.setProperty(this.cssVar, `${newWidth}px`);

      if (this.onResize) {
        this.onResize(newWidth);
      }
    });
  }

  onPointerUp() {
    if (this.resizeRaf) cancelAnimationFrame(this.resizeRaf);
    this.resizeRaf = 0;

    // Bereinige globale Klassen
    document.body.classList.remove('is-resizing');
    this.resizerEl.classList.remove('is-active');

    // Entferne globale Listener
    window.removeEventListener('pointermove', this.onPointerMove);
    window.removeEventListener('pointerup', this.onPointerUp);
    window.removeEventListener('pointercancel', this.onPointerUp);
  }

  /**
   * Zerstört die Resizer-Instanz und räumt alle Event-Listener auf.
   */
  destroy() {
    this.resizerEl.removeEventListener('pointerdown', this.onPointerDown);
    this.onPointerUp();
  }
}
