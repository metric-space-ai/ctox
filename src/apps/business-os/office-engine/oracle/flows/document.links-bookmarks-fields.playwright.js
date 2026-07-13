async (page) => {
  const feature = 'document.links-bookmarks-fields';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.evaluate((featureId) =>
    fetch(`http://127.0.0.1:4180/reset/${featureId}`, { method: 'POST' }),
  feature);
  await page.waitForFunction(() => document.body.innerText.includes('Gate: Lauf gültig'), null, { timeout: 60000 });
  const gate = await page.evaluate(() => window.ctoxOfficeComparison.validate());
  if (!gate.valid) throw new Error(`comparison gate failed: ${gate.failures.join(', ')}`);

  const frames = page.frames();
  const oracle = frames.find((frame) =>
    frame.url().includes('127.0.0.1:8088/')
    && frame.url().includes('/web-apps/apps/documenteditor/main/index.html'));
  const ctox = frames.find((frame) =>
    frame.url().includes('/vendor/ctox-office/upstream/web-apps/apps/documenteditor/main/index.html'));
  const ctoxHost = frames.find((frame) => frame.url().includes('/ctox-document-links-bookmarks-fields.html'));
  if (!oracle || !ctox || !ctoxHost) throw new Error('links/bookmarks/fields comparison frames are missing');

  const closeNotice = async (frame) => {
    const notice = frame.getByRole('button', { name: 'OK', exact: true });
    if (await notice.count()) await notice.click();
  };
  await closeNotice(oracle);
  await closeNotice(ctox);

  const inspectDocument = (frame) => frame.evaluate(() => {
    const editorApi = window.editor || window.Asc?.editor;
    const documentApi = editorApi?.getJsApi?.()?.GetDocument?.();
    if (!editorApi || !documentApi) throw new Error('Document Builder API is unavailable');
    return {
      text: documentApi.GetText?.() || '',
      bookmarks: documentApi.GetAllBookmarksNames?.() || [],
      has_fields: editorApi.asc_HaveFields?.() === true,
      modified: editorApi.isDocumentModified?.() === true,
      can_save: editorApi.asc_isDocumentCanSave?.() === true,
    };
  });
  const assertContains = (value, marker, label) => {
    if (!String(value || '').includes(marker)) throw new Error(`${label} missing ${marker}`);
  };
  const assertInitial = (name, state) => {
    for (const marker of [
      'LINK_CREATE_TARGET', 'CTOX_EXISTING_LINK', 'BOOKMARK_CREATE_TARGET',
      'CTOX_EXISTING_BOOKMARK', 'NUMPAGES_FIELD_TARGET: 99',
      'PRESERVE_LINKS_BOOKMARKS_FIELDS_UNRELATED_B73A',
    ]) assertContains(state.text, marker, `${name} initial text`);
    if (!state.bookmarks.includes('ctox_existing_bookmark')) {
      throw new Error(`${name} initial bookmark is missing: ${JSON.stringify(state.bookmarks)}`);
    }
    if (!state.has_fields) throw new Error(`${name} initial NUMPAGES field is missing`);
  };

  const initial = { oracle: await inspectDocument(oracle), ctox: await inspectDocument(ctox) };
  assertInitial('oracle', initial.oracle);
  assertInitial('ctox', initial.ctox);
  await page.screenshot({ path: `${output}/differential-initial.png` });

  const mutate = async (name, frame) => {
    const state = await frame.evaluate(async () => {
      const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const editorApi = window.editor || window.Asc?.editor;
      const documentApi = editorApi?.getJsApi?.()?.GetDocument?.();
      const linkTarget = documentApi?.Search?.('LINK_CREATE_TARGET')?.[0];
      if (!linkTarget?.Select || typeof editorApi?.add_Hyperlink !== 'function') {
        throw new Error('Document hyperlink API is unavailable');
      }
      linkTarget.Select();
      const hyperlink = new window.Asc.CHyperlinkProperty();
      hyperlink.put_Value('https://ctox.dev/office-oracle');
      hyperlink.put_Text('CTOX_EXTERNAL_LINK');
      hyperlink.put_ToolTip('https://ctox.dev/office-oracle');
      editorApi.add_Hyperlink(hyperlink);

      const bookmarkTarget = documentApi.Search('BOOKMARK_CREATE_TARGET')?.[0];
      const bookmarks = editorApi.asc_GetBookmarksManager?.();
      if (!bookmarkTarget?.Select || typeof bookmarks?.asc_AddBookmark !== 'function') {
        throw new Error('Document bookmark API is unavailable');
      }
      bookmarkTarget.Select();
      bookmarks.asc_AddBookmark('ctox_oracle_bookmark');

      if (typeof documentApi.UpdateAllFields !== 'function') {
        throw new Error('Document field update API is unavailable');
      }
      documentApi.UpdateAllFields();
      await sleep(750);
      return {
        text: documentApi.GetText?.() || '',
        bookmarks: documentApi.GetAllBookmarksNames?.() || [],
        has_fields: editorApi.asc_HaveFields?.() === true,
        modified: editorApi.isDocumentModified?.() === true,
        can_save: editorApi.asc_isDocumentCanSave?.() === true,
      };
    });
    for (const marker of [
      'CTOX_EXTERNAL_LINK', 'CTOX_EXISTING_LINK', 'BOOKMARK_CREATE_TARGET',
      'CTOX_EXISTING_BOOKMARK', 'NUMPAGES_FIELD_TARGET: 1',
      'PRESERVE_LINKS_BOOKMARKS_FIELDS_UNRELATED_B73A',
    ]) assertContains(state.text, marker, `${name} terminal text`);
    for (const bookmark of ['ctox_oracle_bookmark', 'ctox_existing_bookmark']) {
      if (!state.bookmarks.includes(bookmark)) {
        throw new Error(`${name} bookmark ${bookmark} is missing: ${JSON.stringify(state.bookmarks)}`);
      }
    }
    if (!state.modified || !state.can_save || !state.has_fields) {
      throw new Error(`${name} terminal editor state mismatch: ${JSON.stringify(state)}`);
    }
    return state;
  };

  const mutation = { oracle: await mutate('oracle', oracle), ctox: await mutate('ctox', ctox) };
  await page.screenshot({ path: `${output}/differential-terminal-before-save.png` });

  await oracle.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await ctox.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await page.waitForFunction(() =>
    document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.commits?.length === 1,
  null, { timeout: 30000 });
  const captured = await ctoxHost.evaluate(async () => {
    const bytes = await window.ctoxOfficeEvidence.savedBytes();
    const response = await fetch(
      'http://127.0.0.1:4180/capture-ctox/document.links-bookmarks-fields',
      { method: 'POST', body: bytes },
    );
    return response.json();
  });

  let oracleState = null;
  for (let attempt = 0; attempt < 75; attempt += 1) {
    oracleState = await page.evaluate(async () =>
      (await fetch('http://127.0.0.1:4180/state/document.links-bookmarks-fields')).json());
    if (oracleState.saved === true) break;
    await page.waitForTimeout(1000);
  }
  if (oracleState?.saved !== true) {
    throw new Error(`Oracle terminal save callback missing: state=${JSON.stringify(oracleState)}`);
  }
  await page.waitForTimeout(5000);
  const terminal = { oracle: await inspectDocument(oracle), ctox: await inspectDocument(ctox) };
  const capsule = await ctoxHost.evaluate(() => window.ctoxOfficeEvidence.editor.inspect());
  if (capsule.dirty !== false) {
    throw new Error(`CTOX ESM capsule did not acknowledge save: ${JSON.stringify(capsule)}`);
  }
  return {
    feature_id: feature,
    interaction: 'real-documenteditor-hyperlink-bookmark-field-save-capture',
    initial,
    mutation,
    terminal,
    capsule,
    captured,
    oracle_state: oracleState,
  };
}
