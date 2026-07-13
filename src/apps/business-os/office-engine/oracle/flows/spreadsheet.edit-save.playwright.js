async (page) => {
  const feature = 'spreadsheet.edit-save';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  const replacement = 'CTOX_EDIT_CELL_BRAVO_42';
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.waitForFunction(() => document.body.innerText.includes('Gate: Lauf gültig'), null, { timeout: 60000 });
  const gate = await page.evaluate(() => window.ctoxOfficeComparison.validate());
  if (!gate.valid) throw new Error(`comparison gate failed: ${gate.failures.join(', ')}`);

  const frames = page.frames();
  const oracle = frames.find((frame) =>
    frame.url().includes('127.0.0.1:8088/')
    && frame.url().includes('/web-apps/apps/spreadsheeteditor/main/index.html'));
  const ctox = frames.find((frame) =>
    frame.url().includes('/vendor/ctox-office/upstream/web-apps/apps/spreadsheeteditor/main/index.html'));
  const ctoxHost = frames.find((frame) => frame.url().includes('/ctox-spreadsheet-edit-save.html'));
  if (!oracle || !ctox || !ctoxHost) throw new Error('spreadsheet edit/save frames are missing');

  const state = (frame) => frame.evaluate(() => {
    const api = window.editor || window.Asc?.editor;
    return {
      modified: api.isDocumentModified?.() === true,
      can_save: api.asc_isDocumentCanSave?.() === true,
      active_sheet: api.asc_getWorksheetName(api.asc_getActiveWorksheetIndex()),
      cell_name: document.querySelector('#ce-cell-name')?.value,
      cell_value: document.querySelector('#ce-cell-content')?.value,
    };
  });
  const initial = {};
  for (const [name, frame] of [['oracle', oracle], ['ctox', ctox]]) {
    const nameBox = frame.locator('#ce-cell-name');
    const contentBox = frame.locator('#ce-cell-content');
    if (await nameBox.count() !== 1 || await contentBox.count() !== 1) {
      throw new Error(`${name} original SpreadsheetEditor formula controls are missing`);
    }
    await nameBox.fill('A2');
    await nameBox.press('Enter');
    initial[name] = await state(frame);
    if (initial[name].cell_value !== 'CTOX_EDIT_CELL_ALPHA') {
      throw new Error(`${name} A2 fixture value mismatch: ${initial[name].cell_value}`);
    }
    await contentBox.fill(replacement);
    await contentBox.press('Enter');
    const dirty = await state(frame);
    if (!dirty.modified || !dirty.can_save) throw new Error(`${name} did not become saveable`);
  }
  await page.screenshot({ path: `${output}/differential-after-cell-edit.png` });

  for (const [name, frame] of [['oracle', oracle], ['ctox', ctox]]) {
    const save = frame.getByRole('button', { name: 'Speichern (⌘+S)', exact: true });
    if (await save.count() !== 1) throw new Error(`${name} original save button is missing`);
    await save.click();
  }
  await page.waitForFunction(() =>
    document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.state?.commits?.length === 1,
  null, { timeout: 30000 });
  await page.waitForTimeout(2200);
  const terminal = { oracle: await state(oracle), ctox: await state(ctox) };
  if (terminal.oracle.modified || terminal.oracle.can_save || terminal.ctox.can_save) {
    throw new Error(`editor save state did not clear: ${JSON.stringify(terminal)}`);
  }
  await page.screenshot({ path: `${output}/differential-after-edit-save.png` });
  // DocumentServer emits the canonical status=2 package when the last editor
  // session closes. The toolbar save above is still the user save action; this
  // deterministic close merely completes the Oracle persistence lifecycle.
  await page.evaluate(() => { document.querySelector('#oracle').src = 'about:blank'; });
  await page.waitForFunction(async () => {
    const response = await fetch(`http://127.0.0.1:4180/state/spreadsheet.edit-save?t=${Date.now()}`, { cache: 'no-store' });
    return response.ok && (await response.json()).saved === true;
  }, null, { timeout: 30000 });

  const capture = await ctoxHost.evaluate(async () => {
    const bytes = await window.ctoxOfficeEvidence.savedBytes();
    const response = await fetch('http://127.0.0.1:4180/capture-ctox/spreadsheet.edit-save', {
      method: 'POST', body: bytes,
    });
    return response.json();
  });
  const ctoxEvidence = await ctoxHost.evaluate(async () => ({
    inspection: await window.ctoxOfficeEvidence.editor.inspect(),
    commits: window.ctoxOfficeEvidence.state.commits,
  }));
  if (ctoxEvidence.inspection.dirty) throw new Error('CTOX public editor state remained dirty after commit');
  const oracleState = await page.evaluate(async () =>
    (await fetch('http://127.0.0.1:4180/state/spreadsheet.edit-save')).json());
  if (!oracleState.saved) throw new Error(`Oracle canonical save is missing: ${JSON.stringify(oracleState)}`);
  return {
    feature_id: feature,
    interaction: 'original-formula-bar-cell-edit-and-real-toolbar-save',
    replacement,
    gate,
    initial,
    terminal,
    capture,
    oracle: oracleState,
    ctox: ctoxEvidence,
  };
}
