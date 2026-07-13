async (page) => {
  const feature = 'document.styles-lists-numbering';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.waitForFunction(() => {
    const oracle = document.querySelector('#oracle')?.contentWindow;
    const ctox = document.querySelector('#ctox')?.contentWindow;
    const oracleStatus = document.querySelector('#oracle')?.contentDocument
      ?.querySelector('[role="status"], #status')?.textContent?.trim();
    const ctoxStatus = document.querySelector('#ctox')?.contentDocument
      ?.querySelector('[role="status"], #status')?.textContent?.trim();
    return oracle?.oracleEvidence?.comparison_config
      && ctox?.ctoxOfficeEvidence?.comparison_config
      && oracleStatus === 'document-ready'
      && ctoxStatus === 'document-ready';
  }, null, { timeout: 30000 });
  const gate = await page.evaluate(() => window.ctoxOfficeComparison.validate());
  if (!gate.valid) throw new Error(`comparison gate failed: ${gate.failures.join(', ')}`);

  const frames = page.frames();
  const oracle = frames.find((frame) =>
    frame.url().includes('127.0.0.1:8088/')
    && frame.url().includes('/web-apps/apps/documenteditor/main/index.html'));
  const ctox = frames.find((frame) =>
    frame.url().includes('/vendor/ctox-office/upstream/web-apps/apps/documenteditor/main/index.html'));
  const ctoxHost = frames.find((frame) =>
    frame.url().includes('/ctox-document-styles-lists-numbering.html'));
  if (!oracle || !ctox || !ctoxHost) throw new Error('styles/list comparison frames are missing');

  const editors = [['oracle', oracle], ['ctox', ctox]];
  const geometry = {};
  const selectMarker = async (frame, marker) => {
    await dismissNotices(frame);
    const selected = await frame.evaluate((target) => {
      const activeEditor = window.editor || window.Asc?.editor;
      const documentApi = activeEditor?.getJsApi?.()?.GetDocument?.();
      const range = documentApi?.Search?.(target)?.[0];
      return range?.Select?.() === true;
    }, marker);
    if (!selected) throw new Error(`marker range could not be selected: ${marker}`);
    await page.keyboard.press('ArrowRight');
  };
  const selectMarkerParagraph = async (frame, marker) => {
    await dismissNotices(frame);
    const selected = await frame.evaluate((target) => {
      const activeEditor = window.editor || window.Asc?.editor;
      const documentApi = activeEditor?.getJsApi?.()?.GetDocument?.();
      const range = documentApi?.Search?.(target)?.[0];
      const paragraph = range?.GetAllParagraphs?.()?.[0];
      return paragraph?.Select?.() === true;
    }, marker);
    if (!selected) throw new Error(`marker paragraph could not be selected: ${marker}`);
  };
  const openMore = async (frame) => {
    if (await frame.locator('.more-container:visible').count() === 0) {
      await frame.getByRole('button', { name: 'Mehr', exact: true }).click();
    }
  };
  const dismissNotices = async (frame) => {
    for (let attempt = 0; attempt < 10; attempt += 1) {
      const okButton = frame.getByRole('button', { name: 'OK', exact: true }).last();
      if (await okButton.isVisible().catch(() => false)) {
        await okButton.click();
        await frame.waitForTimeout(150);
        continue;
      }
      const okText = frame.getByText('OK', { exact: true }).last();
      if (await okText.isVisible().catch(() => false)) {
        await okText.click();
        await frame.waitForTimeout(150);
        continue;
      }
      break;
    }
  };
  const styleGalleryIndexes = {
    'Überschrift 1': 2,
    Zitat: 17,
  };
  const chooseStyle = async (frame, name) => {
    await dismissNotices(frame);
    await openMore(frame);
    await frame.locator('#slot-field-styles .btn.dropdown-toggle.open-menu').click();
    const index = styleGalleryIndexes[name];
    if (!Number.isInteger(index)) throw new Error(`style gallery index is not mapped: ${name}`);
    const box = await frame.locator('.menu-picker:visible .style').nth(index).boundingBox();
    if (!box) throw new Error(`style gallery tile is not visible: ${name}`);
    await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  };

  for (const [name, frame] of editors) {
    await dismissNotices(frame);
    const box = await frame.locator('#id_viewer_overlay').boundingBox();
    if (!box) throw new Error(`${name} editor canvas is missing`);
    geometry[name] = box;

    await selectMarker(frame, 'NUMBER_CONTINUE_TARGET');
    await openMore(frame);
    await frame.getByRole('button', { name: 'Nummerierung', exact: true }).last().click();
    await frame.locator('.item-multilevellist:visible').nth(4).click();
    // Re-select after list creation because the new list changes page layout.
    await selectMarker(frame, 'NUMBER_CONTINUE_TARGET');
    await dismissNotices(frame);
    await frame.locator('#id_viewer_overlay').press('ContextMenu');
    await frame.getByText('Nummerierung fortführen', { exact: true }).last().click();

    await selectMarker(frame, 'BULLET_NEST_TARGET');
    await openMore(frame);
    await frame.getByRole('button', { name: 'Einzug vergrößern (⌘+M)', exact: true }).click();
    await selectMarkerParagraph(frame, 'STYLE_QUOTE_TARGET');
    await chooseStyle(frame, 'Zitat');
    await selectMarkerParagraph(frame, 'STYLE_HEADING1_TARGET');
    await chooseStyle(frame, 'Überschrift 1');
  }

  for (const [, frame] of editors) {
    await frame.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
  }
  await page.waitForFunction(() =>
    document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.commits?.length === 1);
  const captured = await ctoxHost.evaluate(async () => {
    const bytes = await window.ctoxOfficeEvidence.savedBytes();
    const response = await fetch(
      'http://127.0.0.1:4180/capture-ctox/document.styles-lists-numbering',
      { method: 'POST', body: bytes },
    );
    return response.json();
  });
  await page.waitForTimeout(500);
  await page.screenshot({ path: `${output}/differential-terminal-styles-lists.png` });
  return {
    feature_id: feature,
    interaction: 'original-style-gallery-list-indent-numbering-continue-save',
    geometry,
    captured,
  };
}
