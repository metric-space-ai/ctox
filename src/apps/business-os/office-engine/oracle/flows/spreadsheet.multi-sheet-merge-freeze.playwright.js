async (page) => {
  const feature = 'spreadsheet.multi-sheet-merge-freeze';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  await page.setViewportSize({ width: 1600, height: 900 });
  let oracle;
  let ctox;
  let ctoxHost;
  const deadline = Date.now() + 90000;
  while (Date.now() < deadline) {
    oracle = page.frames().find((frame) => frame.url().includes('127.0.0.1:8088/') && frame.url().includes('/spreadsheeteditor/main/index.html'));
    ctox = page.frames().find((frame) => frame.url().includes('/vendor/ctox-office/upstream/') && frame.url().includes('/spreadsheeteditor/main/index.html'));
    ctoxHost = page.frames().find((frame) => frame.url().includes('/ctox-spreadsheet-multi-sheet-merge-freeze.html'));
    if (oracle && ctox && ctoxHost && await oracle.locator('#ce-cell-name').count() === 1 && await ctox.locator('#ce-cell-name').count() === 1) break;
    await page.waitForTimeout(500);
  }
  if (!oracle || !ctox || !ctoxHost) throw new Error('multi-sheet comparison frames are missing');
  await page.waitForFunction(() => window.ctoxOfficeComparison?.lastValidation?.valid === true, null, { timeout: 30000 });
  const gate = await page.evaluate(() => window.ctoxOfficeComparison.validate());
  if (!gate.valid) throw new Error(`comparison gate failed: ${gate.failures.join(', ')}`);

  const select = async (frame, reference) => {
    const name = frame.locator('#ce-cell-name');
    await name.fill(reference);
    await name.press('Enter');
    await page.waitForTimeout(180);
  };
  const marker = async (frame, reference = 'A2') => {
    await select(frame, reference);
    return frame.locator('#ce-cell-content').inputValue();
  };
  const switchSheet = async (frame, name) => {
    await frame.getByRole('listitem').filter({ hasText: name }).click();
    await page.waitForTimeout(250);
  };
  const toggleMerge = async (frame, reference) => {
    await select(frame, reference);
    const controls = frame.getByRole('button', { name: 'Verbinden und zentrieren', exact: true });
    const primary = controls.first();
    if (!await primary.isVisible()) {
      await frame.getByRole('tab', { name: 'Startseite' }).click();
      await frame.getByRole('button', { name: 'Mehr', exact: true }).filter({ visible: true }).click();
    }
    await primary.click();
    await page.waitForTimeout(250);
  };
  const freezeAtB3 = async (frame) => {
    await select(frame, 'B3');
    // Original sdkjs command used by the original ViewTab controller. The split
    // screen is intentionally too narrow to expose that toolbar group reliably.
    await frame.evaluate(() => {
      Asc.editor.asc_freezePane(undefined);
      Asc.editor.asc_freezePane(null, 1, 2);
    });
    await page.waitForTimeout(250);
  };

  const terminal = {};
  for (const [name, frame] of [['oracle', oracle], ['ctox', ctox]]) {
    const visibleSheets = await frame.getByRole('listitem').allInnerTexts();
    if (!visibleSheets.some((value) => value.includes('Overview')) || !visibleSheets.some((value) => value.includes('Details')) || visibleSheets.some((value) => value.includes('Archive'))) {
      throw new Error(`${name} visible sheet list does not preserve hidden Archive semantics`);
    }
    await switchSheet(frame, 'Details');
    if (await marker(frame) !== 'OPERATIONS_MARKER_A9C4') throw new Error(`${name} Details navigation failed`);
    await switchSheet(frame, 'Overview');
    if (await marker(frame) !== 'OVERVIEW_MARKER_6F21') throw new Error(`${name} Overview navigation failed`);
    await toggleMerge(frame, 'B2:C2');
    await toggleMerge(frame, 'B3:C3');
    await freezeAtB3(frame);
    terminal[name] = {
      sheet: await frame.evaluate(() => Asc.editor.asc_getWorksheetName(Asc.editor.asc_getActiveWorksheetIndex())),
      activeCell: await frame.locator('#ce-cell-name').inputValue(),
      visibleSheets: visibleSheets.filter((value) => /Overview|Details|Archive/.test(value)),
    };
  }
  await page.screenshot({ path: `${output}/differential-terminal-before-save.png` });
  for (const frame of [oracle, ctox]) await frame.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.state?.commits?.length === 1, null, { timeout: 30000 });
  await page.waitForTimeout(5000);
  await page.evaluate(() => { document.querySelector('#oracle').src = 'about:blank'; });
  await page.waitForFunction(async () => {
    const response = await fetch(`http://127.0.0.1:4180/state/spreadsheet.multi-sheet-merge-freeze?t=${Date.now()}`, { cache: 'no-store' });
    return response.ok && (await response.json()).saved === true;
  }, null, { timeout: 30000 });
  const capture = await ctoxHost.evaluate(async () => {
    const bytes = await window.ctoxOfficeEvidence.savedBytes();
    return (await fetch('http://127.0.0.1:4180/capture-ctox/spreadsheet.multi-sheet-merge-freeze', { method: 'POST', body: bytes })).json();
  });
  const ctoxEvidence = await ctoxHost.evaluate(async () => ({ inspection: await window.ctoxOfficeEvidence.editor.inspect(), commits: window.ctoxOfficeEvidence.state.commits }));
  const oracleState = await page.evaluate(async () => (await fetch(`http://127.0.0.1:4180/state/spreadsheet.multi-sheet-merge-freeze?t=${Date.now()}`, { cache: 'no-store' })).json());
  if (ctoxEvidence.inspection.dirty || !oracleState.saved) throw new Error('multi-sheet save evidence is incomplete');
  return { feature_id: feature, interaction: 'original-sheet-tabs-merge-toolbar-and-sdkjs-view-command', gate, terminal, capture, oracle: oracleState, ctox: ctoxEvidence };
}
