async (page) => {
  const feature = 'spreadsheet.validation-conditional-formatting';
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.waitForFunction(() => document.querySelector('#status')?.textContent === 'document-ready', null, { timeout: 60000 });
  const frame = page.frames().find((value) => value.url().includes('/vendor/ctox-office/upstream/') && value.url().includes('/spreadsheeteditor/main/index.html'));
  if (!frame) throw new Error('Business OS did not mount the original SpreadsheetEditor ESM frame');
  const select = async (reference) => {
    const name = frame.locator('#ce-cell-name');
    await name.fill(reference);
    await name.press('Enter');
    await page.waitForTimeout(150);
  };
  const edit = async (reference, value) => {
    await select(reference);
    const input = frame.locator('#ce-cell-content');
    await input.fill(value);
    await input.press('Enter');
    await page.waitForTimeout(250);
  };
  await select('B2');
  await frame.getByRole('tab', { name: 'Daten', exact: true }).click();
  await frame.locator('button:visible').filter({ hasText: 'Datenüberprüfung' }).click();
  const dialog = frame.getByRole('dialog');
  const source = dialog.locator('input:visible:not([readonly])').first();
  if (await source.inputValue() !== 'Draft;Review;Final') throw new Error('Business OS validation source differs');
  await source.fill('Draft;Review;Final;Approved');
  await dialog.getByRole('button', { name: 'OK', exact: true }).click();
  await edit('B2', 'Approved');
  await edit('C2', '8');
  await edit('E2', '80');
  await select('B2');
  const validation = await frame.evaluate(() => {
    const rule = Asc.editor.asc_getDataValidationProps();
    return { type: rule.asc_getType(), formula1: rule.asc_getFormula1()?.text || null };
  });
  if (!validation.formula1?.includes('Approved')) throw new Error('Business OS validation did not update');
  await frame.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await page.waitForFunction(() => window.businessOsSpreadsheetEvidence.commands.some(({ command }) => command.type === 'office.spreadsheet.commit'), null, { timeout: 30000 });
  await page.waitForTimeout(1200);
  const evidence = await page.evaluate(() => ({
    commands: window.businessOsSpreadsheetEvidence.commands.map(({ command, options }) => ({ command, options })),
    chunks: window.businessOsSpreadsheetEvidence.chunks.map((row) => ({ blob_id: row.blob_id, bytes: atob(row.data).length })),
    badge: document.querySelector('[data-spreadsheets-dirty-indicator]')?.textContent?.trim(),
  }));
  const commit = evidence.commands.find(({ command }) => command.type === 'office.spreadsheet.commit');
  const editorBlob = evidence.chunks.find(({ blob_id }) => blob_id === commit.command.payload.editor_blob_id);
  if (commit.options?.until !== 'terminal' || !editorBlob || evidence.badge !== 'Gespeichert') throw new Error(`Business OS validation commit failed: ${JSON.stringify(evidence)}`);
  await page.screenshot({ path: `output/playwright/ctox-office/comparison/${feature}/business-os-mount-validation.png` });
  return {
    feature_id: feature,
    wrapper: 'modules/spreadsheets.mount(ctx)',
    command: commit.command.type,
    base_version_id: commit.command.payload.base_version_id,
    transport: commit.command.client_context?.transport,
    until: commit.options.until,
    editor_blob: editorBlob,
    badge: evidence.badge,
    validation,
  };
}
