async (page) => {
  const feature = 'document.edit-save';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  const replacement = 'CTOX_EDIT_RESULT_BRAVO_42';
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.waitForFunction(() => document.body.innerText.includes('Gate: Lauf gültig'));

  const frames = page.frames();
  const oracle = frames.find((frame) =>
    frame.url().includes('127.0.0.1:8088/')
    && frame.url().includes('/web-apps/apps/documenteditor/main/index.html'));
  const ctox = frames.find((frame) =>
    frame.url().includes('/vendor/ctox-office/upstream/web-apps/apps/documenteditor/main/index.html'));
  const ctoxHost = frames.find((frame) => frame.url().includes('/ctox-document-edit-save.html'));
  if (!oracle || !ctox || !ctoxHost) throw new Error('document edit/save frames are missing');

  const editors = [['oracle', oracle], ['ctox', ctox]];
  const geometry = {};
  for (const [name, frame] of editors) {
    const notice = frame.getByRole('button', { name: 'OK', exact: true });
    if (await notice.count()) await notice.click();
    const overlay = frame.locator('#id_viewer_overlay');
    const box = await overlay.boundingBox();
    if (!box) throw new Error(`${name} editor page rectangle is missing`);
    geometry[name] = box;
    // The deterministic fixture target is located as a ratio of the measured
    // editor canvas. This remains stable when both equal-width panes resize.
    await overlay.click({ position: { x: box.width * 0.36, y: box.height * 0.40 } });
    await page.keyboard.press('End');
    await page.keyboard.press('Shift+Home');
    await page.keyboard.type(replacement);
    await frame.getByRole('button', { name: 'Speichern (⌘+S)', exact: true }).click();
    await page.waitForTimeout(750);
  }

  await page.waitForFunction(() => {
    const frame = document.querySelector('#ctox');
    return frame?.contentWindow?.ctoxOfficeEvidence?.commits?.length === 1;
  });
  await page.waitForFunction(async () => {
    const editor = document.querySelector('#ctox')?.contentWindow?.ctoxOfficeEvidence?.editor;
    return editor && (await editor.inspect()).dirty === false;
  });
  await page.waitForFunction(async () => {
    const response = await fetch('http://127.0.0.1:4180/state/document.edit-save');
    return response.ok && (await response.json()).saved === true;
  }, null, { timeout: 30000 });

  await page.screenshot({ path: `${output}/differential-after-edit-save.png` });
  const ctoxEvidence = await ctoxHost.evaluate(async () => ({
    inspection: await window.ctoxOfficeEvidence.editor.inspect(),
    commits: window.ctoxOfficeEvidence.commits.map(({ bytes, version_id, reason }) => ({ bytes, version_id, reason })),
    events: window.ctoxOfficeEvidence.events.slice(-8),
  }));
  const oracleState = await page.evaluate(async () =>
    (await fetch('http://127.0.0.1:4180/state/document.edit-save')).json());
  return {
    feature_id: feature,
    interaction: 'measured-canvas-keyboard-replace-and-real-toolbar-save',
    replacement,
    geometry,
    oracle: oracleState,
    ctox: ctoxEvidence,
  };
}
