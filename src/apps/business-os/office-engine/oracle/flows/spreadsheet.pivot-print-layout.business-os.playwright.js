async (page) => {
  const feature = 'spreadsheet.pivot-print-layout';
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.waitForFunction(() => document.querySelector('#status')?.textContent === 'document-ready', null, { timeout: 60000 });
  const frame = page.frames().find((value) => value.url().includes('/vendor/ctox-office/upstream/') && value.url().includes('/spreadsheeteditor/main/index.html'));
  if (!frame) throw new Error('Business OS did not mount the original SpreadsheetEditor ESM frame');
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
  await page.waitForFunction(() => window.businessOsSpreadsheetEvidence.commands.some(({ command }) => command.type === 'office.spreadsheet.commit'), null, { timeout: 30000 });
  await page.waitForTimeout(1200);
  const evidence = await page.evaluate(() => ({
    commands: window.businessOsSpreadsheetEvidence.commands.map(({ command, options }) => ({ command, options })),
    chunks: window.businessOsSpreadsheetEvidence.chunks.map((row) => ({ blob_id: row.blob_id, bytes: atob(row.data).length })),
    badge: document.querySelector('[data-spreadsheets-dirty-indicator]')?.textContent?.trim(),
  }));
  const commit = evidence.commands.find(({ command }) => command.type === 'office.spreadsheet.commit');
  const editorBlob = evidence.chunks.find(({ blob_id }) => blob_id === commit.command.payload.editor_blob_id);
  if (commit.options?.until !== 'terminal' || !editorBlob || evidence.badge !== 'Gespeichert') throw new Error(`Business OS pivot/print commit failed: ${JSON.stringify(evidence)}`);
  await page.screenshot({ path: `output/playwright/ctox-office/comparison/${feature}/business-os-mount-validation.png` });
  return { feature_id: feature, wrapper: 'modules/spreadsheets.mount(ctx)', command: commit.command.type, base_version_id: commit.command.payload.base_version_id, transport: commit.command.client_context?.transport, until: commit.options.until, editor_blob: editorBlob, badge: evidence.badge };
}
