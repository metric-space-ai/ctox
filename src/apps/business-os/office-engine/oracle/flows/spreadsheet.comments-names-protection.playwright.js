async (page) => {
  const feature = 'spreadsheet.comments-names-protection';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.waitForFunction(
    () => window.ctoxOfficeComparison?.lastValidation?.valid === true,
    null,
    { timeout: 90000 },
  );
  const oracle = page.frames().find((frame) =>
    frame.url().includes('127.0.0.1:8088/')
      && frame.url().includes('/spreadsheeteditor/main/index.html'));
  const ctox = page.frames().find((frame) =>
    frame.url().includes('/vendor/ctox-office/upstream/')
      && frame.url().includes('/spreadsheeteditor/main/index.html'));
  const ctoxHost = page.frames().find((frame) =>
    frame.url().includes('/ctox-spreadsheet-comments-names-protection.html'));
  if (!oracle || !ctox || !ctoxHost) throw new Error('comments/names/protection comparison frames are missing');
  const gate = await page.evaluate(() => window.ctoxOfficeComparison.validate());
  if (!gate.valid) throw new Error(`comparison gate failed: ${gate.failures.join(', ')}`);

  const initial = {};
  const terminal = {};
  for (const [name, frame] of [['oracle', oracle], ['ctox', ctox]]) {
    initial[name] = await frame.evaluate(() => ({
      names: Asc.editor.asc_getDefinedNames(Asc.c_oAscGetDefinedNamesList.All)
        .map((item) => ({ name: item.asc_getName(), ref: item.asc_getRef(), scope: item.asc_getScope() })),
      sheetProtected: Asc.editor.asc_isProtectedSheet(),
      workbookProtected: Asc.editor.asc_isProtectedWorkbook(),
      comments: Asc.editor.pluginMethod_GetAllComments(),
    }));
    if (!initial[name].sheetProtected || !initial[name].workbookProtected) {
      throw new Error(`${name} did not import both protection levels`);
    }
    if (initial[name].comments[0]?.Data?.Text !== 'CTOX_EXISTING_CELL_COMMENT') {
      throw new Error(`${name} did not import the classic B4 comment`);
    }

    await frame.getByRole('tab', { name: 'Schutz', exact: true }).click();
    await frame.locator('button').filter({ hasText: 'Arbeitsmappe' }).click();
    await frame.locator('button').filter({ hasText: /Blatt\s*schützen/ }).click();

    await frame.getByRole('tab', { name: 'Formel', exact: true }).click();
    // At half-screen width Euro-Office moves this original control outside the
    // visible toolbar row. Trigger its own button/menu handlers, then continue
    // through the visible upstream Name Manager and edit dialog.
    await frame.evaluate(() => {
      document.querySelector('#id-toolbar-btn-insertrange button')?.click();
      document.querySelector('#id-toolbar-btn-insertrange a')?.click();
    });
    const manager = frame.locator('#window-name-manager');
    await manager.waitFor({ state: 'visible' });
    if (!await manager.getByText('CTOX_Amount', { exact: true }).count()) {
      throw new Error(`${name} Name Manager did not show CTOX_Amount`);
    }
    await manager.getByRole('button', { name: 'Bearbeiten', exact: true }).click();
    const editDialog = frame.getByRole('dialog').filter({ hasText: 'Name bearbeiten' });
    const nameInput = editDialog.getByRole('textbox', { name: 'Definierter Name' });
    await nameInput.fill('CTOX_Amount_Reviewed');
    await editDialog.getByRole('button', { name: 'OK', exact: true }).click();
    await manager.getByRole('button', { name: 'Schließen', exact: true }).click();
    await page.waitForTimeout(400);

    const cellName = frame.locator('#ce-cell-name');
    await cellName.fill('C4');
    await cellName.press('Enter');
    const commentsButton = frame.locator('#left-btn-comments');
    if (await commentsButton.getAttribute('aria-pressed') !== 'true') await commentsButton.click();
    await frame.getByRole('button', { name: 'Kommentar hinzufügen', exact: true }).click();
    const commentEditor = frame.locator('.user-comment-item textarea.msg-reply').last();
    await commentEditor.fill('CTOX_ADDED_CELL_COMMENT');
    await frame.locator('.user-comment-item').last().getByRole('button', { name: 'Hinzufügen', exact: true }).click();

    terminal[name] = await frame.evaluate(() => ({
      names: Asc.editor.asc_getDefinedNames(Asc.c_oAscGetDefinedNamesList.All)
        .map((item) => ({ name: item.asc_getName(), ref: item.asc_getRef(), scope: item.asc_getScope() })),
      sheetProtected: Asc.editor.asc_isProtectedSheet(),
      workbookProtected: Asc.editor.asc_isProtectedWorkbook(),
      comments: Asc.editor.pluginMethod_GetAllComments(),
    }));
    if (terminal[name].sheetProtected || terminal[name].workbookProtected) {
      throw new Error(`${name} protection toggle did not reach the runtime`);
    }
    if (!terminal[name].names.some((item) => item.name === 'CTOX_Amount_Reviewed')) {
      throw new Error(`${name} defined name edit did not reach the runtime`);
    }
    if (!terminal[name].comments.some((item) => item.Data?.Text === 'CTOX_ADDED_CELL_COMMENT')) {
      throw new Error(`${name} comment add did not reach the runtime`);
    }
  }

  await page.screenshot({ path: `${output}/differential-terminal-before-save.png` });
  for (const frame of [oracle, ctox]) {
    await frame.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  }
  await page.waitForFunction(
    () => document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.state?.commits?.length === 1,
    null,
    { timeout: 30000 },
  );
  await page.waitForFunction(async () => {
    const response = await fetch(`http://127.0.0.1:4180/state/spreadsheet.comments-names-protection?t=${Date.now()}`, { cache: 'no-store' });
    return response.ok && (await response.json()).saved === true;
  }, null, { timeout: 60000 });
  const capture = await ctoxHost.evaluate(async () => {
    const bytes = await window.ctoxOfficeEvidence.savedBytes();
    return (await fetch('http://127.0.0.1:4180/capture-ctox/spreadsheet.comments-names-protection', {
      method: 'POST', body: bytes,
    })).json();
  });
  const ctoxEvidence = await ctoxHost.evaluate(async () => ({
    inspection: await window.ctoxOfficeEvidence.editor.inspect(),
    commits: window.ctoxOfficeEvidence.state.commits,
  }));
  const oracleState = await page.evaluate(async () =>
    (await fetch(`http://127.0.0.1:4180/state/spreadsheet.comments-names-protection?t=${Date.now()}`, { cache: 'no-store' })).json());
  if (ctoxEvidence.inspection.dirty || !oracleState.saved) throw new Error('save evidence is incomplete');
  return {
    feature_id: feature,
    interaction: 'original-comment-editor-name-manager-and-protection-toolbar',
    gate,
    initial,
    terminal,
    capture,
    oracle: oracleState,
    ctox: ctoxEvidence,
  };
}
