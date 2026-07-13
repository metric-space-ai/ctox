async (page) => {
  const feature = 'spreadsheet.multi-sheet-merge-freeze';
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.waitForFunction(() => document.querySelector('#status')?.textContent === 'document-ready', null, { timeout: 60000 });
  const frame = page.frames().find((value) => value.url().includes('/vendor/ctox-office/upstream/') && value.url().includes('/spreadsheeteditor/main/index.html'));
  if (!frame) throw new Error('Business OS did not mount the original SpreadsheetEditor ESM frame');
  const select = async (reference) => { const name = frame.locator('#ce-cell-name'); await name.fill(reference); await name.press('Enter'); await page.waitForTimeout(180); };
  const marker = async () => { await select('A2'); return frame.locator('#ce-cell-content').inputValue(); };
  const sheets = (await frame.getByRole('listitem').allInnerTexts()).filter((value) => /Overview|Details|Archive/.test(value));
  if (sheets.length !== 2 || sheets.some((value) => value.includes('Archive'))) throw new Error(`Business OS hidden-sheet semantics failed: ${sheets}`);
  await frame.getByRole('listitem').filter({ hasText: 'Details' }).click();
  if (await marker() !== 'OPERATIONS_MARKER_A9C4') throw new Error('Business OS Details navigation failed');
  await frame.getByRole('listitem').filter({ hasText: 'Overview' }).click();
  if (await marker() !== 'OVERVIEW_MARKER_6F21') throw new Error('Business OS Overview navigation failed');
  const toggleMerge = async (reference) => {
    await select(reference);
    const primary = frame.getByRole('button', { name: 'Verbinden und zentrieren', exact: true }).first();
    if (!await primary.isVisible()) {
      await frame.getByRole('tab', { name: 'Startseite' }).click();
      await frame.getByRole('button', { name: 'Mehr', exact: true }).filter({ visible: true }).click();
    }
    await primary.click();
    await page.waitForTimeout(250);
  };
  await toggleMerge('B2:C2');
  await toggleMerge('B3:C3');
  await select('B3');
  await frame.evaluate(() => { Asc.editor.asc_freezePane(undefined); Asc.editor.asc_freezePane(null, 1, 2); });
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
  if (commit.options?.until !== 'terminal' || !editorBlob || evidence.badge !== 'Gespeichert') throw new Error(`Business OS multi-sheet commit failed: ${JSON.stringify(evidence)}`);
  await page.screenshot({ path: `output/playwright/ctox-office/comparison/${feature}/business-os-mount-multi-sheet.png` });
  return { feature_id: feature, wrapper: 'modules/spreadsheets.mount(ctx)', command: commit.command.type, base_version_id: commit.command.payload.base_version_id, transport: commit.command.client_context?.transport, until: commit.options.until, editor_blob: editorBlob, badge: evidence.badge, visible_sheets: sheets };
}
