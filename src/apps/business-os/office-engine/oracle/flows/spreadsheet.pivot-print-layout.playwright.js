async (page) => {
  const feature = 'spreadsheet.pivot-print-layout';
  await page.setViewportSize({ width: 1200, height: 800 });
  await page.waitForFunction(() => window.ctoxOfficeComparison?.lastValidation?.valid === true, null, { timeout: 60000 });
  const frames = page.frames().filter((frame) => frame.url().includes('/spreadsheeteditor/main/'));
  if (frames.length !== 2) throw new Error('Both original SpreadsheetEditor frames are required');
  for (const frame of frames) {
    await frame.locator('#ce-cell-name').fill('E3');
    await page.keyboard.press('Enter');
    await frame.getByRole('button', { name: 'Einstellungen der Pivot-Tabelle' }).click();
    await frame.locator('#pivot-advanced-link').click();
    await frame.locator('#pivot-adv-name').getByRole('textbox').fill('CTOXRevenuePivot2026');
    await frame.getByRole('button', { name: 'OK' }).click();
    await frame.getByRole('tab', { name: 'Layout' }).click();
    await frame.locator('#tlbtn-editheader-1').click();
    await frame.getByRole('checkbox', { name: 'Erste Seite anders' }).click();
    await frame.getByRole('button', { name: 'OK' }).click();
    await frame.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  }
  await page.waitForFunction(() => window.ctoxOfficeComparison.frames.ctox.contentWindow.ctoxOfficeEvidence.state.commits.length === 1, null, { timeout: 30000 });
  const bytes = await page.locator('#ctox').evaluate(async (element) => Array.from((await element.contentWindow.ctoxOfficeEvidence.editor.export({ format: 'xlsx' })).bytes));
  await page.evaluate(async (payload) => fetch('http://127.0.0.1:4180/capture-ctox/spreadsheet.pivot-print-layout', { method: 'POST', mode: 'no-cors', headers: { 'content-type': 'text/plain' }, body: new Uint8Array(payload) }), bytes);
  await page.screenshot({ path: `output/playwright/ctox-office/comparison/${feature}/original-ui-pivot-header-change.png` });
  return { feature_id: feature, pivot_name: 'CTOXRevenuePivot2026', different_first_page: true, editor_bytes: bytes.length };
}
