async (page) => {
  const feature = 'document.character-paragraph-formatting';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.waitForFunction(() => document.body.innerText.includes('Gate: Lauf gültig'));

  const frames = page.frames();
  const oracle = frames.find((frame) =>
    frame.url().includes('127.0.0.1:8088/')
    && frame.url().includes('/web-apps/apps/documenteditor/main/index.html'));
  const ctox = frames.find((frame) =>
    frame.url().includes('/vendor/ctox-office/upstream/web-apps/apps/documenteditor/main/index.html'));
  const ctoxHost = frames.find((frame) => frame.url().includes('/ctox-document-character-paragraph-formatting.html'));
  if (!oracle || !ctox || !ctoxHost) throw new Error('document formatting frames are missing');

  const editors = [['oracle', oracle], ['ctox', ctox]];
  const geometry = {};
  for (const [name, frame] of editors) {
    const notice = frame.getByRole('button', { name: 'OK', exact: true });
    if (await notice.count()) await notice.click();
    const box = await frame.locator('#id_viewer_overlay').boundingBox();
    if (!box) throw new Error(`${name} editor canvas is missing`);
    geometry[name] = box;
  }
  const selectLine = async (frame, yRatio) => {
    const overlay = frame.locator('#id_viewer_overlay');
    const box = await overlay.boundingBox();
    await overlay.click({ position: { x: box.width * 0.36, y: box.height * yRatio } });
    await page.keyboard.press('End');
    await page.keyboard.press('Shift+Home');
  };
  const placeCursor = async (frame, yRatio) => {
    const overlay = frame.locator('#id_viewer_overlay');
    const box = await overlay.boundingBox();
    await overlay.click({ position: { x: box.width * 0.36, y: box.height * yRatio } });
  };
  const openMore = async (frame) => {
    const more = frame.getByRole('button', { name: 'Mehr', exact: true });
    if (await frame.locator('.more-container:visible').count() === 0) await more.click();
  };

  for (const [, frame] of editors) {
    // Work bottom-up so font-size and line-spacing reflow cannot move a target
    // that has not been operated on yet.
    await placeCursor(frame, 0.72);
    await openMore(frame);
    await frame.getByRole('button', { name: 'Zeilenabstand', exact: true }).click();
    await frame.getByText('1.5', { exact: true }).last().click();
    await placeCursor(frame, 0.68);
    await openMore(frame);
    await frame.getByRole('button', { name: 'Einzug vergrößern (⌘+M)', exact: true }).click();
    await placeCursor(frame, 0.635);
    await openMore(frame);
    await frame.getByRole('button', { name: 'Zentriert ausrichten (⌘+E)', exact: true }).click();

    await selectLine(frame, 0.54);
    const fontColor = frame.getByRole('button', { name: 'Schriftfarbe', exact: true });
    await fontColor.last().click();
    await frame.locator('.palette-color-effect.color-953735:visible').click();
    await selectLine(frame, 0.50);
    const fontSize = frame.locator('input[aria-label="Schriftgrad"]');
    await fontSize.fill('18');
    await fontSize.press('Enter');
    await selectLine(frame, 0.46);
    await frame.getByRole('button', { name: 'Unterstrichen (⌘+U)', exact: true }).click();
    await selectLine(frame, 0.415);
    await frame.getByRole('button', { name: 'Kursiv (⌘+I)', exact: true }).click();
    await selectLine(frame, 0.37);
    await frame.getByRole('button', { name: 'Fett (⌘+B)', exact: true }).click();
    await page.waitForTimeout(500);
  }
  await page.screenshot({ path: `${output}/differential-terminal-formatting.png` });

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
    const response = await fetch('http://127.0.0.1:4180/state/document.character-paragraph-formatting');
    return response.ok && (await response.json()).saved === true;
  }, null, { timeout: 30000 });

  const ctoxEvidence = await ctoxHost.evaluate(async () => ({
    inspection: await window.ctoxOfficeEvidence.editor.inspect(),
    commits: window.ctoxOfficeEvidence.commits.map(({ bytes, version_id, reason }) => ({ bytes, version_id, reason })),
    events: window.ctoxOfficeEvidence.events.slice(-10),
  }));
  const oracleState = await page.evaluate(async () =>
    (await fetch('http://127.0.0.1:4180/state/document.character-paragraph-formatting')).json());
  return {
    feature_id: feature,
    interaction: 'original-toolbar-character-and-paragraph-formatting-save',
    geometry,
    oracle: oracleState,
    ctox: ctoxEvidence,
  };
}
