async (page) => {
  const feature = 'spreadsheet.undo-clipboard-fill';
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.waitForFunction(() => document.querySelector('#status')?.textContent === 'document-ready', null, { timeout: 60000 });
  const frame = page.frames().find((value) => value.url().includes('/vendor/ctox-office/upstream/')
    && value.url().includes('/spreadsheeteditor/main/index.html'));
  if (!frame) throw new Error('Business OS did not mount the original SpreadsheetEditor ESM frame');
  const nameBox = frame.locator('#ce-cell-name');
  const content = frame.locator('#ce-cell-content');
  const selectAndRead = async (reference) => {
    await nameBox.fill(reference);
    await nameBox.press('Enter');
    return content.inputValue();
  };
  const dismissNotice = async () => {
    const ok = frame.getByRole('button', { name: 'OK', exact: true });
    if (await ok.count()) await ok.click();
  };

  await selectAndRead('A2');
  await content.fill('UNDO_FILL_BASE_ONE');
  await content.press('Enter');
  await frame.getByRole('button', { name: 'Rückgängig machen (⌘+Z)', exact: true }).click();
  if (await selectAndRead('A2') !== 'UNDO_FILL_BASE') throw new Error('Business OS undo failed');
  await frame.getByRole('button', { name: 'Wiederholen (⌘+Y)', exact: true }).click();
  if (await selectAndRead('A2') !== 'UNDO_FILL_BASE_ONE') throw new Error('Business OS redo failed');
  await selectAndRead('A3');
  await frame.getByRole('button', { name: 'Kopieren (⌘+C)', exact: true }).click();
  await dismissNotice();
  await selectAndRead('B3');
  await frame.getByRole('button', { name: 'Einfügen (⌘+V)', exact: true }).click();
  await dismissNotice();
  if (await selectAndRead('B3') !== 'COPY_SOURCE_TEXT') throw new Error('Business OS paste failed');
  await nameBox.fill('B4:B5');
  await nameBox.press('Enter');
  await frame.getByRole('button', { name: 'Mehr', exact: true }).click();
  await frame.getByRole('button', { name: 'Ausfüllen', exact: true }).click();
  await frame.getByText('Nach unten', { exact: true }).click();
  if (await selectAndRead('B5') !== '125000') throw new Error('Business OS fill down failed');
  await frame.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await page.waitForFunction(() => window.businessOsSpreadsheetEvidence.commands
    .some(({ command }) => command.type === 'office.spreadsheet.commit'), null, { timeout: 30000 });
  await page.waitForTimeout(2200);
  const evidence = await page.evaluate(() => ({
    commands: window.businessOsSpreadsheetEvidence.commands.map(({ command, options }) => ({ command, options })),
    chunks: window.businessOsSpreadsheetEvidence.chunks.map((row) => ({ blob_id: row.blob_id, bytes: atob(row.data).length })),
    badge: document.querySelector('[data-spreadsheets-dirty-indicator]')?.textContent?.trim(),
  }));
  const commit = evidence.commands.find(({ command }) => command.type === 'office.spreadsheet.commit');
  const editorBlob = evidence.chunks.find(({ blob_id }) => blob_id === commit.command.payload.editor_blob_id);
  if (commit.options?.until !== 'terminal' || !editorBlob || evidence.badge !== 'Gespeichert') {
    throw new Error(`Business OS commit evidence failed: ${JSON.stringify(evidence)}`);
  }
  await page.screenshot({ path: `output/playwright/ctox-office/comparison/${feature}/business-os-mount-undo-clipboard-fill.png` });
  return {
    feature_id: feature,
    wrapper: 'modules/spreadsheets.mount(ctx)',
    command: commit.command.type,
    base_version_id: commit.command.payload.base_version_id,
    transport: commit.command.client_context?.transport,
    until: commit.options.until,
    editor_blob: editorBlob,
    badge: evidence.badge,
  };
}
