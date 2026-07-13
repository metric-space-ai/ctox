async (page) => {
  const feature = 'spreadsheet.cell-format-rows-columns';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  await page.setViewportSize({ width: 1600, height: 900 });
  let oracle;
  let ctox;
  let ctoxHost;
  const readyDeadline = Date.now() + 90000;
  while (Date.now() < readyDeadline) {
    const frames = page.frames();
    oracle = frames.find((frame) => frame.url().includes('127.0.0.1:8088/')
      && frame.url().includes('/web-apps/apps/spreadsheeteditor/main/index.html'));
    ctox = frames.find((frame) => frame.url().includes('/vendor/ctox-office/upstream/web-apps/apps/spreadsheeteditor/main/index.html'));
    ctoxHost = frames.find((frame) => frame.url().includes('/ctox-spreadsheet-cell-format-rows-columns.html'));
    if (oracle && ctox && ctoxHost
      && await oracle.locator('#ce-cell-name').count() === 1
      && await ctox.locator('#ce-cell-name').count() === 1) break;
    await page.waitForTimeout(500);
  }
  if (!oracle || !ctox || !ctoxHost) throw new Error('spreadsheet formatting frames are missing');
  await page.waitForFunction(() => {
    const frames = window.ctoxOfficeComparison?.frames;
    return frames?.oracle?.contentDocument?.querySelector('[role="status"]')?.textContent === 'document-ready'
      && frames?.ctox?.contentDocument?.querySelector('[role="status"]')?.textContent === 'document-ready';
  }, null, { timeout: 30000 });
  const gate = await page.evaluate(() => window.ctoxOfficeComparison.validate());
  if (!gate.valid) throw new Error(`comparison gate failed: ${gate.failures.join(', ')}`);

  const select = async (frame, reference) => {
    const name = frame.locator('#ce-cell-name');
    await name.fill(reference);
    await name.press('Enter');
    await page.waitForTimeout(120);
  };
  const expandedToolbar = (frame) => frame.getByRole('button').filter({ hasText: 'Format' });
  const openFormatMenu = async (frame) => {
    if (!await expandedToolbar(frame).isVisible()) {
      await frame.getByRole('button', { name: 'Mehr', exact: true }).click();
    }
    await expandedToolbar(frame).click();
  };
  const setCustomSize = async (frame, axis, value) => {
    await openFormatMenu(frame);
    await frame.getByText(axis === 'row' ? 'Zeilenhöhe' : 'Spaltenbreite', { exact: true }).click();
    await frame.getByText(axis === 'row' ? 'Benutzerdefinierte Zeilenhöhe' : 'Benutzerdefinierte Spaltenbreite', { exact: true }).click();
    const dialog = frame.getByRole('dialog');
    await dialog.getByRole('spinbutton').fill(String(value));
    await dialog.getByRole('button', { name: 'OK', exact: true }).click();
  };
  const rowVisibility = async (frame, action) => {
    await openFormatMenu(frame);
    const command = frame.getByText(action === 'hide' ? 'Ausblenden' : 'Anzeigen', { exact: true });
    const expectedCommands = action === 'hide' ? 1 : 2;
    if (await command.count() !== expectedCommands) throw new Error(`row visibility command is incomplete: ${action}`);
    await command.nth(0).click();
    const rows = frame.getByText('Zeilen', { exact: true });
    if (await rows.count() !== 3) throw new Error(`row submenu is incomplete: ${action}`);
    await rows.nth(action === 'hide' ? 0 : 1).click();
  };
  const semantic = async (frame) => {
    const state = {};
    for (const reference of ['A1', 'A2', 'B4', 'A4', 'B3', 'A5']) {
      await select(frame, reference);
      state[reference] = await frame.evaluate(() => {
        const api = window.editor || window.Asc?.editor;
        const xfs = api.asc_getCellInfo().asc_getXfs();
        return { reference: document.querySelector('#ce-cell-name')?.value, bold: xfs.asc_getFontBold(), italic: xfs.asc_getFontItalic(), number_format: xfs.asc_getNumFormat(), row_height: api.asc_getRowHeight(), column_width: api.asc_getColumnWidth() };
      });
    }
    return state;
  };

  for (const [name, frame] of [['oracle', oracle], ['ctox', ctox]]) {
    await select(frame, 'A2');
    const bold = frame.getByRole('button', { name: 'Fett (⌘+B)', exact: true });
    const italic = frame.getByRole('button', { name: 'Kursiv (⌘+I)', exact: true });
    if (await bold.getAttribute('aria-pressed') !== 'true') await bold.click();
    if (await italic.getAttribute('aria-pressed') !== 'true') await italic.click();

    await select(frame, 'B4');
    if (!await expandedToolbar(frame).isVisible()) await frame.getByRole('button', { name: 'Mehr', exact: true }).click();
    await frame.getByRole('button', { name: 'Buchhaltungsformat', exact: true }).nth(1).click();
    await frame.getByText('€ Euro', { exact: true }).click();

    await select(frame, 'A4');
    await setCustomSize(frame, 'row', 27.75);
    await select(frame, 'B3');
    await setCustomSize(frame, 'column', 32.625);

    await select(frame, 'A5');
    await rowVisibility(frame, 'hide');
    await select(frame, 'A4:A6');
    await rowVisibility(frame, 'show');
    await select(frame, 'A5');
    const state = await semantic(frame);
    if (!state.A1.bold || state.A1.italic) throw new Error(`${name} A1 header style changed unintentionally`);
    if (!state.A2.bold || !state.A2.italic) throw new Error(`${name} A2 bold/italic mismatch`);
    if (!String(state.B4.number_format).includes('€')) throw new Error(`${name} B4 Euro accounting mismatch: ${state.B4.number_format}`);
    if (Math.abs(state.A4.row_height - 27.75) > 0.01) throw new Error(`${name} row height mismatch: ${state.A4.row_height}`);
    if (Math.abs(state.B3.column_width - 32.63) > 0.02) throw new Error(`${name} column width mismatch: ${state.B3.column_width}`);
  }
  const terminal = { oracle: await semantic(oracle), ctox: await semantic(ctox) };
  await page.screenshot({ path: `${output}/differential-terminal-before-save.png` });

  for (const frame of [oracle, ctox]) {
    await frame.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  }
  await page.waitForFunction(() =>
    document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.state?.commits?.length === 1,
  null, { timeout: 30000 });
  await page.waitForTimeout(2200);
  await page.evaluate(() => { document.querySelector('#oracle').src = 'about:blank'; });
  await page.waitForFunction(async () => {
    const response = await fetch(`http://127.0.0.1:4180/state/spreadsheet.cell-format-rows-columns?t=${Date.now()}`, { cache: 'no-store' });
    return response.ok && (await response.json()).saved === true;
  }, null, { timeout: 30000 });

  const capture = await ctoxHost.evaluate(async () => {
    const bytes = await window.ctoxOfficeEvidence.savedBytes();
    const response = await fetch('http://127.0.0.1:4180/capture-ctox/spreadsheet.cell-format-rows-columns', { method: 'POST', body: bytes });
    return response.json();
  });
  const ctoxEvidence = await ctoxHost.evaluate(async () => ({
    inspection: await window.ctoxOfficeEvidence.editor.inspect(),
    commits: window.ctoxOfficeEvidence.state.commits,
  }));
  const oracleState = await page.evaluate(async () =>
    (await fetch(`http://127.0.0.1:4180/state/spreadsheet.cell-format-rows-columns?t=${Date.now()}`, { cache: 'no-store' })).json());
  if (ctoxEvidence.inspection.dirty || !oracleState.saved) throw new Error('formatting save evidence is incomplete');
  return { feature_id: feature, interaction: 'original-toolbar-cell-format-row-column-and-visibility', gate, terminal, capture, oracle: oracleState, ctox: ctoxEvidence };
}
