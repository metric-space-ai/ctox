async (page) => {
  const feature = 'spreadsheet.open-render-sheets';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
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
  const ctoxHost = frames.find((frame) => frame.url().includes('/ctox-spreadsheet-open-render-sheets.html'));
  if (!oracle || !ctox || !ctoxHost) throw new Error('spreadsheet comparison frames are missing');

  const workbookState = (frame) => frame.evaluate(() => {
    const api = window.editor || window.Asc?.editor;
    const count = api.asc_getWorksheetsCount();
    const active = api.asc_getActiveWorksheetIndex();
    const worksheets = [];
    for (let index = 0; index < count; index += 1) {
      worksheets.push({
        index,
        name: api.asc_getWorksheetName(index),
        hidden: api.asc_isWorksheetHidden(index),
      });
    }
    return {
      active_index: active,
      active_sheet: api.asc_getWorksheetName(active),
      worksheets,
      modified: api.isDocumentModified?.() === true,
      can_save: api.asc_isDocumentCanSave?.() === true,
    };
  });
  const initial = {};
  for (const [name, frame] of [['oracle', oracle], ['ctox', ctox]]) {
    initial[name] = await workbookState(frame);
    if (initial[name].active_sheet !== 'Overview') throw new Error(`${name} did not start on Overview`);
    if (initial[name].worksheets.length !== 3) throw new Error(`${name} worksheet count mismatch`);
    if (initial[name].worksheets.filter((sheet) => !sheet.hidden).length !== 2) {
      throw new Error(`${name} visible worksheet count mismatch`);
    }
    if (initial[name].worksheets.find((sheet) => sheet.name === 'Archive')?.hidden !== true) {
      throw new Error(`${name} Archive worksheet is not hidden`);
    }
  }
  await page.screenshot({ path: `${output}/differential-initial-open-render.png` });

  for (const [name, frame] of [['oracle', oracle], ['ctox', ctox]]) {
    const details = frame.locator('#statusbar_bottom [data-label="Details"] span[tabtitle="Details"]');
    if (await details.count() !== 1) throw new Error(`${name} Details sheet tab is missing`);
    await details.click();
    await page.waitForFunction(
      ({ frameName }) => {
        const target = window.frames[frameName];
        return target != null;
      },
      { frameName: name === 'oracle' ? 'oracle' : 'ctox' },
      { timeout: 1000 },
    ).catch(() => {});
    await page.waitForTimeout(350);
    const state = await workbookState(frame);
    if (state.active_sheet !== 'Details') throw new Error(`${name} did not navigate to Details`);
  }
  const terminal = {
    oracle: await workbookState(oracle),
    ctox: await workbookState(ctox),
  };
  if (terminal.oracle.modified || terminal.ctox.modified) throw new Error('view navigation dirtied the workbook');
  const inspection = await ctoxHost.evaluate(() => window.ctoxOfficeEvidence.editor.inspect());
  if (inspection.runtime !== 'ctox-spreadsheets-fork') throw new Error('CTOX upstream runtime proof is missing');
  await page.screenshot({ path: `${output}/differential-details-open-render.png` });
  return {
    feature_id: feature,
    interaction: 'real-spreadsheeteditor-statusbar-sheet-tab-navigation',
    gate,
    initial,
    terminal,
    inspection,
  };
}
