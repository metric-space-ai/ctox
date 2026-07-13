async (page) => {
  const feature = 'spreadsheet.validation-conditional-formatting';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  await page.setViewportSize({ width: 1600, height: 900 });
  let oracle;
  let ctox;
  let ctoxHost;
  const deadline = Date.now() + 90000;
  while (Date.now() < deadline) {
    oracle = page.frames().find((frame) => frame.url().includes('127.0.0.1:8088/') && frame.url().includes('/spreadsheeteditor/main/index.html'));
    ctox = page.frames().find((frame) => frame.url().includes('/vendor/ctox-office/upstream/') && frame.url().includes('/spreadsheeteditor/main/index.html'));
    ctoxHost = page.frames().find((frame) => frame.url().includes('/ctox-spreadsheet-validation-conditional-formatting.html'));
    if (oracle && ctox && ctoxHost && await oracle.locator('#ce-cell-name').count() === 1 && await ctox.locator('#ce-cell-name').count() === 1) break;
    await page.waitForTimeout(500);
  }
  if (!oracle || !ctox || !ctoxHost) throw new Error('validation comparison frames are missing');
  await page.waitForFunction(() => window.ctoxOfficeComparison?.lastValidation?.valid === true, null, { timeout: 30000 });
  const gate = await page.evaluate(() => window.ctoxOfficeComparison.validate());
  if (!gate.valid) throw new Error(`comparison gate failed: ${gate.failures.join(', ')}`);
  const select = async (frame, reference) => {
    const name = frame.locator('#ce-cell-name');
    await name.fill(reference);
    await name.press('Enter');
    await page.waitForTimeout(150);
  };
  const edit = async (frame, reference, value) => {
    await select(frame, reference);
    const input = frame.locator('#ce-cell-content');
    await input.fill(value);
    await input.press('Enter');
    await page.waitForTimeout(250);
  };
  const semantic = async (frame) => frame.evaluate(() => {
    const rule = Asc.editor.asc_getDataValidationProps();
    return {
      type: rule.asc_getType(),
      operator: rule.asc_getOperator(),
      formula1: rule.asc_getFormula1()?.text || null,
      formula2: rule.asc_getFormula2()?.text || null,
      showErrorMessage: rule.asc_getShowErrorMessage(),
    };
  });
  const terminal = {};
  for (const [name, frame] of [['oracle', oracle], ['ctox', ctox]]) {
    await select(frame, 'B2');
    await frame.getByRole('tab', { name: 'Daten', exact: true }).click();
    await frame.locator('button:visible').filter({ hasText: 'Datenüberprüfung' }).click();
    const dialog = frame.getByRole('dialog');
    await dialog.waitFor({ state: 'visible' });
    const source = dialog.locator('input:visible:not([readonly])').first();
    if (await source.inputValue() !== 'Draft;Review;Final') throw new Error(`${name} validation source differs`);
    await source.fill('Draft;Review;Final;Approved');
    await dialog.getByRole('button', { name: 'OK', exact: true }).click();
    await page.waitForTimeout(350);
    await edit(frame, 'B2', 'Approved');
    await edit(frame, 'C2', '8');
    await edit(frame, 'E2', '80');
    await select(frame, 'B2');
    const approved = await frame.locator('#ce-cell-content').inputValue();
    await select(frame, 'E2');
    const thresholdValue = await frame.locator('#ce-cell-content').inputValue();
    await select(frame, 'B2');
    const state = await semantic(frame);
    if (approved !== 'Approved' || thresholdValue !== '80') throw new Error(`${name} edited values differ`);
    if (!String(state.formula1).includes('Approved')) throw new Error(`${name} validation did not update`);
    terminal[name] = { approved, quantity: '8', thresholdValue, semantic: state };
  }
  await page.screenshot({ path: `${output}/differential-terminal-before-save.png` });
  for (const frame of [oracle, ctox]) await frame.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.state?.commits?.length === 1, null, { timeout: 30000 });
  await page.waitForTimeout(5000);
  await page.evaluate(() => { document.querySelector('#oracle').src = 'about:blank'; });
  await page.waitForFunction(async () => {
    const response = await fetch(`http://127.0.0.1:4180/state/spreadsheet.validation-conditional-formatting?t=${Date.now()}`, { cache: 'no-store' });
    return response.ok && (await response.json()).saved === true;
  }, null, { timeout: 30000 });
  const capture = await ctoxHost.evaluate(async () => {
    const bytes = await window.ctoxOfficeEvidence.savedBytes();
    return (await fetch('http://127.0.0.1:4180/capture-ctox/spreadsheet.validation-conditional-formatting', { method: 'POST', body: bytes })).json();
  });
  const ctoxEvidence = await ctoxHost.evaluate(async () => ({ inspection: await window.ctoxOfficeEvidence.editor.inspect(), commits: window.ctoxOfficeEvidence.state.commits }));
  const oracleState = await page.evaluate(async () => (await fetch(`http://127.0.0.1:4180/state/spreadsheet.validation-conditional-formatting?t=${Date.now()}`, { cache: 'no-store' })).json());
  if (ctoxEvidence.inspection.dirty || !oracleState.saved) throw new Error('validation save evidence is incomplete');
  return { feature_id: feature, interaction: 'original-data-validation-dialog-and-conditional-rule-recalculation', gate, terminal, capture, oracle: oracleState, ctox: ctoxEvidence };
}
