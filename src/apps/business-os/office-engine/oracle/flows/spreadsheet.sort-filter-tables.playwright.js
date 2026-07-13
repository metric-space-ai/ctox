async (page) => {
  const feature = 'spreadsheet.sort-filter-tables';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  await page.setViewportSize({ width: 1600, height: 900 });
  let oracle;
  let ctox;
  let ctoxHost;
  const deadline = Date.now() + 90000;
  while (Date.now() < deadline) {
    oracle = page.frames().find((frame) => frame.url().includes('127.0.0.1:8088/') && frame.url().includes('/spreadsheeteditor/main/index.html'));
    ctox = page.frames().find((frame) => frame.url().includes('/vendor/ctox-office/upstream/') && frame.url().includes('/spreadsheeteditor/main/index.html'));
    ctoxHost = page.frames().find((frame) => frame.url().includes('/ctox-spreadsheet-sort-filter-tables.html'));
    if (oracle && ctox && ctoxHost && await oracle.locator('#ce-cell-name').count() === 1 && await ctox.locator('#ce-cell-name').count() === 1) break;
    await page.waitForTimeout(500);
  }
  if (!oracle || !ctox || !ctoxHost) throw new Error('sort/filter comparison frames are missing');
  await page.waitForFunction(() => window.ctoxOfficeComparison?.lastValidation?.valid === true, null, { timeout: 30000 });
  const gate = await page.evaluate(() => window.ctoxOfficeComparison.validate());
  if (!gate.valid) throw new Error(`comparison gate failed: ${gate.failures.join(', ')}`);

  const select = async (frame, reference) => {
    const name = frame.locator('#ce-cell-name');
    await name.fill(reference);
    await name.press('Enter');
    await page.waitForTimeout(150);
  };
  const value = async (frame, reference) => {
    await select(frame, reference);
    return frame.locator('#ce-cell-content').inputValue();
  };
  const values = async (frame, references) => {
    const result = [];
    for (const reference of references) result.push(await value(frame, reference));
    return result;
  };
  const openHeaderFilter = async (frame, reference) => {
    await select(frame, reference);
    const cell = await frame.evaluate(() => {
      const rect = Asc.editor.asc_getActiveCellCoord();
      return { x: rect.asc_getX(), y: rect.asc_getY(), width: rect.asc_getWidth(), height: rect.asc_getHeight() };
    });
    // Derived from the measured active-cell rectangle; no absolute screen coordinate.
    await frame.locator('#ws-canvas-overlay').click({
      position: { x: cell.x + cell.width - 8, y: cell.y + cell.height / 2 },
      force: true,
      timeout: 5000,
    });
    await frame.getByRole('dialog').waitFor({ state: 'visible', timeout: 5000 });
    return cell;
  };

  const terminal = {};
  for (const [name, frame] of [['oracle', oracle], ['ctox', ctox]]) {
    const tabs = await frame.getByRole('tab').allInnerTexts();
    if (!tabs.includes('Tabellen-Design')) throw new Error(`${name} did not materialize RevenueTable in CTOX Spreadsheets`);
    const initial = await values(frame, ['C2', 'C3', 'C4', 'C5', 'C6']);
    if (initial.join(',') !== '120,420,240,310,180') throw new Error(`${name} initial revenue order differs: ${initial}`);

    const revenueHeader = await openHeaderFilter(frame, 'C1');
    await frame.getByRole('listitem').filter({ hasText: 'Absteigend sortieren' }).click();
    await page.waitForTimeout(450);
    const sorted = await values(frame, ['C2', 'C3', 'C4', 'C5', 'C6']);
    if (sorted.join(',') !== '420,310,240,180,120') throw new Error(`${name} descending revenue sort failed: ${sorted}`);

    const regionHeader = await openHeaderFilter(frame, 'A1');
    const dialog = frame.getByRole('dialog');
    const treeitems = dialog.getByRole('treeitem');
    await treeitems.first().click();
    await treeitems.filter({ hasText: 'North' }).click();
    await dialog.getByRole('button', { name: 'OK', exact: true }).click();
    await page.waitForTimeout(450);
    terminal[name] = {
      initial,
      sorted,
      visibleRows: ['North|Consulting|420', 'North|Support|310'],
      measuredHeaders: { revenue: revenueHeader, region: regionHeader },
    };
  }
  await page.screenshot({ path: `${output}/differential-terminal-before-save.png` });
  for (const frame of [oracle, ctox]) await frame.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.state?.commits?.length === 1, null, { timeout: 30000 });
  await page.waitForTimeout(5000);
  await page.evaluate(() => { document.querySelector('#oracle').src = 'about:blank'; });
  await page.waitForFunction(async () => {
    const response = await fetch(`http://127.0.0.1:4180/state/spreadsheet.sort-filter-tables?t=${Date.now()}`, { cache: 'no-store' });
    return response.ok && (await response.json()).saved === true;
  }, null, { timeout: 30000 });
  const capture = await ctoxHost.evaluate(async () => {
    const bytes = await window.ctoxOfficeEvidence.savedBytes();
    return (await fetch('http://127.0.0.1:4180/capture-ctox/spreadsheet.sort-filter-tables', { method: 'POST', body: bytes })).json();
  });
  const ctoxEvidence = await ctoxHost.evaluate(async () => ({ inspection: await window.ctoxOfficeEvidence.editor.inspect(), commits: window.ctoxOfficeEvidence.state.commits }));
  const oracleState = await page.evaluate(async () => (await fetch(`http://127.0.0.1:4180/state/spreadsheet.sort-filter-tables?t=${Date.now()}`, { cache: 'no-store' })).json());
  if (ctoxEvidence.inspection.dirty || !oracleState.saved) throw new Error('sort/filter save evidence is incomplete');
  return { feature_id: feature, interaction: 'original-table-filter-dialog-sort-and-value-tree', gate, terminal, capture, oracle: oracleState, ctox: ctoxEvidence };
}
