async (page) => {
  const feature = 'spreadsheet.comments-names-protection';
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
  const initial = await frame.evaluate(() => ({
    sheetProtected: Asc.editor.asc_isProtectedSheet(),
    workbookProtected: Asc.editor.asc_isProtectedWorkbook(),
    comments: Asc.editor.pluginMethod_GetAllComments(),
    names: Asc.editor.asc_getDefinedNames(Asc.c_oAscGetDefinedNamesList.All).map((item) => item.asc_getName()),
  }));
  if (!initial.sheetProtected || !initial.workbookProtected || initial.comments.length !== 1) {
    throw new Error(`Business OS initial semantic state differs: ${JSON.stringify(initial)}`);
  }

  await frame.getByRole('tab', { name: 'Schutz', exact: true }).click();
  await frame.locator('button').filter({ hasText: 'Arbeitsmappe' }).click();
  await frame.locator('button').filter({ hasText: /Blatt\s*schützen/ }).click();
  await frame.getByRole('tab', { name: 'Formel', exact: true }).click();
  await frame.evaluate(() => {
    document.querySelector('#id-toolbar-btn-insertrange button')?.click();
    document.querySelector('#id-toolbar-btn-insertrange a')?.click();
  });
  const manager = frame.locator('#window-name-manager');
  await manager.getByRole('button', { name: 'Bearbeiten', exact: true }).click();
  const editDialog = frame.getByRole('dialog').filter({ hasText: 'Name bearbeiten' });
  await editDialog.getByRole('textbox', { name: 'Definierter Name' }).fill('CTOX_Amount_Reviewed');
  await editDialog.getByRole('button', { name: 'OK', exact: true }).click();
  await manager.getByRole('button', { name: 'Schließen', exact: true }).click();

  const cellName = frame.locator('#ce-cell-name');
  await cellName.fill('C4');
  await cellName.press('Enter');
  const commentsButton = frame.locator('#left-btn-comments');
  if (await commentsButton.getAttribute('aria-pressed') !== 'true') await commentsButton.click();
  await frame.getByRole('button', { name: 'Kommentar hinzufügen', exact: true }).click();
  await frame.locator('.user-comment-item textarea.msg-reply').last().fill('CTOX_ADDED_CELL_COMMENT');
  await frame.locator('.user-comment-item').last().getByRole('button', { name: 'Hinzufügen', exact: true }).click();
  const terminal = await frame.evaluate(() => ({
    sheetProtected: Asc.editor.asc_isProtectedSheet(),
    workbookProtected: Asc.editor.asc_isProtectedWorkbook(),
    comments: Asc.editor.pluginMethod_GetAllComments(),
    names: Asc.editor.asc_getDefinedNames(Asc.c_oAscGetDefinedNamesList.All).map((item) => item.asc_getName()),
  }));
  if (terminal.sheetProtected || terminal.workbookProtected
      || !terminal.names.includes('CTOX_Amount_Reviewed')
      || !terminal.comments.some((item) => item.Data?.Text === 'CTOX_ADDED_CELL_COMMENT')) {
    throw new Error(`Business OS terminal semantic state differs: ${JSON.stringify(terminal)}`);
  }

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
    throw new Error(`Business OS comments/names/protection commit failed: ${JSON.stringify(evidence)}`);
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
    initial,
    terminal,
  };
}
