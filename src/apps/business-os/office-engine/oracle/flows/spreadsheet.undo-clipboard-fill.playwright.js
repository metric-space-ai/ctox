async (page) => {
  const feature = 'spreadsheet.undo-clipboard-fill';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  const edited = 'UNDO_FILL_BASE_ONE';
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.waitForFunction(() => document.body.innerText.includes('Gate: Lauf gültig'), null, { timeout: 60000 });
  const gate = await page.evaluate(() => window.ctoxOfficeComparison.validate());
  if (!gate.valid) throw new Error(`comparison gate failed: ${gate.failures.join(', ')}`);

  const frames = page.frames();
  const oracle = frames.find((frame) => frame.url().includes('127.0.0.1:8088/')
    && frame.url().includes('/web-apps/apps/spreadsheeteditor/main/index.html'));
  const ctox = frames.find((frame) => frame.url().includes('/vendor/ctox-office/upstream/web-apps/apps/spreadsheeteditor/main/index.html'));
  const ctoxHost = frames.find((frame) => frame.url().includes('/ctox-spreadsheet-undo-clipboard-fill.html'));
  if (!oracle || !ctox || !ctoxHost) throw new Error('spreadsheet undo/clipboard/fill frames are missing');

  const selectAndRead = async (frame, reference) => {
    const name = frame.locator('#ce-cell-name');
    const content = frame.locator('#ce-cell-content');
    await name.fill(reference);
    await name.press('Enter');
    return content.inputValue();
  };
  const terminal = {};
  const dismissClipboardNotice = async (frame) => {
    const ok = frame.getByRole('button', { name: 'OK', exact: true });
    if (await ok.count()) await ok.click();
  };
  for (const [name, frame] of [['oracle', oracle], ['ctox', ctox]]) {
    const nameBox = frame.locator('#ce-cell-name');
    const content = frame.locator('#ce-cell-content');
    if (await nameBox.count() !== 1 || await content.count() !== 1) throw new Error(`${name} original formula controls are missing`);

    if (await selectAndRead(frame, 'A2') !== 'UNDO_FILL_BASE') throw new Error(`${name} A2 fixture mismatch`);
    await content.fill(edited);
    await content.press('Enter');
    const undo = frame.getByRole('button', { name: 'Rückgängig machen (⌘+Z)', exact: true });
    const redo = frame.getByRole('button', { name: 'Wiederholen (⌘+Y)', exact: true });
    await undo.click();
    if (await selectAndRead(frame, 'A2') !== 'UNDO_FILL_BASE') throw new Error(`${name} undo mismatch`);
    await redo.click();
    if (await selectAndRead(frame, 'A2') !== edited) throw new Error(`${name} redo mismatch`);

    if (await selectAndRead(frame, 'A3') !== 'COPY_SOURCE_TEXT') throw new Error(`${name} copy source mismatch`);
    await frame.getByRole('button', { name: 'Kopieren (⌘+C)', exact: true }).click();
    await dismissClipboardNotice(frame);
    await selectAndRead(frame, 'B3');
    await frame.getByRole('button', { name: 'Einfügen (⌘+V)', exact: true }).click();
    await dismissClipboardNotice(frame);
    await page.waitForTimeout(300);
    if (await selectAndRead(frame, 'B3') !== 'COPY_SOURCE_TEXT') throw new Error(`${name} paste mismatch`);

    await nameBox.fill('B4:B5');
    await nameBox.press('Enter');
    await frame.getByRole('button', { name: 'Mehr', exact: true }).click();
    await frame.getByRole('button', { name: 'Ausfüllen', exact: true }).click();
    await frame.getByText('Nach unten', { exact: true }).click();
    await page.waitForTimeout(300);
    if (await selectAndRead(frame, 'B4') !== '125000' || await selectAndRead(frame, 'B5') !== '125000') {
      throw new Error(`${name} fill-down mismatch`);
    }
    terminal[name] = { A2: await selectAndRead(frame, 'A2'), B3: await selectAndRead(frame, 'B3'), B5: await selectAndRead(frame, 'B5') };
  }
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
    const response = await fetch(`http://127.0.0.1:4180/state/spreadsheet.undo-clipboard-fill?t=${Date.now()}`, { cache: 'no-store' });
    return response.ok && (await response.json()).saved === true;
  }, null, { timeout: 30000 });

  const capture = await ctoxHost.evaluate(async () => {
    const bytes = await window.ctoxOfficeEvidence.savedBytes();
    const response = await fetch('http://127.0.0.1:4180/capture-ctox/spreadsheet.undo-clipboard-fill', { method: 'POST', body: bytes });
    return response.json();
  });
  const ctoxEvidence = await ctoxHost.evaluate(async () => ({
    inspection: await window.ctoxOfficeEvidence.editor.inspect(),
    commits: window.ctoxOfficeEvidence.state.commits,
  }));
  const oracleState = await page.evaluate(async () =>
    (await fetch(`http://127.0.0.1:4180/state/spreadsheet.undo-clipboard-fill?t=${Date.now()}`, { cache: 'no-store' })).json());
  if (ctoxEvidence.inspection.dirty || !oracleState.saved) throw new Error('save evidence is incomplete');
  return {
    feature_id: feature,
    interaction: 'original-toolbar-undo-redo-copy-paste-and-fill-down',
    gate,
    terminal,
    capture,
    oracle: oracleState,
    ctox: ctoxEvidence,
  };
}
