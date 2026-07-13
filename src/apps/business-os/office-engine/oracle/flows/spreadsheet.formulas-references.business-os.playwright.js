async (page) => {
  const feature = 'spreadsheet.formulas-references';
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.waitForFunction(() => document.querySelector('#status')?.textContent === 'document-ready', null, { timeout: 60000 });
  const frame = page.frames().find((value) => value.url().includes('/vendor/ctox-office/upstream/') && value.url().includes('/spreadsheeteditor/main/index.html'));
  if (!frame) throw new Error('Business OS did not mount the original SpreadsheetEditor ESM frame');
  const nameBox = frame.locator('#ce-cell-name');
  const select = async (reference) => { await nameBox.fill(reference); await nameBox.press('Enter'); await page.waitForTimeout(150); };
  const formula = async (reference) => { await select(reference); return frame.locator('#ce-cell-content').inputValue(); };
  const setFormula = async (reference, value) => { await select(reference); const bar = frame.locator('#ce-cell-content'); await bar.fill(value); await bar.press('Enter'); await page.waitForTimeout(250); };
  if (await formula('B3') !== '=B2*2') throw new Error('Business OS native formula payload did not open');
  await setFormula('D3', '=D2*3');
  await select('B7');
  await frame.getByRole('button', { name: 'Kopieren (⌘+C)', exact: true }).click();
  await select('C7');
  await frame.getByRole('button', { name: 'Einfügen (⌘+V)', exact: true }).click();
  await page.waitForTimeout(300);
  const clipboardDialog = frame.getByRole('dialog');
  if (await clipboardDialog.isVisible()) await clipboardDialog.getByRole('button', { name: 'OK', exact: true }).click();
  if (await formula('D3') !== '=D2*3' || await formula('C7') !== '=C2+1') throw new Error('Business OS formula edits differ from the Oracle flow');
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
  if (commit.options?.until !== 'terminal' || !editorBlob || evidence.badge !== 'Gespeichert') throw new Error(`Business OS formula commit failed: ${JSON.stringify(evidence)}`);
  await page.screenshot({ path: `output/playwright/ctox-office/comparison/${feature}/business-os-mount-formulas.png` });
  return { feature_id: feature, wrapper: 'modules/spreadsheets.mount(ctx)', command: commit.command.type, base_version_id: commit.command.payload.base_version_id, transport: commit.command.client_context?.transport, until: commit.options.until, editor_blob: editorBlob, badge: evidence.badge };
}
