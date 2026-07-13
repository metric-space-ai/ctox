async (page) => {
  const feature = 'spreadsheet.cell-format-rows-columns';
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.waitForFunction(() => document.querySelector('#status')?.textContent === 'document-ready', null, { timeout: 60000 });
  const frame = page.frames().find((value) => value.url().includes('/vendor/ctox-office/upstream/')
    && value.url().includes('/spreadsheeteditor/main/index.html'));
  if (!frame) throw new Error('Business OS did not mount the original SpreadsheetEditor ESM frame');
  const nameBox = frame.locator('#ce-cell-name');
  const select = async (reference) => { await nameBox.fill(reference); await nameBox.press('Enter'); await page.waitForTimeout(120); };
  const formatButton = () => frame.getByRole('button').filter({ hasText: 'Format' });
  const openFormat = async () => {
    if (!await formatButton().isVisible()) await frame.getByRole('button', { name: 'Mehr', exact: true }).click();
    await formatButton().click();
  };
  const setSize = async (axis, value) => {
    await openFormat();
    await frame.getByText(axis === 'row' ? 'Zeilenhöhe' : 'Spaltenbreite', { exact: true }).click();
    await frame.getByText(axis === 'row' ? 'Benutzerdefinierte Zeilenhöhe' : 'Benutzerdefinierte Spaltenbreite', { exact: true }).click();
    const dialog = frame.getByRole('dialog');
    await dialog.getByRole('spinbutton').fill(String(value));
    await dialog.getByRole('button', { name: 'OK', exact: true }).click();
  };

  await select('A2');
  await frame.getByRole('button', { name: 'Fett (⌘+B)', exact: true }).click();
  await frame.getByRole('button', { name: 'Kursiv (⌘+I)', exact: true }).click();
  await select('B4');
  if (!await formatButton().isVisible()) await frame.getByRole('button', { name: 'Mehr', exact: true }).click();
  const accounting = frame.getByRole('button', { name: 'Buchhaltungsformat', exact: true });
  if (await accounting.count() !== 2) throw new Error('Business OS accounting controls are incomplete');
  await accounting.nth(1).click();
  await frame.getByText('€ Euro', { exact: true }).click();
  await select('A4');
  await setSize('row', 27.75);
  await select('B3');
  await setSize('column', 32.625);
  await select('A5');
  await openFormat();
  const hide = frame.getByText('Ausblenden', { exact: true });
  if (await hide.count() !== 1) throw new Error('Business OS hide-row action is ambiguous');
  await hide.click();
  const rows = frame.getByText('Zeilen', { exact: true });
  if (await rows.count() !== 3) throw new Error('Business OS row submenu is incomplete');
  await rows.nth(0).click();
  await select('A4:A6');
  await openFormat();
  const show = frame.getByText('Anzeigen', { exact: true });
  if (await show.count() !== 2) throw new Error('Business OS show-row action is incomplete');
  await show.nth(0).click();
  await rows.nth(1).click();

  await frame.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await page.waitForFunction(() => window.businessOsSpreadsheetEvidence.commands
    .some(({ command }) => command.type === 'office.spreadsheet.commit'), null, { timeout: 30000 });
  await page.waitForTimeout(1200);
  const evidence = await page.evaluate(() => ({
    commands: window.businessOsSpreadsheetEvidence.commands.map(({ command, options }) => ({ command, options })),
    chunks: window.businessOsSpreadsheetEvidence.chunks.map((row) => ({ blob_id: row.blob_id, bytes: atob(row.data).length })),
    badge: document.querySelector('[data-spreadsheets-dirty-indicator]')?.textContent?.trim(),
  }));
  const commit = evidence.commands.find(({ command }) => command.type === 'office.spreadsheet.commit');
  const editorBlob = evidence.chunks.find(({ blob_id }) => blob_id === commit.command.payload.editor_blob_id);
  if (commit.options?.until !== 'terminal' || !editorBlob || evidence.badge !== 'Gespeichert') {
    throw new Error(`Business OS formatting commit failed: ${JSON.stringify(evidence)}`);
  }
  await page.screenshot({ path: `output/playwright/ctox-office/comparison/${feature}/business-os-mount-cell-format.png` });
  return { feature_id: feature, wrapper: 'modules/spreadsheets.mount(ctx)', command: commit.command.type, base_version_id: commit.command.payload.base_version_id, transport: commit.command.client_context?.transport, until: commit.options.until, editor_blob: editorBlob, badge: evidence.badge };
}
