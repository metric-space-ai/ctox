async (page) => {
  const feature = 'spreadsheet.sort-filter-tables';
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.waitForFunction(() => document.querySelector('#status')?.textContent === 'document-ready', null, { timeout: 60000 });
  const frame = page.frames().find((value) => value.url().includes('/vendor/ctox-office/upstream/') && value.url().includes('/spreadsheeteditor/main/index.html'));
  if (!frame) throw new Error('Business OS did not mount the original SpreadsheetEditor ESM frame');
  if (!(await frame.getByRole('tab').allInnerTexts()).includes('Tabellen-Design')) throw new Error('Business OS did not materialize RevenueTable in CTOX Spreadsheets');
  const select = async (reference) => {
    const name = frame.locator('#ce-cell-name');
    await name.fill(reference);
    await name.press('Enter');
    await page.waitForTimeout(150);
  };
  const values = async (references) => {
    const result = [];
    for (const reference of references) {
      await select(reference);
      result.push(await frame.locator('#ce-cell-content').inputValue());
    }
    return result;
  };
  const openHeaderFilter = async (reference) => {
    await select(reference);
    const cell = await frame.evaluate(() => {
      const rect = Asc.editor.asc_getActiveCellCoord();
      return { x: rect.asc_getX(), y: rect.asc_getY(), width: rect.asc_getWidth(), height: rect.asc_getHeight() };
    });
    await frame.locator('#ws-canvas-overlay').click({
      position: { x: cell.x + cell.width - 8, y: cell.y + cell.height / 2 },
      force: true,
      timeout: 5000,
    });
    await frame.getByRole('dialog').waitFor({ state: 'visible', timeout: 5000 });
    return cell;
  };
  const revenueHeader = await openHeaderFilter('C1');
  await frame.getByRole('listitem').filter({ hasText: 'Absteigend sortieren' }).click();
  await page.waitForTimeout(450);
  const sorted = await values(['C2', 'C3', 'C4', 'C5', 'C6']);
  if (sorted.join(',') !== '420,310,240,180,120') throw new Error(`Business OS descending sort failed: ${sorted}`);
  const regionHeader = await openHeaderFilter('A1');
  const dialog = frame.getByRole('dialog');
  const treeitems = dialog.getByRole('treeitem');
  await treeitems.first().click();
  await treeitems.filter({ hasText: 'North' }).click();
  await dialog.getByRole('button', { name: 'OK', exact: true }).click();
  await page.waitForTimeout(450);
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
  if (commit.options?.until !== 'terminal' || !editorBlob || evidence.badge !== 'Gespeichert') throw new Error(`Business OS sort/filter commit failed: ${JSON.stringify(evidence)}`);
  await page.screenshot({ path: `output/playwright/ctox-office/comparison/${feature}/business-os-mount-sort-filter.png` });
  return {
    feature_id: feature,
    wrapper: 'modules/spreadsheets.mount(ctx)',
    command: commit.command.type,
    base_version_id: commit.command.payload.base_version_id,
    transport: commit.command.client_context?.transport,
    until: commit.options.until,
    editor_blob: editorBlob,
    badge: evidence.badge,
    sorted,
    measured_headers: { revenue: revenueHeader, region: regionHeader },
  };
}
