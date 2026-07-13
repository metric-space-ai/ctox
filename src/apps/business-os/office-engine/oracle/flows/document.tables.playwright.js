async (page) => {
  const feature = 'document.tables';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.evaluate((featureId) =>
    fetch(`http://127.0.0.1:4180/reset/${featureId}`, { method: 'POST' }),
  feature);
  await page.waitForFunction(() => {
    const oracle = document.querySelector('#oracle')?.contentWindow;
    const ctox = document.querySelector('#ctox')?.contentWindow;
    const oracleStatus = document.querySelector('#oracle')?.contentDocument
      ?.querySelector('[role="status"], #oracle-status')?.textContent?.trim();
    const ctoxStatus = document.querySelector('#ctox')?.contentDocument
      ?.querySelector('[role="status"], #status')?.textContent?.trim();
    return oracle?.oracleEvidence?.comparison_config
      && ctox?.ctoxOfficeEvidence?.comparison_config
      && oracleStatus === 'document-ready'
      && ctoxStatus === 'document-ready';
  }, null, { timeout: 60000 });
  const gate = await page.evaluate(() => window.ctoxOfficeComparison.validate());
  if (!gate.valid) throw new Error(`comparison gate failed: ${gate.failures.join(', ')}`);

  const frames = page.frames();
  const oracle = frames.find((frame) =>
    frame.url().includes('127.0.0.1:8088/')
    && frame.url().includes('/web-apps/apps/documenteditor/main/index.html'));
  const ctox = frames.find((frame) =>
    frame.url().includes('/vendor/ctox-office/upstream/web-apps/apps/documenteditor/main/index.html'));
  const ctoxHost = frames.find((frame) => frame.url().includes('/ctox-document-tables.html'));
  if (!oracle || !ctox || !ctoxHost) throw new Error('table comparison frames are missing');

  const selectMarker = async (frame, marker) => {
    const selected = await frame.evaluate((target) => {
      const api = (window.editor || window.Asc?.editor)?.getJsApi?.();
      const range = api?.GetDocument?.()?.Search?.(target)?.[0];
      return range?.Select?.() === true;
    }, marker);
    if (!selected) throw new Error(`marker range could not be selected: ${marker}`);
    await page.keyboard.press('ArrowRight');
  };
  const openTableContextMenu = async (frame, marker) => {
    await page.keyboard.press('Escape').catch(() => {});
    await selectMarker(frame, marker);
    const box = await frame.locator('#id_viewer_overlay').boundingBox();
    if (!box) throw new Error('editor overlay is missing');
    await page.mouse.click(box.x + box.width * 0.35, box.y + box.height * 0.35, { button: 'right' });
    await page.waitForTimeout(250);
  };
  const clickVisibleMenuText = async (frame, text) => {
    const clicked = await frame.evaluate((target) => {
      const item = Array.from(document.querySelectorAll('a.menu-item')).find((element) => {
        const rect = element.getBoundingClientRect();
        return element.textContent?.trim() === target && rect.width > 0 && rect.height > 0;
      });
      item?.click();
      return Boolean(item);
    }, text);
    if (!clicked) throw new Error(`visible menu item is missing: ${text}`);
    await page.waitForTimeout(700);
  };
  const clickTableInsertCommand = async (frame, marker, command) => {
    await openTableContextMenu(frame, marker);
    const tableInsertId = await frame.evaluate(() => {
      const item = Array.from(document.querySelectorAll('a.menu-item')).find((element) => {
        const rect = element.getBoundingClientRect();
        return element.textContent?.trim() === 'Einfügen'
          && element.parentElement?.classList.contains('dropdown-submenu')
          && rect.width > 0
          && rect.height > 0;
      });
      return item?.id || null;
    });
    if (!tableInsertId) throw new Error('table insert context-menu item is missing');
    await frame.locator(`#${tableInsertId}`).hover();
    await page.waitForTimeout(350);
    await clickVisibleMenuText(frame, command);
  };

  const mutateTables = async (frame) => {
    await clickTableInsertCommand(frame, 'TABLE_ROW_ANCHOR', 'Zeile unterhalb');
    await clickTableInsertCommand(frame, 'TABLE_COLUMN_ANCHOR', 'Spalte nach rechts');
    await selectMarker(frame, 'TABLE_EDIT_TARGET');
    await page.keyboard.type('TABLE_EDITED_VALUE');
    return frame.evaluate(() => {
    const api = (window.editor || window.Asc?.editor)?.getJsApi?.();
    const documentApi = api?.GetDocument?.();
    if (!api || !documentApi) throw new Error('Document Builder API is unavailable');
    const shape = (table) => Array.from({ length: table.GetRowsCount() }, (_, rowIndex) => {
      let count = 0;
      for (let cellIndex = 0; cellIndex < 12; cellIndex += 1) {
        if (table.GetCell(rowIndex, cellIndex)) count += 1;
      }
      return count;
    });
    const tables = documentApi.GetAllTables();
    if (tables.length < 2) throw new Error(`expected at least 2 tables, got ${tables.length}`);

    const main = tables[0];
    const mergeTable = documentApi.GetAllTables()[1];
    const merged = mergeTable.MergeCells([mergeTable.GetCell(0, 0), mergeTable.GetCell(0, 1)]);
    const splitResult = merged?.Split?.(2, 2) ?? mergeTable.Split(merged, 2, 2);
    const text = documentApi.GetText();
    return {
      tableCount: documentApi.GetAllTables().length,
      mainShape: shape(main),
      mergeShape: shape(mergeTable),
      splitResult: splitResult === true,
      hasEdited: text.includes('TABLE_EDITED_VALUE'),
      hasNestedA1: text.includes('NESTED_A1'),
      hasNestedB2: text.includes('NESTED_B2'),
      modified: (window.editor || window.Asc?.editor)?.isDocumentModified?.() === true,
      canSave: (window.editor || window.Asc?.editor)?.asc_isDocumentCanSave?.() === true,
    };
    });
  };

  const terminal = {
    oracle: await mutateTables(oracle),
    ctox: await mutateTables(ctox),
  };
  for (const [name, state] of Object.entries(terminal)) {
    if (state.tableCount !== 4) throw new Error(`${name} table count mismatch: ${state.tableCount}`);
    if (state.mainShape.join(',') !== '4,4,4,4') throw new Error(`${name} main table shape mismatch: ${state.mainShape}`);
    if (state.mergeShape.join(',') !== '2,2,2') throw new Error(`${name} merge/split shape mismatch: ${state.mergeShape}`);
    if (!state.hasEdited || !state.hasNestedA1 || !state.hasNestedB2) {
      throw new Error(`${name} terminal table markers missing: ${JSON.stringify(state)}`);
    }
  }

  await oracle.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await ctox.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  await page.waitForFunction(() =>
    document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.commits?.length === 1,
  null, { timeout: 30000 });
  const captured = await ctoxHost.evaluate(async () => {
    const bytes = await window.ctoxOfficeEvidence.savedBytes();
    const response = await fetch(
      'http://127.0.0.1:4180/capture-ctox/document.tables',
      { method: 'POST', body: bytes },
    );
    return response.json();
  });
  let oracleState = null;
  for (let attempt = 0; attempt < 75; attempt += 1) {
    oracleState = await page.evaluate(async () =>
      (await fetch('http://127.0.0.1:4180/state/document.tables')).json());
    if (oracleState.saved === true) break;
    await page.waitForTimeout(1000);
  }
  if (oracleState?.saved !== true) {
    throw new Error(`Oracle terminal save callback missing: state=${JSON.stringify(oracleState)}`);
  }

  await page.screenshot({ path: `${output}/differential-terminal-tables.png` });
  return {
    feature_id: feature,
    interaction: 'real-documenteditor-context-menu-row-column-keyboard-edit-runtime-merge-split-save',
    terminal,
    captured,
    oracle_state: oracleState,
  };
}
