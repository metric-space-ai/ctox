async (page) => {
  const feature = 'spreadsheet.charts';
  await page.setViewportSize({ width: 1200, height: 800 });
  await page.waitForFunction(
    () => window.ctoxOfficeComparison?.lastValidation?.valid === true,
    null,
    { timeout: 60000 },
  );
  const originalFrame = page.frames().find((frame) =>
    frame.url().includes('127.0.0.1:8088') && frame.url().includes('/spreadsheeteditor/main/'));
  const ctoxFrame = page.frames().find((frame) =>
    frame.url().includes('/vendor/ctox-office/upstream/') && frame.url().includes('/spreadsheeteditor/main/'));
  if (!originalFrame || !ctoxFrame) throw new Error('Both original SpreadsheetEditor frames are required');

  const outerFrames = [page.locator('#oracle'), page.locator('#ctox')];
  const editorFrames = [originalFrame, ctoxFrame];
  const selected = [];
  for (let index = 0; index < outerFrames.length; index += 1) {
    const box = await outerFrames[index].boundingBox();
    if (!box) throw new Error('Comparison iframe geometry is unavailable');
    const point = { x: box.x + box.width * 0.78, y: box.y + box.height * 0.48 };
    await page.mouse.click(point.x, point.y);
    await page.waitForTimeout(300);
    const name = await editorFrames[index].locator('#ce-cell-name').inputValue();
    if (name !== 'CTOX Revenue Chart') throw new Error(`Chart selection differs: ${name}`);
    selected.push({ box, point, name });
  }

  for (const frame of editorFrames) {
    await frame.getByRole('button', { name: 'Diagrammeinstellungen' }).click();
    await frame.getByRole('spinbutton', { name: 'Breite' }).fill('14');
    await frame.getByRole('spinbutton', { name: 'Höhe' }).fill('8');
    await page.keyboard.press('Enter');
    const tip = frame.getByText('OK', { exact: true });
    if (await tip.count()) await tip.first().click();
    await frame.getByRole('listitem', { name: 'Stil 2' }).click();
  }
  for (const frame of editorFrames) {
    await frame.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  }
  await page.waitForFunction(
    () => window.ctoxOfficeComparison.frames.ctox.contentWindow.ctoxOfficeEvidence.state.commits.length === 1,
    null,
    { timeout: 30000 },
  );
  const ctoxOuter = page.frames().find((frame) => frame.url().includes('ctox-spreadsheet-charts.html'));
  const bytes = await ctoxOuter.evaluate(async () =>
    Array.from((await window.ctoxOfficeEvidence.editor.export({ format: 'xlsx' })).bytes));
  await page.evaluate(async (payload) => {
    await fetch('http://127.0.0.1:4180/capture-ctox/spreadsheet.charts', {
      method: 'POST', mode: 'no-cors', headers: { 'content-type': 'text/plain' }, body: new Uint8Array(payload),
    });
  }, bytes);
  await page.screenshot({ path: `output/playwright/ctox-office/comparison/${feature}/original-ui-size-style-change.png` });
  return { feature_id: feature, selected, editor_bytes: bytes.length, width_cm: 14, height_cm: 8, style: 2 };
}
