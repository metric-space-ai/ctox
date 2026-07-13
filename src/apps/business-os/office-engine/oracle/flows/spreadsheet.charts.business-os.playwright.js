async (page) => {
  const feature = 'spreadsheet.charts';
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.waitForFunction(
    () => document.querySelector('#status')?.textContent === 'document-ready',
    null,
    { timeout: 60000 },
  );
  const frame = page.frames().find((value) =>
    value.url().includes('/vendor/ctox-office/upstream/')
      && value.url().includes('/spreadsheeteditor/main/index.html'));
  if (!frame) throw new Error('Business OS did not mount the original SpreadsheetEditor ESM frame');

  const cellSettings = frame.getByRole('button', { name: 'Zelleneinstellungen' });
  if (await cellSettings.getAttribute('aria-pressed') === 'true') await cellSettings.click();
  const canvas = frame.locator('#ws-canvas');
  const box = await canvas.boundingBox();
  if (!box) throw new Error('Spreadsheet canvas geometry is unavailable');
  const point = { x: box.x + box.width * 0.78, y: box.y + box.height * 0.38 };
  await page.mouse.click(point.x, point.y);
  if (await frame.locator('#ce-cell-name').inputValue() !== 'CTOX Revenue Chart') {
    throw new Error('Measured Business OS chart selection did not select the original chart object');
  }
  await frame.getByRole('button', { name: 'Diagrammeinstellungen' }).click();
  await frame.getByRole('spinbutton', { name: 'Breite' }).fill('14');
  await frame.getByRole('spinbutton', { name: 'Höhe' }).fill('8');
  await page.keyboard.press('Enter');
  const tip = frame.getByText('OK', { exact: true });
  if (await tip.count()) await tip.first().click();
  await frame.getByRole('listitem', { name: 'Stil 2' }).click();
  await frame.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();

  await page.waitForFunction(
    () => window.businessOsSpreadsheetEvidence.commands.some(({ command }) => command.type === 'office.spreadsheet.commit'),
    null,
    { timeout: 30000 },
  );
  await page.waitForTimeout(1200);
  const evidence = await page.evaluate(() => ({
    commands: window.businessOsSpreadsheetEvidence.commands.map(({ command, options }) => ({ command, options })),
    chunks: window.businessOsSpreadsheetEvidence.chunks.map((row) => ({ blob_id: row.blob_id, bytes: atob(row.data).length })),
    badge: document.querySelector('[data-spreadsheets-dirty-indicator]')?.textContent?.trim(),
  }));
  const commit = evidence.commands.find(({ command }) => command.type === 'office.spreadsheet.commit');
  const editorBlob = evidence.chunks.find(({ blob_id }) => blob_id === commit.command.payload.editor_blob_id);
  if (commit.options?.until !== 'terminal' || !editorBlob || evidence.badge !== 'Gespeichert') {
    throw new Error(`Business OS chart commit failed: ${JSON.stringify(evidence)}`);
  }
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
    measured_canvas: box,
    selection_point: point,
  };
}
