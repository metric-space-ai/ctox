async (page) => {
  const feature = 'spreadsheet.formulas-references';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  await page.setViewportSize({ width: 1600, height: 900 });
  let oracle;
  let ctox;
  let ctoxHost;
  const deadline = Date.now() + 90000;
  while (Date.now() < deadline) {
    const frames = page.frames();
    oracle = frames.find((frame) => frame.url().includes('127.0.0.1:8088/') && frame.url().includes('/web-apps/apps/spreadsheeteditor/main/index.html'));
    ctox = frames.find((frame) => frame.url().includes('/vendor/ctox-office/upstream/web-apps/apps/spreadsheeteditor/main/index.html'));
    ctoxHost = frames.find((frame) => frame.url().includes('/ctox-spreadsheet-formulas-references.html'));
    if (oracle && ctox && ctoxHost && await oracle.locator('#ce-cell-name').count() === 1 && await ctox.locator('#ce-cell-name').count() === 1) break;
    await page.waitForTimeout(500);
  }
  if (!oracle || !ctox || !ctoxHost) throw new Error('formula comparison frames are missing');
  await page.waitForFunction(() => window.ctoxOfficeComparison?.lastValidation?.valid === true, null, { timeout: 30000 });
  const gate = await page.evaluate(() => window.ctoxOfficeComparison.validate());
  if (!gate.valid) throw new Error(`comparison gate failed: ${gate.failures.join(', ')}`);

  const select = async (frame, reference) => {
    const name = frame.locator('#ce-cell-name');
    await name.fill(reference);
    await name.press('Enter');
    await page.waitForTimeout(150);
  };
  const normalizeFormula = (value) => value
    .replace(/^=SUMME\(/, '=SUM(')
    .replace(/^=([A-Za-z0-9_]+)!/, "='$1'!");
  const formula = async (frame, reference) => {
    await select(frame, reference);
    return normalizeFormula(await frame.locator('#ce-cell-content').inputValue());
  };
  const setFormula = async (frame, reference, value) => {
    await select(frame, reference);
    const bar = frame.locator('#ce-cell-content');
    await bar.fill(value);
    await bar.press('Enter');
    await page.waitForTimeout(250);
  };
  const expectedInitial = {
    B3: '=B2*2', B4: '=$B$2+5', B5: '=SUM(B2:B4)', B6: "='Details'!B4+1", B7: '=B2+1', B8: '=1/0',
  };
  for (const [name, frame] of [['oracle', oracle], ['ctox', ctox]]) {
    for (const [reference, expected] of Object.entries(expectedInitial)) {
      const actual = await formula(frame, reference);
      if (actual !== expected) throw new Error(`${name} initial ${reference} formula mismatch: ${actual}`);
    }
    await setFormula(frame, 'D3', '=D2*3');
    await select(frame, 'B7');
    await frame.getByRole('button', { name: 'Kopieren (⌘+C)', exact: true }).click();
    await select(frame, 'C7');
    await frame.getByRole('button', { name: 'Einfügen (⌘+V)', exact: true }).click();
    await page.waitForTimeout(350);
    const clipboardDialog = frame.getByRole('dialog');
    if (await clipboardDialog.isVisible()) await clipboardDialog.getByRole('button', { name: 'OK', exact: true }).click();
    if (await formula(frame, 'D3') !== '=D2*3') throw new Error(`${name} manual formula edit failed`);
    if (await formula(frame, 'C7') !== '=C2+1') throw new Error(`${name} relative copy shift failed`);
    if (await formula(frame, 'B4') !== '=$B$2+5') throw new Error(`${name} absolute reference changed`);
    if (await formula(frame, 'B8') !== '=1/0') throw new Error(`${name} error formula changed`);
  }
  const terminal = {
    oracle: { D3: await formula(oracle, 'D3'), C7: await formula(oracle, 'C7'), B4: await formula(oracle, 'B4'), B8: await formula(oracle, 'B8') },
    ctox: { D3: await formula(ctox, 'D3'), C7: await formula(ctox, 'C7'), B4: await formula(ctox, 'B4'), B8: await formula(ctox, 'B8') },
  };
  await page.screenshot({ path: `${output}/differential-terminal-before-save.png` });
  for (const frame of [oracle, ctox]) await frame.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.state?.commits?.length === 1, null, { timeout: 30000 });
  await page.waitForTimeout(2200);
  await page.evaluate(() => { document.querySelector('#oracle').src = 'about:blank'; });
  await page.waitForFunction(async () => {
    const response = await fetch(`http://127.0.0.1:4180/state/spreadsheet.formulas-references?t=${Date.now()}`, { cache: 'no-store' });
    return response.ok && (await response.json()).saved === true;
  }, null, { timeout: 30000 });
  const capture = await ctoxHost.evaluate(async () => {
    const bytes = await window.ctoxOfficeEvidence.savedBytes();
    return (await fetch('http://127.0.0.1:4180/capture-ctox/spreadsheet.formulas-references', { method: 'POST', body: bytes })).json();
  });
  const ctoxEvidence = await ctoxHost.evaluate(async () => ({ inspection: await window.ctoxOfficeEvidence.editor.inspect(), commits: window.ctoxOfficeEvidence.state.commits }));
  const oracleState = await page.evaluate(async () => (await fetch(`http://127.0.0.1:4180/state/spreadsheet.formulas-references?t=${Date.now()}`, { cache: 'no-store' })).json());
  if (ctoxEvidence.inspection.dirty || !oracleState.saved) throw new Error('formula save evidence is incomplete');
  return { feature_id: feature, interaction: 'original-formula-bar-and-copy-paste-toolbar', gate, terminal, capture, oracle: oracleState, ctox: ctoxEvidence };
}
