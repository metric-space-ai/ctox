async (page) => {
  const feature = 'document.undo-clipboard-keyboard';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.waitForFunction(() => document.body.innerText.includes('Gate: Lauf gültig'));

  const frames = page.frames();
  const oracle = frames.find((frame) =>
    frame.url().includes('127.0.0.1:8088/')
    && frame.url().includes('/web-apps/apps/documenteditor/main/index.html'));
  const ctox = frames.find((frame) =>
    frame.url().includes('/vendor/ctox-office/upstream/web-apps/apps/documenteditor/main/index.html'));
  const ctoxHost = frames.find((frame) => frame.url().includes('/ctox-document-undo-clipboard-keyboard.html'));
  if (!oracle || !ctox || !ctoxHost) throw new Error('document undo/clipboard frames are missing');

  const editors = [['oracle', oracle], ['ctox', ctox]];
  const geometry = {};
  for (const [name, frame] of editors) {
    const notice = frame.getByRole('button', { name: 'OK', exact: true });
    if (await notice.count()) await notice.click();
    const box = await frame.locator('#id_viewer_overlay').boundingBox();
    if (!box) throw new Error(`${name} editor canvas is missing`);
    geometry[name] = box;
  }
  const clickFixtureLine = async (frame, yRatio) => {
    const overlay = frame.locator('#id_viewer_overlay');
    const box = await overlay.boundingBox();
    await overlay.click({ position: { x: box.width * 0.36, y: box.height * yRatio } });
  };

  for (const [, frame] of editors) {
    await clickFixtureLine(frame, 0.39);
    await page.keyboard.press('End');
    await page.keyboard.type('_ONE');
    await page.waitForTimeout(300);
  }
  await page.screenshot({ path: `${output}/differential-after-input.png` });

  for (const [, frame] of editors) {
    await clickFixtureLine(frame, 0.39);
    await page.keyboard.press('End');
    await page.keyboard.press('Meta+Z');
    await page.waitForTimeout(300);
  }
  await page.screenshot({ path: `${output}/differential-after-keyboard-undo.png` });

  for (const [, frame] of editors) {
    await clickFixtureLine(frame, 0.39);
    await page.keyboard.press('End');
    await page.keyboard.press('Meta+Y');
    await page.waitForTimeout(300);
  }
  await page.screenshot({ path: `${output}/differential-after-keyboard-redo.png` });

  for (const [, frame] of editors) {
    await frame.getByRole('button', { name: 'Rückgängig (⌘+Z)', exact: true }).click();
    await page.waitForTimeout(300);
  }
  await page.screenshot({ path: `${output}/differential-after-toolbar-undo.png` });
  for (const [, frame] of editors) {
    await frame.getByRole('button', { name: 'Wiederholen (⌘+Y)', exact: true }).click();
    await page.waitForTimeout(300);
  }

  for (const [, frame] of editors) {
    await clickFixtureLine(frame, 0.39);
    await page.keyboard.press('End');
    await page.keyboard.press('Shift+Home');
    await page.keyboard.press('Meta+C');
    await clickFixtureLine(frame, 0.48);
    await page.keyboard.press('End');
    await page.keyboard.press('Shift+Home');
    await page.keyboard.press('Meta+V');
    await page.keyboard.press('End');
    await page.keyboard.press('Shift+Home');
    await page.keyboard.press('Meta+X');
    await page.keyboard.press('Meta+V');
  }
  await page.screenshot({ path: `${output}/differential-after-copy-cut-paste.png` });

  for (const [, frame] of editors) {
    await frame.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
    await page.waitForTimeout(750);
  }
  await page.waitForFunction(() =>
    document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.commits?.length === 1);
  await page.waitForFunction(async () => {
    const editor = document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.editor;
    return editor && (await editor.inspect()).dirty === false;
  });
  await page.waitForFunction(async () => {
    const response = await fetch('http://127.0.0.1:4180/state/document.undo-clipboard-keyboard');
    return response.ok && (await response.json()).saved === true;
  }, null, { timeout: 30000 });
  await page.screenshot({ path: `${output}/differential-after-save.png` });

  const ctoxEvidence = await ctoxHost.evaluate(async () => ({
    inspection: await window.ctoxOfficeEvidence.editor.inspect(),
    commits: window.ctoxOfficeEvidence.commits.map(({ bytes, version_id, reason }) => ({ bytes, version_id, reason })),
    events: window.ctoxOfficeEvidence.events.slice(-10),
  }));
  const oracleState = await page.evaluate(async () =>
    (await fetch('http://127.0.0.1:4180/state/document.undo-clipboard-keyboard')).json());
  return {
    feature_id: feature,
    interaction: 'keyboard-and-original-toolbar-undo-redo-copy-cut-paste-save',
    geometry,
    oracle: oracleState,
    ctox: ctoxEvidence,
  };
}
