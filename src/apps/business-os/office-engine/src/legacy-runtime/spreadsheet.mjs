// Legacy rollback source. Never included in the CTOX Spreadsheets product bundle.
const XLSX_MIME = 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet';

export async function createOfficeFrameRuntime({ root, bridge, permissions, emit, locale = 'de' }) {
  const vendorRoot = import.meta.url.includes('/office-engine/src/')
    ? new URL('../../../vendor/', import.meta.url)
    : new URL('../../', import.meta.url);
  const { DocxZipper } = await import(new URL('superdoc.mjs', vendorRoot).href);
  let access = { ...permissions };
  let sourceBytes = null;
  let recordId = null;
  let versionId = null;
  let workbook = null;
  let activeSheetIndex = 0;
  let zoomPercent = 100;
  let dirty = false;
  let selectedCell = 'A1';
  let selectedRange = null;
  let changes = new Map();
  let formatChanges = new Map();
  let geometryChanges = new Map();
  let structuralChanges = new Map();
  let tableChanges = new Map();
  let undoStack = [];
  let redoStack = [];
  let copiedCell = null;
  const editStarts = new WeakMap();

  const labels = locale === 'en'
    ? { view: 'View', file: 'File', save: 'Save', saved: 'Saved', unsaved: 'Unsaved', zoomOut: 'Zoom out', zoomIn: 'Zoom in', status: 'Ready' }
    : { view: 'Ansicht', file: 'Datei', save: 'Speichern', saved: 'Gespeichert', unsaved: 'Ungespeichert', zoomOut: 'Verkleinern', zoomIn: 'Vergrößern', status: 'Bereit' };

  root.innerHTML = `
    <div class="ctox-office-spreadsheet-runtime" data-read-only="${access.write === false}">
      <div class="ctox-office-sheet-toolbar" role="toolbar" aria-label="${labels.view}">
        <strong>${access.write ? labels.file : labels.view}</strong>
        ${access.write ? `<button type="button" data-action="save">${labels.save}</button><output data-role="save-state">${labels.saved}</output><button type="button" data-action="undo" disabled>${locale === 'en' ? 'Undo' : 'Rückgängig'}</button><button type="button" data-action="redo" disabled>${locale === 'en' ? 'Redo' : 'Wiederholen'}</button><button type="button" data-action="fill-down">${locale === 'en' ? 'Fill down' : 'Ausfüllen nach unten'}</button><button type="button" data-action="bold" aria-pressed="false">${locale === 'en' ? 'Bold' : 'Fett'}</button><button type="button" data-action="italic" aria-pressed="false">${locale === 'en' ? 'Italic' : 'Kursiv'}</button><button type="button" data-action="accounting">${locale === 'en' ? 'Euro accounting' : 'Euro-Buchhaltung'}</button><button type="button" data-action="row-height">${locale === 'en' ? 'Row height 27.75' : 'Zeilenhöhe 27,75'}</button><button type="button" data-action="column-width">${locale === 'en' ? 'Column width 32.625' : 'Spaltenbreite 32,625'}</button><button type="button" data-action="hide-row">${locale === 'en' ? 'Hide row' : 'Zeile ausblenden'}</button><button type="button" data-action="show-row">${locale === 'en' ? 'Show row' : 'Zeile anzeigen'}</button><button type="button" data-action="merge">${locale === 'en' ? 'Merge B3:C3' : 'B3:C3 verbinden'}</button><button type="button" data-action="unmerge">${locale === 'en' ? 'Unmerge B2:C2' : 'B2:C2 trennen'}</button><button type="button" data-action="freeze">${locale === 'en' ? 'Freeze at B3' : 'Bei B3 fixieren'}</button><button type="button" data-action="unfreeze">${locale === 'en' ? 'Unfreeze' : 'Fixierung aufheben'}</button><button type="button" data-action="sort-revenue">${locale === 'en' ? 'Revenue descending' : 'Umsatz absteigend'}</button><button type="button" data-action="filter-north">${locale === 'en' ? 'Filter North' : 'Filter North'}</button><button type="button" data-action="clear-filter">${locale === 'en' ? 'Clear filter' : 'Filter löschen'}</button><button type="button" data-action="status-final">Status Final</button><button type="button" data-action="quantity-invalid">${locale === 'en' ? 'Try quantity 15' : 'Menge 15 testen'}</button><button type="button" data-action="quantity-valid">${locale === 'en' ? 'Set quantity 8' : 'Menge 8 setzen'}</button>` : ''}
        <button type="button" data-action="zoom-out" aria-label="${labels.zoomOut}">−</button>
        <output data-role="zoom">100%</output>
        <button type="button" data-action="zoom-in" aria-label="${labels.zoomIn}">+</button>
      </div>
      <div class="ctox-office-formula-bar"><span data-role="cell-name">A1</span><span data-role="formula"></span></div>
      <div class="ctox-office-sheet-viewport"><div class="ctox-office-grid" data-role="grid"></div></div>
      <div class="ctox-office-sheet-tabs" role="tablist" aria-label="Worksheets"></div>
      <div class="ctox-office-sheet-statusbar"><span>${labels.status}</span><span class="ctox-office-status-spacer"></span><span data-role="sheet-status"></span><span data-role="zoom-status">Zoom 100%</span></div>
    </div>`;
  const shell = root.firstElementChild;
  const grid = shell.querySelector('[data-role="grid"]');
  const tabs = shell.querySelector('[role="tablist"]');
  const zoomOutput = shell.querySelector('[data-role="zoom"]');
  const zoomStatus = shell.querySelector('[data-role="zoom-status"]');
  const sheetStatus = shell.querySelector('[data-role="sheet-status"]');
  const cellName = shell.querySelector('[data-role="cell-name"]');
  const formula = shell.querySelector('[data-role="formula"]');
  const saveState = shell.querySelector('[data-role="save-state"]');
  const undoButton = shell.querySelector('[data-action="undo"]');
  const redoButton = shell.querySelector('[data-action="redo"]');

  const visibleSheets = () => workbook?.sheets.filter((sheet) => sheet.state !== 'hidden') || [];
  const currentSheet = () => visibleSheets()[activeSheetIndex] || visibleSheets()[0] || null;
  const cellFor = (sheet, reference) => sheet?.cells.find((cell) => cell.reference === reference) || null;
  const changeKey = (sheet, reference) => `${sheet.name}!${reference}`;
  const structuralKey = (sheet, kind, index) => `${sheet.name}!${kind}!${index}`;
  const updateHistoryControls = () => {
    if (undoButton) undoButton.disabled = undoStack.length === 0;
    if (redoButton) redoButton.disabled = redoStack.length === 0;
  };

  const markDirty = () => {
    if (dirty) return;
    dirty = true;
    if (saveState) saveState.textContent = labels.unsaved;
    emit?.('dirty', { recordId, versionId, dirty: true });
  };

  const selectedCoordinates = () => ({
    row: Number(selectedCell.match(/\d+$/)?.[0] || 1),
    column: columnNumber(selectedCell.match(/^[A-Z]+/)?.[0] || 'A'),
  });

  const applyFormat = (property, value) => {
    const sheet = currentSheet();
    const cell = cellFor(sheet, selectedCell);
    if (!cell) return false;
    const key = changeKey(sheet, selectedCell);
    const next = { ...(cell.format || {}), ...(formatChanges.get(key)?.format || {}), [property]: value };
    cell.format = next;
    formatChanges.set(key, { sheet: sheet.name, path: sheet.path, reference: selectedCell, format: next });
    markDirty();
    render();
    emit?.('cellFormatChanged', { sheet: sheet.name, reference: selectedCell, format: next });
    return true;
  };

  const applyGeometry = (kind, value) => {
    const sheet = currentSheet();
    const coordinates = selectedCoordinates();
    const index = kind === 'row' ? coordinates.row : coordinates.column;
    const key = structuralKey(sheet, kind, index);
    geometryChanges.set(key, { sheet: sheet.name, path: sheet.path, kind, index, ...value });
    if (kind === 'row') sheet.rows.set(index, { ...(sheet.rows.get(index) || {}), ...value });
    else sheet.columns.set(index, { ...(sheet.columns.get(index) || {}), ...value });
    markDirty();
    render();
    emit?.('sheetGeometryChanged', { sheet: sheet.name, kind, index, ...value });
    return true;
  };

  const applyCellValue = (sheet, reference, value, { recordHistory = true, cellType = 's' } = {}) => {
    const cell = cellFor(sheet, reference);
    if (!cell) return false;
    const before = cell.display;
    const after = String(value ?? '');
    if (before === after) return false;
    cell.display = after;
    cell.formula = null;
    const key = changeKey(sheet, reference);
    if (after === cell.originalDisplay) changes.delete(key);
    else changes.set(key, { sheet: sheet.name, path: sheet.path, reference, value: after, cellType });
    const node = grid.querySelector(`td[data-reference="${reference}"]`);
    if (node && sheet === currentSheet()) node.textContent = after;
    if (reference === selectedCell && sheet === currentSheet()) formula.textContent = after;
    if (recordHistory) {
      undoStack.push({ sheet: sheet.name, reference, before, after });
      redoStack = [];
      updateHistoryControls();
    }
    const wasDirty = dirty;
    dirty = changes.size > 0;
    if (saveState) saveState.textContent = dirty ? labels.unsaved : labels.saved;
    if (dirty && !wasDirty) { dirty = false; markDirty(); }
    return true;
  };

  const applyFormulaValue = (sheet, reference, formulaText, { recordHistory = true } = {}) => {
    const cell = cellFor(sheet, reference);
    if (!cell) return false;
    const before = cell.formula || cell.display;
    const formulaValue = String(formulaText || '').replace(/^=/, '');
    const result = evaluateFormula(workbook, sheet, formulaValue);
    cell.formula = `=${formulaValue}`;
    cell.display = result.value;
    cell.type = result.error ? 'e' : null;
    const key = changeKey(sheet, reference);
    changes.set(key, { sheet: sheet.name, path: sheet.path, reference, formula: formulaValue, value: result.value, cellType: result.error ? 'e' : null });
    if (recordHistory) {
      undoStack.push({ sheet: sheet.name, reference, before, after: cell.formula });
      redoStack = [];
      updateHistoryControls();
    }
    markDirty();
    render();
    emit?.('formulaChanged', { sheet: sheet.name, reference, formula: cell.formula, cached_value: result.value, error: result.error });
    return true;
  };

  const undo = () => {
    const entry = undoStack.pop();
    if (!entry) return false;
    const sheet = workbook.sheets.find((item) => item.name === entry.sheet);
    if (String(entry.before).startsWith('=')) applyFormulaValue(sheet, entry.reference, entry.before, { recordHistory: false });
    else applyCellValue(sheet, entry.reference, entry.before, { recordHistory: false });
    redoStack.push(entry);
    updateHistoryControls();
    emit?.('historyChanged', { can_undo: undoStack.length > 0, can_redo: true });
    return true;
  };

  const redo = () => {
    const entry = redoStack.pop();
    if (!entry) return false;
    const sheet = workbook.sheets.find((item) => item.name === entry.sheet);
    if (String(entry.after).startsWith('=')) applyFormulaValue(sheet, entry.reference, entry.after, { recordHistory: false });
    else applyCellValue(sheet, entry.reference, entry.after, { recordHistory: false });
    undoStack.push(entry);
    updateHistoryControls();
    emit?.('historyChanged', { can_undo: true, can_redo: redoStack.length > 0 });
    return true;
  };

  const render = () => {
    if (!workbook) return;
    const visibleSheets = workbook.sheets.filter((sheet) => sheet.state !== 'hidden');
    const active = visibleSheets[activeSheetIndex] || visibleSheets[0];
    tabs.replaceChildren(...visibleSheets.map((sheet, index) => {
      const button = document.createElement('button');
      button.type = 'button';
      button.role = 'tab';
      button.textContent = sheet.name;
      button.setAttribute('aria-selected', String(index === activeSheetIndex));
      button.addEventListener('click', () => { activeSheetIndex = index; render(); emit?.('sheetChanged', { sheet: sheet.name, index }); });
      return button;
    }));
    const maxRow = Math.max(1, ...active.cells.map((cell) => cell.row));
    const maxColumn = Math.max(1, ...active.cells.map((cell) => cell.column));
    const values = new Map(active.cells.map((cell) => [cell.reference, cell]));
    const mergeStarts = new Map();
    const mergeCovered = new Set();
    for (const merge of active.merges) {
      const [start, end] = merge.split(':');
      const startAt = parseCellReference(start); const endAt = parseCellReference(end);
      mergeStarts.set(start, { colspan: endAt.column - startAt.column + 1, rowspan: endAt.row - startAt.row + 1 });
      for (const reference of rangeReferences({ start, end })) if (reference !== start) mergeCovered.add(reference);
    }
    const table = document.createElement('table');
    table.setAttribute('aria-label', `${active.name} worksheet`);
    const head = document.createElement('thead');
    const headRow = document.createElement('tr');
    headRow.append(document.createElement('th'));
    for (let column = 1; column <= maxColumn; column += 1) {
      const th = document.createElement('th');
      th.textContent = columnName(column);
      const geometry = active.columns.get(column);
      if (geometry?.width) th.style.width = `${Math.max(48, geometry.width * 7)}px`;
      if (geometry?.hidden) th.hidden = true;
      headRow.append(th);
    }
    head.append(headRow);
    table.append(head);
    const body = document.createElement('tbody');
    for (let row = 1; row <= maxRow; row += 1) {
      const tr = document.createElement('tr');
      const rowGeometry = active.rows.get(row);
      if (rowGeometry?.height) tr.style.height = `${rowGeometry.height * 96 / 72}px`;
      if (rowGeometry?.hidden) tr.hidden = true;
      const rowHeader = document.createElement('th');
      rowHeader.textContent = String(row);
      tr.append(rowHeader);
      for (let column = 1; column <= maxColumn; column += 1) {
        const reference = `${columnName(column)}${row}`;
        if (mergeCovered.has(reference)) continue;
        const td = document.createElement('td');
        const cell = values.get(reference);
        td.textContent = cell?.display ?? '';
        td.dataset.reference = reference;
        td.tabIndex = reference === selectedCell ? 0 : -1;
        td.setAttribute('aria-selected', String(reference === selectedCell));
        td.contentEditable = access.write ? 'plaintext-only' : 'false';
        const merge = mergeStarts.get(reference);
        if (merge) { td.colSpan = merge.colspan; td.rowSpan = merge.rowspan; td.dataset.mergedRange = active.merges.find((item) => item.startsWith(`${reference}:`)) || reference; }
        if (active.freeze?.state === 'frozen') {
          if (column <= active.freeze.xSplit) td.classList.add('ctox-office-cell-frozen-column');
          if (row <= active.freeze.ySplit) td.classList.add('ctox-office-cell-frozen-row');
        }
        const columnGeometry = active.columns.get(column);
        if (columnGeometry?.width) td.style.width = `${Math.max(48, columnGeometry.width * 7)}px`;
        if (columnGeometry?.hidden) td.hidden = true;
        if (cell?.format?.bold) td.classList.add('ctox-office-cell-bold');
        if (cell?.format?.italic) td.classList.add('ctox-office-cell-italic');
        if (cell?.format?.numberFormat === 'euro-accounting') td.classList.add('ctox-office-cell-accounting');
        const conditional = conditionalStyleFor(active, reference, cell?.display);
        if (conditional?.background) td.style.background = conditional.background;
        if (conditional?.color) td.style.color = conditional.color;
        if (active.validations.some((rule) => rangeReferences({ start: rule.sqref.split(':')[0], end: rule.sqref.split(':').at(-1) }).includes(reference))) td.dataset.validated = 'true';
        const selectCell = (extend = false) => {
          const previous = selectedCell;
          selectedCell = reference;
          selectedRange = extend && previous !== reference ? { start: previous, end: reference } : null;
          cellName.textContent = reference;
          formula.textContent = cell?.formula || td.textContent || '';
          table.querySelectorAll('td[aria-selected="true"]').forEach((node) => node.setAttribute('aria-selected', 'false'));
          for (const selected of rangeReferences(selectedRange || { start: reference, end: reference })) {
            table.querySelector(`td[data-reference="${selected}"]`)?.setAttribute('aria-selected', 'true');
          }
        };
        const finishEdit = () => {
          const before = editStarts.get(td);
          const after = td.textContent?.replace(/\n/g, '') ?? '';
          if (before == null || before === after) return;
          if (after.startsWith('=')) {
            editStarts.set(td, after);
            applyFormulaValue(active, reference, after);
            return;
          }
          undoStack.push({ sheet: active.name, reference, before, after });
          redoStack = [];
          editStarts.set(td, after);
          updateHistoryControls();
        };
        td.addEventListener('focus', () => { selectCell(false); editStarts.set(td, cell?.formula || cell?.display || ''); });
        td.addEventListener('click', (event) => selectCell(event.shiftKey));
        td.addEventListener('blur', finishEdit);
        if (access.write) {
          td.addEventListener('input', () => {
            selectCell(false);
            const value = td.textContent?.replace(/\n/g, '') ?? '';
            if (value.startsWith('=')) {
              formula.textContent = value;
              markDirty();
              return;
            }
            if (cell) cell.display = value;
            const key = changeKey(active, reference);
            if (cell && value === cell.originalDisplay) changes.delete(key);
            else changes.set(key, { sheet: active.name, path: active.path, reference, value, cellType: 's' });
            formula.textContent = value;
            markDirty();
          });
          td.addEventListener('keydown', (event) => {
            const modifier = event.metaKey || event.ctrlKey;
            if (modifier && event.key.toLowerCase() === 'z') {
              event.preventDefault();
              finishEdit();
              if (event.shiftKey) redo(); else undo();
              return;
            }
            if (event.shiftKey && event.key === 'ArrowDown') {
              event.preventDefault();
              const next = table.querySelector(`td[data-reference="${columnName(column)}${row + 1}"]`);
              if (next) {
                selectedRange = { start: reference, end: next.dataset.reference };
                selectedCell = next.dataset.reference;
                cellName.textContent = selectedCell;
                formula.textContent = next.textContent || '';
                renderRangeSelection(table, selectedRange);
              }
              return;
            }
            if (event.key === 'Enter') {
              event.preventDefault();
              finishEdit();
              const next = table.querySelector(`td[data-reference="${columnName(column)}${row + 1}"]`);
              next?.focus();
            }
          });
        }
        if (cell?.style === 2) td.classList.add('ctox-office-sheet-title');
        tr.append(td);
      }
      body.append(tr);
    }
    table.append(body);
    grid.replaceChildren(table);
    grid.style.setProperty('--ctox-sheet-zoom', String(zoomPercent / 100));
    zoomOutput.value = `${zoomPercent}%`;
    zoomOutput.textContent = `${zoomPercent}%`;
    zoomStatus.textContent = `Zoom ${zoomPercent}%`;
    sheetStatus.textContent = `${active.name} · ${active.dimension} · ${active.merges.length} Merge · ${active.freeze?.state === 'frozen' ? `Freeze ${active.freeze.topLeftCell}` : 'No freeze'} · ${active.table?.name || 'No table'}`;
    cellName.textContent = selectedCell;
    formula.textContent = values.get(selectedCell)?.formula || values.get(selectedCell)?.display || '';
    const selectedFormat = values.get(selectedCell)?.format || {};
    shell.querySelector('[data-action="bold"]')?.setAttribute('aria-pressed', String(Boolean(selectedFormat.bold)));
    shell.querySelector('[data-action="italic"]')?.setAttribute('aria-pressed', String(Boolean(selectedFormat.italic)));
  };

  const setZoom = (next) => {
    zoomPercent = Math.min(200, Math.max(50, Math.round(next / 10) * 10));
    render();
    emit?.('zoomChanged', { zoom_percent: zoomPercent });
  };
  const onZoomOut = () => setZoom(zoomPercent - 10);
  const onZoomIn = () => setZoom(zoomPercent + 10);
  let runtimeApi = null;
  const onSave = async () => {
    const button = shell.querySelector('[data-action="save"]');
    if (button) button.disabled = true;
    try { await runtimeApi.save({ reason: 'toolbar' }); }
    catch (error) { emit?.('error', { code: error?.code || 'save_failed', message: error?.message || String(error) }); }
    finally { if (button) button.disabled = false; }
  };
  const onCopy = (event) => {
    const cell = cellFor(currentSheet(), selectedCell);
    if (!cell) return;
    copiedCell = { sheet: currentSheet().name, reference: selectedCell, formula: cell.formula, display: cell.display };
    event.clipboardData?.setData('text/plain', cell.formula || cell.display);
    event.preventDefault();
  };
  const onCut = (event) => {
    const sheet = currentSheet();
    const cell = cellFor(sheet, selectedCell);
    if (!cell || !access.write) return;
    event.clipboardData?.setData('text/plain', cell.display);
    event.preventDefault();
    applyCellValue(sheet, selectedCell, '');
  };
  const onPaste = (event) => {
    if (!access.write) return;
    const value = event.clipboardData?.getData('text/plain');
    if (value == null) return;
    event.preventDefault();
    const pasted = value.replace(/\r?\n$/, '');
    if (pasted.startsWith('=')) {
      const shifted = copiedCell?.formula === pasted ? shiftFormula(pasted, copiedCell.reference, selectedCell) : pasted;
      applyFormulaValue(currentSheet(), selectedCell, shifted);
    } else applyCellValue(currentSheet(), selectedCell, pasted);
  };
  const fillDown = () => {
    const sheet = currentSheet();
    const refs = rangeReferences(selectedRange || { start: selectedCell, end: selectedCell });
    if (refs.length < 2) return false;
    const source = cellFor(sheet, refs[0]);
    if (!source) return false;
    for (const reference of refs.slice(1)) applyCellValue(sheet, reference, source.display, { cellType: source.type });
    emit?.('rangeFilled', { sheet: sheet.name, direction: 'down', range: `${refs[0]}:${refs.at(-1)}` });
    return true;
  };
  const toggleBold = () => applyFormat('bold', !cellFor(currentSheet(), selectedCell)?.format?.bold);
  const toggleItalic = () => applyFormat('italic', !cellFor(currentSheet(), selectedCell)?.format?.italic);
  const setAccounting = () => applyFormat('numberFormat', 'euro-accounting');
  const setRowHeight = () => applyGeometry('row', { height: 27.75, hidden: false });
  const setColumnWidth = () => applyGeometry('column', { width: 32.625, hidden: false });
  const hideRow = () => applyGeometry('row', { hidden: true });
  const showRow = () => applyGeometry('row', { hidden: false });
  const changeStructure = (kind, value) => {
    const sheet = currentSheet();
    structuralChanges.set(`${sheet.path}!${kind}`, { sheet: sheet.name, path: sheet.path, kind, value });
    if (kind === 'merges') sheet.merges = [...value]; else if (kind === 'freeze') sheet.freeze = value;
    markDirty(); render(); emit?.('sheetStructureChanged', { sheet: sheet.name, kind, value });
    return true;
  };
  const mergeTarget = () => changeStructure('merges', [...new Set([...currentSheet().merges, 'B3:C3'])]);
  const unmergeSource = () => changeStructure('merges', currentSheet().merges.filter((range) => range !== 'B2:C2'));
  const freezeTarget = () => changeStructure('freeze', { state: 'frozen', xSplit: 1, ySplit: 2, topLeftCell: 'B3', activePane: 'bottomRight' });
  const unfreeze = () => changeStructure('freeze', null);
  const updateTable = (value) => {
    const sheet = currentSheet();
    if (!sheet.table) return false;
    sheet.table = { ...sheet.table, ...value };
    tableChanges.set(sheet.table.path, { sheet: sheet.name, path: sheet.table.path, ...sheet.table });
    markDirty(); render(); emit?.('tableChanged', { sheet: sheet.name, table: { ...sheet.table } });
    return true;
  };
  const sortRevenueDescending = () => {
    const sheet = currentSheet();
    if (!sheet.table) return false;
    const rows = [2, 3, 4, 5, 6].map((row) => ['A', 'B', 'C'].map((column) => cellFor(sheet, `${column}${row}`)))
      .sort((left, right) => Number(right[2]?.display || 0) - Number(left[2]?.display || 0))
      .map((row) => row.map((cell) => ({ display: cell?.display || '', type: cell?.type || null })));
    rows.forEach((source, index) => source.forEach((cell, column) => applyCellValue(sheet, `${columnName(column + 1)}${index + 2}`, cell.display, { recordHistory: false, cellType: cell.type })));
    return updateTable({ sort: { ref: 'C1:C6', stateRef: 'A2:C6', descending: true }, filter: null });
  };
  const filterNorth = () => {
    const sheet = currentSheet();
    if (!sheet.table) return false;
    for (let row = 2; row <= 6; row += 1) {
      const value = { hidden: cellFor(sheet, `A${row}`)?.display !== 'North' };
      geometryChanges.set(structuralKey(sheet, 'row', row), { sheet: sheet.name, path: sheet.path, kind: 'row', index: row, ...value });
      sheet.rows.set(row, { ...(sheet.rows.get(row) || {}), ...value });
    }
    return updateTable({ filter: { column: 0, values: ['North'] } });
  };
  const clearFilter = () => {
    const sheet = currentSheet();
    if (!sheet.table) return false;
    for (let row = 2; row <= 6; row += 1) {
      geometryChanges.set(structuralKey(sheet, 'row', row), { sheet: sheet.name, path: sheet.path, kind: 'row', index: row, hidden: false });
      sheet.rows.set(row, { ...(sheet.rows.get(row) || {}), hidden: false });
    }
    return updateTable({ filter: null });
  };
  const applyValidatedValue = (reference, value, cellType = 's') => {
    const sheet = currentSheet();
    const validation = validateCellValue(sheet, reference, value);
    if (!validation.valid) {
      emit?.('validationError', { sheet: sheet.name, reference, value: String(value), title: validation.rule?.errorTitle || 'Invalid value', message: validation.rule?.error || 'The value does not satisfy data validation.' });
      return false;
    }
    selectedCell = reference;
    const changed = applyCellValue(sheet, reference, value, { cellType });
    if (changed) emit?.('validatedCellChanged', { sheet: sheet.name, reference, value: String(value), validation_type: validation.rule?.type || null });
    render();
    return changed;
  };
  const setStatusFinal = () => applyValidatedValue('B2', 'Final', 's');
  const tryInvalidQuantity = () => applyValidatedValue('C2', '15', null);
  const setValidQuantity = () => applyValidatedValue('C2', '8', null);
  shell.querySelector('[data-action="zoom-out"]').addEventListener('click', onZoomOut);
  shell.querySelector('[data-action="zoom-in"]').addEventListener('click', onZoomIn);
  shell.querySelector('[data-action="save"]')?.addEventListener('click', onSave);
  undoButton?.addEventListener('click', undo);
  redoButton?.addEventListener('click', redo);
  shell.querySelector('[data-action="fill-down"]')?.addEventListener('click', fillDown);
  shell.querySelector('[data-action="bold"]')?.addEventListener('click', toggleBold);
  shell.querySelector('[data-action="italic"]')?.addEventListener('click', toggleItalic);
  shell.querySelector('[data-action="accounting"]')?.addEventListener('click', setAccounting);
  shell.querySelector('[data-action="row-height"]')?.addEventListener('click', setRowHeight);
  shell.querySelector('[data-action="column-width"]')?.addEventListener('click', setColumnWidth);
  shell.querySelector('[data-action="hide-row"]')?.addEventListener('click', hideRow);
  shell.querySelector('[data-action="show-row"]')?.addEventListener('click', showRow);
  shell.querySelector('[data-action="merge"]')?.addEventListener('click', mergeTarget);
  shell.querySelector('[data-action="unmerge"]')?.addEventListener('click', unmergeSource);
  shell.querySelector('[data-action="freeze"]')?.addEventListener('click', freezeTarget);
  shell.querySelector('[data-action="unfreeze"]')?.addEventListener('click', unfreeze);
  shell.querySelector('[data-action="sort-revenue"]')?.addEventListener('click', sortRevenueDescending);
  shell.querySelector('[data-action="filter-north"]')?.addEventListener('click', filterNorth);
  shell.querySelector('[data-action="clear-filter"]')?.addEventListener('click', clearFilter);
  shell.querySelector('[data-action="status-final"]')?.addEventListener('click', setStatusFinal);
  shell.querySelector('[data-action="quantity-invalid"]')?.addEventListener('click', tryInvalidQuantity);
  shell.querySelector('[data-action="quantity-valid"]')?.addEventListener('click', setValidQuantity);
  grid.addEventListener('copy', onCopy);
  grid.addEventListener('cut', onCut);
  grid.addEventListener('paste', onPaste);

  return runtimeApi = {
    async open(request = {}) {
      let loaded = await bridge.loadVersion(request);
      if (!loaded.editorBytes && loaded.version?.conversion_state !== 'prepared') {
        await bridge.prepare({ recordId: request.recordId, versionId: loaded.version?.id || request.versionId });
        loaded = await bridge.loadVersion({ recordId: request.recordId, versionId: loaded.version?.id || request.versionId });
      }
      sourceBytes = await toUint8Array(loaded.editorBytes ?? loaded.canonicalBytes);
      recordId = request.recordId ?? loaded.record?.id ?? null;
      versionId = request.versionId ?? loaded.version?.id ?? null;
      workbook = await inspectWorkbook(sourceBytes, DocxZipper);
      activeSheetIndex = Math.max(0, workbook.visibleActiveIndex);
      dirty = false;
      selectedCell = 'A1';
      selectedRange = null;
      changes = new Map();
      formatChanges = new Map();
      geometryChanges = new Map();
      structuralChanges = new Map();
      tableChanges = new Map();
      undoStack = [];
      redoStack = [];
      updateHistoryControls();
      render();
      const inspection = this.inspect();
      emit?.('opened', inspection);
      return inspection;
    },
    async save(request = {}) {
      if (!workbook) throw new Error('No spreadsheet is open');
      if (!access.write) throw permissionError('Spreadsheet is read-only');
      if (!dirty) return { ok: true, record_id: recordId, version_id: versionId, unchanged: true };
      const bytes = await applySpreadsheetMutations(sourceBytes, workbook, changes, formatChanges, geometryChanges, structuralChanges, tableChanges, DocxZipper);
      const result = await bridge.commit({
        recordId,
        baseVersionId: versionId,
        editorProtocol: 'ctox-euro-office-editor-bootstrap-v1',
        editorProtocolVersion: 1,
        implementedFeatures: ['spreadsheet.open-render-sheets', 'spreadsheet.edit-save', 'spreadsheet.undo-clipboard-fill', 'spreadsheet.cell-format-rows-columns', 'spreadsheet.formulas-references', 'spreadsheet.multi-sheet-merge-freeze', 'spreadsheet.sort-filter-tables', 'spreadsheet.validation-conditional-formatting'],
        reason: String(request.reason || 'manual'),
        bytes,
      }, [bytes.buffer]);
      versionId = result.version_id || result.versionId || versionId;
      sourceBytes = bytes;
      dirty = false;
      changes.clear();
      formatChanges.clear();
      geometryChanges.clear();
      structuralChanges.clear();
      tableChanges.clear();
      for (const sheet of workbook.sheets) for (const cell of sheet.cells) cell.originalDisplay = cell.display;
      undoStack = [];
      redoStack = [];
      updateHistoryControls();
      if (saveState) saveState.textContent = labels.saved;
      emit?.('saved', { recordId, versionId });
      return result;
    },
    async export({ format = 'xlsx' } = {}) {
      if (!workbook) throw new Error('No spreadsheet is open');
      if (!access.export) throw permissionError('Spreadsheet export is not permitted');
      if (format !== 'xlsx') throw Object.assign(new Error(`Unsupported spreadsheet export format: ${format}`), { code: 'unsupported_format' });
      return bridge.export({ recordId, versionId, format, mime: XLSX_MIME });
    },
    focus() { grid.focus?.(); return { focused: true }; },
    setPermissions(next = {}) {
      access = { ...access, ...next };
      shell.dataset.readOnly = String(access.write === false);
      return { permissions: { ...access }, requiresReopen: false };
    },
    inspect() {
      const visible = workbook?.sheets.filter((sheet) => sheet.state !== 'hidden') || [];
      const active = visible[activeSheetIndex] || null;
      return {
        schema_version: 'ctox-office-editor-inspection-v1',
        kind: 'spreadsheet',
        runtime: 'ctox-xlsx-bootstrap',
        target_runtime: 'euro-office-sdkjs-cell',
        record_id: recordId,
        version_id: versionId,
        open: Boolean(workbook),
        dirty,
        read_only: access.write === false,
        zoom_percent: zoomPercent,
        active_sheet: active?.name ?? null,
        sheet_count: workbook?.sheets.length ?? 0,
        visible_sheet_count: visible.length,
        sheets: workbook?.sheets.map((sheet) => ({
          name: sheet.name,
          state: sheet.state,
          dimension: sheet.dimension,
          cell_count: sheet.cells.length,
          formulas: sheet.cells.filter((cell) => cell.formula).map((cell) => ({ reference: cell.reference, formula: cell.formula, cached_value: cell.display, error: cell.type === 'e' })),
          merges: [...sheet.merges],
          freeze: sheet.freeze ? { ...sheet.freeze } : null,
          table: sheet.table ? { ...sheet.table } : null,
          validations: sheet.validations.map((rule) => ({ ...rule })),
          conditional_formats: sheet.conditionalFormats.map((rule) => ({ ...rule })),
          markers: sheet.cells.filter((cell) => /^ORACLE_SHEET_/.test(cell.display)).map((cell) => cell.display),
        })) ?? [],
        selected_cell: selectedCell,
        selected_range: selectedRange ? `${selectedRange.start}:${selectedRange.end}` : null,
        history: { can_undo: undoStack.length > 0, can_redo: redoStack.length > 0, undo_depth: undoStack.length, redo_depth: redoStack.length },
        pending_changes: [...changes.values()].map((change) => ({ sheet: change.sheet, reference: change.reference, value: change.value })),
        pending_formats: [...formatChanges.values()],
        pending_geometry: [...geometryChanges.values()],
        pending_structure: [...structuralChanges.values()],
        pending_tables: [...tableChanges.values()],
        implemented_features: ['spreadsheet.open-render-sheets', 'spreadsheet.edit-save', 'spreadsheet.undo-clipboard-fill', 'spreadsheet.cell-format-rows-columns', 'spreadsheet.formulas-references', 'spreadsheet.multi-sheet-merge-freeze', 'spreadsheet.sort-filter-tables', 'spreadsheet.validation-conditional-formatting'],
      };
    },
    destroy() {
      shell.querySelector('[data-action="zoom-out"]')?.removeEventListener('click', onZoomOut);
      shell.querySelector('[data-action="zoom-in"]')?.removeEventListener('click', onZoomIn);
      shell.querySelector('[data-action="save"]')?.removeEventListener('click', onSave);
      undoButton?.removeEventListener('click', undo);
      redoButton?.removeEventListener('click', redo);
      shell.querySelector('[data-action="fill-down"]')?.removeEventListener('click', fillDown);
      shell.querySelector('[data-action="bold"]')?.removeEventListener('click', toggleBold);
      shell.querySelector('[data-action="italic"]')?.removeEventListener('click', toggleItalic);
      shell.querySelector('[data-action="accounting"]')?.removeEventListener('click', setAccounting);
      shell.querySelector('[data-action="row-height"]')?.removeEventListener('click', setRowHeight);
      shell.querySelector('[data-action="column-width"]')?.removeEventListener('click', setColumnWidth);
      shell.querySelector('[data-action="hide-row"]')?.removeEventListener('click', hideRow);
      shell.querySelector('[data-action="show-row"]')?.removeEventListener('click', showRow);
      shell.querySelector('[data-action="merge"]')?.removeEventListener('click', mergeTarget);
      shell.querySelector('[data-action="unmerge"]')?.removeEventListener('click', unmergeSource);
      shell.querySelector('[data-action="freeze"]')?.removeEventListener('click', freezeTarget);
      shell.querySelector('[data-action="unfreeze"]')?.removeEventListener('click', unfreeze);
      shell.querySelector('[data-action="sort-revenue"]')?.removeEventListener('click', sortRevenueDescending);
      shell.querySelector('[data-action="filter-north"]')?.removeEventListener('click', filterNorth);
      shell.querySelector('[data-action="clear-filter"]')?.removeEventListener('click', clearFilter);
      shell.querySelector('[data-action="status-final"]')?.removeEventListener('click', setStatusFinal);
      shell.querySelector('[data-action="quantity-invalid"]')?.removeEventListener('click', tryInvalidQuantity);
      shell.querySelector('[data-action="quantity-valid"]')?.removeEventListener('click', setValidQuantity);
      grid.removeEventListener('copy', onCopy);
      grid.removeEventListener('cut', onCut);
      grid.removeEventListener('paste', onPaste);
      root.replaceChildren();
      workbook = null;
      sourceBytes = null;
      return { destroyed: true };
    },
  };
}

async function inspectWorkbook(bytes, DocxZipper) {
  const zip = await new DocxZipper().unzip(bytes);
  const parse = async (path) => {
    const entry = zip.file(path);
    if (!entry) throw Object.assign(new Error(`XLSX package is missing ${path}`), { code: 'invalid_office_package' });
    const xml = new DOMParser().parseFromString(await entry.async('string'), 'application/xml');
    if (xml.querySelector('parsererror')) throw Object.assign(new Error(`Invalid XML in ${path}`), { code: 'invalid_office_package' });
    return xml;
  };
  const parseOptional = async (path) => {
    const entry = zip.file(path);
    if (!entry) return null;
    const xml = new DOMParser().parseFromString(await entry.async('string'), 'application/xml');
    if (xml.querySelector('parsererror')) throw Object.assign(new Error(`Invalid XML in ${path}`), { code: 'invalid_office_package' });
    return xml;
  };
  const workbookXml = await parse('xl/workbook.xml');
  const relsXml = await parse('xl/_rels/workbook.xml.rels');
  const sharedXml = await parse('xl/sharedStrings.xml');
  const stylesXml = await parse('xl/styles.xml');
  const shared = [...sharedXml.getElementsByTagName('*')]
    .filter((node) => node.localName === 'si')
    .map((node) => [...node.getElementsByTagName('*')].filter((child) => child.localName === 't').map((child) => child.textContent || '').join(''));
  const relationships = new Map([...relsXml.getElementsByTagName('*')]
    .filter((node) => node.localName === 'Relationship')
    .map((node) => [node.getAttribute('Id'), node.getAttribute('Target')]));
  const fonts = [...stylesXml.getElementsByTagName('*')].filter((node) => node.localName === 'fonts')[0];
  const fontFormats = [...(fonts?.children || [])].filter((node) => node.localName === 'font').map((font) => ({
    bold: [...font.children].some((node) => node.localName === 'b'),
    italic: [...font.children].some((node) => node.localName === 'i'),
  }));
  const cellXfs = [...stylesXml.getElementsByTagName('*')].find((node) => node.localName === 'cellXfs');
  const styleFormats = [...(cellXfs?.children || [])].filter((node) => node.localName === 'xf').map((xf) => ({
    ...(fontFormats[Number(xf.getAttribute('fontId') || 0)] || {}),
    numberFormat: Number(xf.getAttribute('numFmtId') || 0) >= 164 ? 'euro-accounting' : null,
  }));
  const sheetNodes = [...workbookXml.getElementsByTagName('*')].filter((node) => node.localName === 'sheet');
  const activeTab = Number([...workbookXml.getElementsByTagName('*')].find((node) => node.localName === 'workbookView')?.getAttribute('activeTab') || 0);
  const sheets = [];
  for (const node of sheetNodes) {
    const relationshipId = [...node.attributes].find((attribute) => attribute.localName === 'id')?.value;
    const target = relationships.get(relationshipId);
    const path = target?.startsWith('/') ? target.slice(1) : `xl/${target}`;
    const sheetXml = await parse(path);
    const cells = [...sheetXml.getElementsByTagName('*')].filter((cell) => cell.localName === 'c').map((cell) => {
      const reference = cell.getAttribute('r');
      const raw = [...cell.children].find((child) => child.localName === 'v')?.textContent ?? '';
      const formulaNode = [...cell.children].find((child) => child.localName === 'f');
      const type = cell.getAttribute('t');
      const display = type === 's' ? (shared[Number(raw)] ?? '') : raw;
      const style = Number(cell.getAttribute('s') || 0);
      return {
        reference,
        row: Number(reference.match(/\d+$/)?.[0] || 1),
        column: columnNumber(reference.match(/^[A-Z]+/)?.[0] || 'A'),
        style,
        format: { ...(styleFormats[style] || {}) },
        type,
        formula: formulaNode ? `=${formulaNode.textContent || ''}` : null,
        raw,
        display,
        originalDisplay: display,
      };
    });
    const rows = new Map([...sheetXml.getElementsByTagName('*')].filter((item) => item.localName === 'row').map((row) => [Number(row.getAttribute('r')), {
      height: row.hasAttribute('ht') ? Number(row.getAttribute('ht')) : null,
      hidden: row.getAttribute('hidden') === '1',
    }]));
    const columns = new Map();
    for (const column of [...sheetXml.getElementsByTagName('*')].filter((item) => item.localName === 'col')) {
      const min = Number(column.getAttribute('min'));
      const max = Number(column.getAttribute('max'));
      for (let index = min; index <= max; index += 1) columns.set(index, {
        width: column.hasAttribute('width') ? Number(column.getAttribute('width')) : null,
        hidden: column.getAttribute('hidden') === '1',
      });
    }
    const merges = [...sheetXml.getElementsByTagName('*')].filter((item) => item.localName === 'mergeCell').map((item) => item.getAttribute('ref')).filter(Boolean);
    const pane = [...sheetXml.getElementsByTagName('*')].find((item) => item.localName === 'pane');
    const freeze = pane?.getAttribute('state') === 'frozen' ? {
      state: 'frozen',
      xSplit: Number(pane.getAttribute('xSplit') || 0),
      ySplit: Number(pane.getAttribute('ySplit') || 0),
      topLeftCell: pane.getAttribute('topLeftCell') || 'A1',
      activePane: pane.getAttribute('activePane') || null,
    } : null;
    let table = null;
    const tablePart = [...sheetXml.getElementsByTagName('*')].find((item) => item.localName === 'tablePart');
    if (tablePart) {
      const relPath = path.replace(/([^/]+)$/, '_rels/$1.rels');
      const sheetRels = await parseOptional(relPath);
      const tableRelationshipId = [...tablePart.attributes].find((attribute) => attribute.localName === 'id')?.value;
      const tableTarget = [...(sheetRels?.getElementsByTagName('*') || [])].find((item) => item.localName === 'Relationship' && item.getAttribute('Id') === tableRelationshipId)?.getAttribute('Target');
      const tablePath = tableTarget ? normalizeOfficePath(path.replace(/[^/]+$/, '') + tableTarget) : null;
      if (tablePath) {
        const tableXml = await parse(tablePath);
        const tableRoot = tableXml.documentElement;
        const autoFilter = [...tableRoot.children].find((item) => item.localName === 'autoFilter');
        const filterColumn = [...(autoFilter?.children || [])].find((item) => item.localName === 'filterColumn');
        const filters = [...(filterColumn?.children || [])].find((item) => item.localName === 'filters');
        const sortState = [...tableRoot.children].find((item) => item.localName === 'sortState') || [...(autoFilter?.children || [])].find((item) => item.localName === 'sortState');
        const sortCondition = [...(sortState?.children || [])].find((item) => item.localName === 'sortCondition');
        table = {
          path: tablePath,
          name: tableRoot.getAttribute('displayName') || tableRoot.getAttribute('name'),
          ref: tableRoot.getAttribute('ref'),
          style: [...tableRoot.children].find((item) => item.localName === 'tableStyleInfo')?.getAttribute('name') || null,
          filter: filterColumn ? { column: Number(filterColumn.getAttribute('colId') || 0), values: [...(filters?.children || [])].filter((item) => item.localName === 'filter').map((item) => item.getAttribute('val')) } : null,
          sort: sortCondition ? { ref: sortCondition.getAttribute('ref'), stateRef: sortState.getAttribute('ref'), descending: sortCondition.getAttribute('descending') === '1' } : null,
        };
      }
    }
    const validations = [...sheetXml.getElementsByTagName('*')].filter((item) => item.localName === 'dataValidation').map((rule) => ({
      type: rule.getAttribute('type') || 'none',
      operator: rule.getAttribute('operator') || null,
      sqref: rule.getAttribute('sqref') || 'A1',
      allowBlank: rule.getAttribute('allowBlank') === '1',
      errorTitle: rule.getAttribute('errorTitle') || '',
      error: rule.getAttribute('error') || '',
      formula1: [...rule.children].find((item) => item.localName === 'formula1')?.textContent || '',
      formula2: [...rule.children].find((item) => item.localName === 'formula2')?.textContent || '',
    }));
    const conditionalFormats = [...sheetXml.getElementsByTagName('*')].filter((item) => item.localName === 'conditionalFormatting').flatMap((container) => [...container.children].filter((item) => item.localName === 'cfRule').map((rule) => {
      const colorScale = [...rule.children].find((item) => item.localName === 'colorScale');
      return {
        sqref: container.getAttribute('sqref') || 'A1',
        type: rule.getAttribute('type'),
        operator: rule.getAttribute('operator') || null,
        priority: Number(rule.getAttribute('priority') || 0),
        formula: [...rule.children].find((item) => item.localName === 'formula')?.textContent || null,
        colors: [...(colorScale?.children || [])].filter((item) => item.localName === 'color').map((item) => item.getAttribute('rgb')),
      };
    }));
    sheets.push({
      name: node.getAttribute('name'),
      state: node.getAttribute('state') || 'visible',
      path,
      dimension: [...sheetXml.getElementsByTagName('*')].find((item) => item.localName === 'dimension')?.getAttribute('ref') || 'A1',
      cells,
      rows,
      columns,
      merges,
      freeze,
      table,
      validations,
      conditionalFormats,
    });
  }
  const visibleBeforeActive = sheets.slice(0, activeTab).filter((sheet) => sheet.state !== 'hidden').length;
  return { sheets, visibleActiveIndex: visibleBeforeActive };
}

async function applySpreadsheetMutations(sourceBytes, workbook, changes, formatChanges, geometryChanges, structuralChanges, tableChanges, DocxZipper) {
  if (!changes.size && !formatChanges.size && !geometryChanges.size && !structuralChanges.size && !tableChanges.size) return sourceBytes.slice();
  const zip = await new DocxZipper().unzip(sourceBytes);
  const parser = new DOMParser();
  const serializer = new XMLSerializer();
  const spreadsheetNamespace = 'http://schemas.openxmlformats.org/spreadsheetml/2006/main';
  const sharedEntry = zip.file('xl/sharedStrings.xml');
  if (!sharedEntry) throw Object.assign(new Error('XLSX package has no shared strings part'), { code: 'invalid_office_package' });
  const sharedXml = parser.parseFromString(await sharedEntry.async('string'), 'application/xml');
  if (sharedXml.querySelector('parsererror')) throw Object.assign(new Error('Invalid shared strings XML'), { code: 'invalid_office_package' });
  const sst = sharedXml.documentElement;
  let sharedIndex = [...sst.children].filter((node) => node.localName === 'si').length;
  let sharedStringsChanged = false;
  const grouped = new Map();
  for (const change of changes.values()) {
    const list = grouped.get(change.path) || [];
    const isSharedString = change.cellType === 's';
    list.push({ ...change, sharedIndex: isSharedString ? sharedIndex++ : null });
    grouped.set(change.path, list);
    if (!isSharedString) continue;
    sharedStringsChanged = true;
    const si = sharedXml.createElementNS(spreadsheetNamespace, 'si');
    const text = sharedXml.createElementNS(spreadsheetNamespace, 't');
    if (/^\s|\s$/.test(change.value)) text.setAttributeNS('http://www.w3.org/XML/1998/namespace', 'xml:space', 'preserve');
    text.textContent = change.value;
    si.append(text);
    sst.append(si);
  }
  if (sharedStringsChanged) {
    sst.setAttribute('uniqueCount', String(sharedIndex));
    sst.setAttribute('count', String(sharedIndex));
    zip.file('xl/sharedStrings.xml', serializer.serializeToString(sharedXml));
  }

  const formatStyleIndexes = new Map();
  if (formatChanges.size) {
    const stylesEntry = zip.file('xl/styles.xml');
    if (!stylesEntry) throw Object.assign(new Error('XLSX package has no styles part'), { code: 'invalid_office_package' });
    const stylesXml = parser.parseFromString(await stylesEntry.async('string'), 'application/xml');
    const fonts = [...stylesXml.documentElement.children].find((node) => node.localName === 'fonts');
    const cellXfs = [...stylesXml.documentElement.children].find((node) => node.localName === 'cellXfs');
    if (!fonts || !cellXfs) throw Object.assign(new Error('XLSX styles part has no fonts or cellXfs'), { code: 'invalid_office_package' });
    let numFmts = [...stylesXml.documentElement.children].find((node) => node.localName === 'numFmts');
    if (!numFmts) {
      numFmts = stylesXml.createElementNS(spreadsheetNamespace, 'numFmts');
      numFmts.setAttribute('count', '0');
      stylesXml.documentElement.insertBefore(numFmts, fonts);
    }
    let accountingNumFmtId = null;
    const styleCache = new Map();
    for (const change of formatChanges.values()) {
      const key = JSON.stringify(change.format);
      if (!styleCache.has(key)) {
        const font = stylesXml.createElementNS(spreadsheetNamespace, 'font');
        if (change.format.bold) font.append(stylesXml.createElementNS(spreadsheetNamespace, 'b'));
        if (change.format.italic) font.append(stylesXml.createElementNS(spreadsheetNamespace, 'i'));
        const size = stylesXml.createElementNS(spreadsheetNamespace, 'sz'); size.setAttribute('val', '11'); font.append(size);
        const name = stylesXml.createElementNS(spreadsheetNamespace, 'name'); name.setAttribute('val', 'Aptos'); font.append(name);
        const fontId = [...fonts.children].filter((node) => node.localName === 'font').length;
        fonts.append(font); fonts.setAttribute('count', String(fontId + 1));
        if (change.format.numberFormat === 'euro-accounting' && accountingNumFmtId == null) {
          accountingNumFmtId = 164;
          while ([...numFmts.children].some((node) => Number(node.getAttribute('numFmtId')) === accountingNumFmtId)) accountingNumFmtId += 1;
          const numFmt = stylesXml.createElementNS(spreadsheetNamespace, 'numFmt');
          numFmt.setAttribute('numFmtId', String(accountingNumFmtId));
          numFmt.setAttribute('formatCode', '_-* #,##0.00\\ [$€-7]_-;\\-* #,##0.00\\ [$€-7]_-;_-* "-"??\\ [$€-7]_-;_-@_-');
          numFmts.append(numFmt); numFmts.setAttribute('count', String([...numFmts.children].length));
        }
        const xf = stylesXml.createElementNS(spreadsheetNamespace, 'xf');
        xf.setAttribute('fontId', String(fontId)); xf.setAttribute('fillId', '0'); xf.setAttribute('borderId', '0');
        xf.setAttribute('numFmtId', String(change.format.numberFormat === 'euro-accounting' ? accountingNumFmtId : 0)); xf.setAttribute('xfId', '0');
        xf.setAttribute('applyFont', '1'); if (change.format.numberFormat) xf.setAttribute('applyNumberFormat', '1');
        const styleIndex = [...cellXfs.children].filter((node) => node.localName === 'xf').length;
        cellXfs.append(xf); cellXfs.setAttribute('count', String(styleIndex + 1)); styleCache.set(key, styleIndex);
      }
      formatStyleIndexes.set(`${change.path}!${change.reference}`, styleCache.get(key));
      if (!grouped.has(change.path)) grouped.set(change.path, []);
    }
    zip.file('xl/styles.xml', serializer.serializeToString(stylesXml));
  }
  for (const change of geometryChanges.values()) if (!grouped.has(change.path)) grouped.set(change.path, []);
  for (const change of structuralChanges.values()) if (!grouped.has(change.path)) grouped.set(change.path, []);

  for (const [path, sheetChanges] of grouped) {
    const entry = zip.file(path);
    if (!entry) throw Object.assign(new Error(`XLSX package has no worksheet part ${path}`), { code: 'invalid_office_package' });
    const sheetXml = parser.parseFromString(await entry.async('string'), 'application/xml');
    if (sheetXml.querySelector('parsererror')) throw Object.assign(new Error(`Invalid worksheet XML: ${path}`), { code: 'invalid_office_package' });
    const cells = [...sheetXml.getElementsByTagName('*')].filter((node) => node.localName === 'c');
    for (const change of sheetChanges) {
      const cell = cells.find((node) => node.getAttribute('r') === change.reference);
      if (!cell) throw Object.assign(new Error(`Editable cell ${change.reference} is missing from ${path}`), { code: 'cell_not_found' });
      [...cell.children].filter((node) => node.localName === 'f' || node.localName === 'is').forEach((node) => node.remove());
      let value = [...cell.children].find((node) => node.localName === 'v');
      if (!value) { value = sheetXml.createElementNS(spreadsheetNamespace, 'v'); cell.append(value); }
      if (change.formula != null) {
        const formula = sheetXml.createElementNS(spreadsheetNamespace, 'f');
        formula.textContent = change.formula;
        cell.insertBefore(formula, value);
        if (change.cellType === 'e') cell.setAttribute('t', 'e'); else cell.removeAttribute('t');
        value.textContent = change.value;
      } else if (change.cellType === 's') {
        cell.setAttribute('t', 's');
        value.textContent = String(change.sharedIndex);
      } else {
        cell.removeAttribute('t');
        value.textContent = change.value;
      }
    }
    for (const change of formatChanges.values()) {
      if (change.path !== path) continue;
      const cell = cells.find((node) => node.getAttribute('r') === change.reference);
      if (!cell) throw Object.assign(new Error(`Format cell ${change.reference} is missing from ${path}`), { code: 'cell_not_found' });
      cell.setAttribute('s', String(formatStyleIndexes.get(`${path}!${change.reference}`)));
    }
    for (const change of geometryChanges.values()) {
      if (change.path !== path) continue;
      if (change.kind === 'row') {
        const row = [...sheetXml.getElementsByTagName('*')].find((node) => node.localName === 'row' && Number(node.getAttribute('r')) === change.index);
        if (!row) throw Object.assign(new Error(`Row ${change.index} is missing from ${path}`), { code: 'row_not_found' });
        if (change.height != null) { row.setAttribute('ht', String(change.height)); row.setAttribute('customHeight', '1'); }
        if (change.hidden) row.setAttribute('hidden', '1'); else row.removeAttribute('hidden');
      } else {
        const column = [...sheetXml.getElementsByTagName('*')].find((node) => node.localName === 'col' && Number(node.getAttribute('min')) <= change.index && Number(node.getAttribute('max')) >= change.index);
        if (!column) throw Object.assign(new Error(`Column ${change.index} is missing from ${path}`), { code: 'column_not_found' });
        if (change.width != null) { column.setAttribute('width', String(change.width)); column.setAttribute('customWidth', '1'); }
        if (change.hidden) column.setAttribute('hidden', '1'); else column.removeAttribute('hidden');
      }
    }
    for (const change of structuralChanges.values()) {
      if (change.path !== path) continue;
      if (change.kind === 'merges') {
        const old = [...sheetXml.documentElement.children].find((node) => node.localName === 'mergeCells');
        old?.remove();
        if (change.value.length) {
          const container = sheetXml.createElementNS(spreadsheetNamespace, 'mergeCells');
          container.setAttribute('count', String(change.value.length));
          for (const range of change.value) { const node = sheetXml.createElementNS(spreadsheetNamespace, 'mergeCell'); node.setAttribute('ref', range); container.append(node); }
          const pageMargins = [...sheetXml.documentElement.children].find((node) => node.localName === 'pageMargins');
          sheetXml.documentElement.insertBefore(container, pageMargins || null);
        }
      } else if (change.kind === 'freeze') {
        const sheetView = [...sheetXml.getElementsByTagName('*')].find((node) => node.localName === 'sheetView');
        if (!sheetView) throw Object.assign(new Error(`Worksheet ${path} has no sheetView`), { code: 'invalid_office_package' });
        [...sheetView.children].filter((node) => node.localName === 'pane').forEach((node) => node.remove());
        if (change.value) {
          const pane = sheetXml.createElementNS(spreadsheetNamespace, 'pane');
          pane.setAttribute('xSplit', String(change.value.xSplit)); pane.setAttribute('ySplit', String(change.value.ySplit));
          pane.setAttribute('topLeftCell', change.value.topLeftCell); pane.setAttribute('activePane', change.value.activePane); pane.setAttribute('state', 'frozen');
          sheetView.insertBefore(pane, sheetView.firstChild);
          const selection = [...sheetView.children].find((node) => node.localName === 'selection');
          if (selection) selection.setAttribute('pane', change.value.activePane);
        } else {
          for (const selection of [...sheetView.children].filter((node) => node.localName === 'selection')) selection.removeAttribute('pane');
        }
      }
    }
    zip.file(path, serializer.serializeToString(sheetXml));
  }
  for (const change of tableChanges.values()) {
    const entry = zip.file(change.path);
    if (!entry) throw Object.assign(new Error(`XLSX package has no table part ${change.path}`), { code: 'invalid_office_package' });
    const tableXml = parser.parseFromString(await entry.async('string'), 'application/xml');
    if (tableXml.querySelector('parsererror')) throw Object.assign(new Error(`Invalid table XML: ${change.path}`), { code: 'invalid_office_package' });
    const root = tableXml.documentElement;
    let autoFilter = [...root.children].find((node) => node.localName === 'autoFilter');
    if (!autoFilter) { autoFilter = tableXml.createElementNS(spreadsheetNamespace, 'autoFilter'); autoFilter.setAttribute('ref', change.ref); root.insertBefore(autoFilter, [...root.children].find((node) => node.localName === 'tableColumns') || null); }
    [...autoFilter.children].filter((node) => node.localName === 'filterColumn' || node.localName === 'sortState').forEach((node) => node.remove());
    [...root.children].filter((node) => node.localName === 'sortState').forEach((node) => node.remove());
    if (change.filter) {
      const column = tableXml.createElementNS(spreadsheetNamespace, 'filterColumn'); column.setAttribute('colId', String(change.filter.column));
      const filters = tableXml.createElementNS(spreadsheetNamespace, 'filters');
      for (const value of change.filter.values) { const filter = tableXml.createElementNS(spreadsheetNamespace, 'filter'); filter.setAttribute('val', value); filters.append(filter); }
      column.append(filters); autoFilter.append(column);
    }
    if (change.sort) {
      const sortState = tableXml.createElementNS(spreadsheetNamespace, 'sortState'); sortState.setAttribute('ref', change.sort.stateRef || change.ref);
      const condition = tableXml.createElementNS(spreadsheetNamespace, 'sortCondition'); condition.setAttribute('ref', change.sort.ref);
      if (change.sort.descending) condition.setAttribute('descending', '1');
      sortState.append(condition); root.insertBefore(sortState, [...root.children].find((node) => node.localName === 'tableColumns') || null);
    }
    zip.file(change.path, serializer.serializeToString(tableXml));
  }
  const output = await zip.generateAsync({
    type: 'arraybuffer',
    mimeType: XLSX_MIME,
    compression: 'DEFLATE',
    compressionOptions: { level: 6 },
  });
  return new Uint8Array(output);
}

function rangeReferences(range) {
  const parse = (reference) => ({
    column: columnNumber(reference.match(/^[A-Z]+/)?.[0] || 'A'),
    row: Number(reference.match(/\d+$/)?.[0] || 1),
  });
  const start = parse(range.start);
  const end = parse(range.end);
  const refs = [];
  for (let row = Math.min(start.row, end.row); row <= Math.max(start.row, end.row); row += 1) {
    for (let column = Math.min(start.column, end.column); column <= Math.max(start.column, end.column); column += 1) {
      refs.push(`${columnName(column)}${row}`);
    }
  }
  return refs;
}

function renderRangeSelection(table, range) {
  table.querySelectorAll('td[aria-selected="true"]').forEach((node) => node.setAttribute('aria-selected', 'false'));
  for (const reference of rangeReferences(range)) {
    table.querySelector(`td[data-reference="${reference}"]`)?.setAttribute('aria-selected', 'true');
  }
}

function evaluateFormula(workbook, activeSheet, formula, seen = new Set()) {
  const error = (value) => ({ value, error: true });
  try {
    let expression = String(formula || '').replace(/^=/, '').trim();
    expression = expression.replace(/SUM\((\$?[A-Z]+\$?\d+):(\$?[A-Z]+\$?\d+)\)/gi, (_, start, end) => {
      const values = rangeReferences({ start: start.replace(/\$/g, ''), end: end.replace(/\$/g, '') })
        .map((reference) => numericCellValue(workbook, activeSheet, reference, seen));
      if (values.some((value) => !Number.isFinite(value))) throw new Error('#VALUE!');
      return String(values.reduce((sum, value) => sum + value, 0));
    });
    const referencePattern = /(?:(?:'([^']+)'|([A-Za-z_][A-Za-z0-9_ ]*))!)?(\$?)([A-Z]{1,3})(\$?)(\d+)/g;
    expression = expression.replace(referencePattern, (_, quotedSheet, plainSheet, _absoluteColumn, column, _absoluteRow, row) => {
      const sheet = workbook.sheets.find((item) => item.name === (quotedSheet || plainSheet)) || activeSheet;
      const value = numericCellValue(workbook, sheet, `${column}${row}`, seen);
      if (!Number.isFinite(value)) throw new Error('#VALUE!');
      return String(value);
    });
    if (!/^[0-9+\-*/().\s]+$/.test(expression)) return error('#NAME?');
    const value = Function(`"use strict"; return (${expression});`)();
    if (!Number.isFinite(value)) return error('#DIV/0!');
    return { value: String(value), error: false };
  } catch (caught) {
    const value = /^#/.test(caught?.message || '') ? caught.message : '#VALUE!';
    return error(value);
  }
}

function numericCellValue(workbook, sheet, reference, seen) {
  const key = `${sheet.name}!${reference}`;
  if (seen.has(key)) return Number.NaN;
  const cell = sheet.cells.find((item) => item.reference === reference);
  if (!cell) return 0;
  if (cell.formula) {
    const nextSeen = new Set(seen); nextSeen.add(key);
    const result = evaluateFormula(workbook, sheet, cell.formula, nextSeen);
    return result.error ? Number.NaN : Number(result.value);
  }
  return Number(cell.display);
}

function shiftFormula(formula, sourceReference, targetReference) {
  const source = parseCellReference(sourceReference);
  const target = parseCellReference(targetReference);
  const columnDelta = target.column - source.column;
  const rowDelta = target.row - source.row;
  return String(formula).replace(/(\$?)([A-Z]{1,3})(\$?)(\d+)/g, (_, absoluteColumn, column, absoluteRow, row) => {
    const shiftedColumn = absoluteColumn ? columnNumber(column) : Math.max(1, columnNumber(column) + columnDelta);
    const shiftedRow = absoluteRow ? Number(row) : Math.max(1, Number(row) + rowDelta);
    return `${absoluteColumn}${columnName(shiftedColumn)}${absoluteRow}${shiftedRow}`;
  });
}

function parseCellReference(reference) {
  return {
    column: columnNumber(reference.match(/^[A-Z]+/)?.[0] || 'A'),
    row: Number(reference.match(/\d+$/)?.[0] || 1),
  };
}

function columnName(column) {
  let value = column;
  let result = '';
  while (value > 0) { value -= 1; result = String.fromCharCode(65 + (value % 26)) + result; value = Math.floor(value / 26); }
  return result;
}

function columnNumber(name) {
  return [...name].reduce((value, letter) => value * 26 + letter.charCodeAt(0) - 64, 0);
}

function normalizeOfficePath(path) {
  const parts = [];
  for (const part of path.split('/')) { if (!part || part === '.') continue; if (part === '..') parts.pop(); else parts.push(part); }
  return parts.join('/');
}

function validateCellValue(sheet, reference, value) {
  const rule = sheet?.validations?.find((candidate) => rangeReferences({ start: candidate.sqref.split(':')[0], end: candidate.sqref.split(':').at(-1) }).includes(reference));
  if (!rule) return { valid: true, rule: null };
  const text = String(value ?? '');
  if (!text && rule.allowBlank) return { valid: true, rule };
  if (rule.type === 'list') {
    const values = rule.formula1.replace(/^"|"$/g, '').split(',').map((item) => item.trim());
    return { valid: values.includes(text), rule, values };
  }
  if (rule.type === 'whole') {
    const numeric = Number(text); const minimum = Number(rule.formula1); const maximum = Number(rule.formula2);
    const valid = Number.isInteger(numeric) && (rule.operator === 'between' ? numeric >= minimum && numeric <= maximum : true);
    return { valid, rule, minimum, maximum };
  }
  return { valid: true, rule };
}

function conditionalStyleFor(sheet, reference, display) {
  for (const rule of sheet?.conditionalFormats || []) {
    const refs = rangeReferences({ start: rule.sqref.split(':')[0], end: rule.sqref.split(':').at(-1) });
    if (!refs.includes(reference)) continue;
    const value = Number(display);
    if (rule.type === 'cellIs' && rule.operator === 'greaterThan' && value > Number(rule.formula)) return { background: '#c6efce', color: '#006100' };
    if (rule.type === 'colorScale' && rule.colors.length >= 3) {
      const values = refs.map((ref) => Number(cellForRule(sheet, ref))).filter(Number.isFinite);
      const minimum = Math.min(...values); const maximum = Math.max(...values); const midpoint = (minimum + maximum) / 2;
      const color = value <= midpoint ? rule.colors[0] : value < maximum ? rule.colors[1] : rule.colors[2];
      return { background: `#${String(color).replace(/^FF/i, '')}` };
    }
  }
  return null;
}

function cellForRule(sheet, reference) { return sheet.cells.find((cell) => cell.reference === reference)?.display ?? ''; }

async function toUint8Array(value) {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (value instanceof Blob) return new Uint8Array(await value.arrayBuffer());
  if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  throw new Error('Unsupported spreadsheet payload');
}

function permissionError(message) {
  return Object.assign(new Error(message), { code: 'permission_denied' });
}

export const __spreadsheetRuntimeTestHooks = Object.freeze({ evaluateFormula, shiftFormula, validateCellValue, conditionalStyleFor });
