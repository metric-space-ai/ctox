async (page) => {
  const feature = 'document.open-render-zoom';
  const output = `output/playwright/ctox-office/comparison/${feature}`;
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.waitForFunction(() => document.body.innerText.includes('Gate: Lauf gültig'));

  const frames = page.frames();
  const oracle = frames.find((frame) =>
    frame.url().includes('127.0.0.1:8088/')
    && frame.url().includes('/web-apps/apps/documenteditor/main/index.html'));
  const ctox = frames.find((frame) =>
    frame.url().includes('/vendor/ctox-office/upstream/web-apps/apps/documenteditor/main/index.html'));
  if (!oracle || !ctox) throw new Error('document editor frames are missing');

  const editors = [
    ['oracle', oracle],
    ['ctox', ctox],
  ];
  const status = async (frame) =>
    (await frame.locator('body').innerText()).match(/Seite \d von 3/)?.[0];
  const assertEqual = (actual, expected, label) => {
    if (actual !== expected) throw new Error(`${label}: expected ${expected}, got ${actual}`);
  };
  const pageDownThreeTimes = async () => {
    for (let press = 0; press < 3; press += 1) {
      await page.keyboard.press('PageDown');
      await page.waitForTimeout(150);
    }
    await page.waitForTimeout(1000);
  };

  for (const [name, frame] of editors) {
    assertEqual(await status(frame), 'Seite 1 von 3', `${name} initial page`);
    assertEqual(await frame.locator('.slot-field-zoom input').inputValue(), '100%', `${name} initial zoom`);
  }
  await page.screenshot({ path: `${output}/differential-page1-zoom100.png` });

  for (const [name, frame] of editors) {
    const zoomIn = frame.getByRole('button', { name: 'Vergrößern (⌘+=)', exact: true });
    await zoomIn.click();
    await zoomIn.click();
    assertEqual(await frame.locator('.slot-field-zoom input').inputValue(), '120%', `${name} zoom`);
  }
  await page.screenshot({ path: `${output}/differential-page1-zoom120.png` });

  for (const [name, frame] of editors) {
    await frame.locator('#id_viewer_overlay').click({ position: { x: 400, y: 400 } });
    await pageDownThreeTimes();
    assertEqual(await status(frame), 'Seite 2 von 3', `${name} page after three PageDown presses`);
  }
  await page.screenshot({ path: `${output}/differential-page2-zoom120.png` });

  for (const [name, frame] of editors) {
    await frame.locator('#id_viewer_overlay').click({ position: { x: 400, y: 400 } });
    await pageDownThreeTimes();
    assertEqual(await status(frame), 'Seite 3 von 3', `${name} page after six PageDown presses`);
  }
  await page.screenshot({ path: `${output}/differential-page3-zoom120.png` });

  return {
    feature_id: feature,
    interaction: 'real-toolbar-zoom-and-keyboard-page-down',
    oracle: { zoom: '120%', page: 'Seite 3 von 3' },
    ctox: { zoom: '120%', page: 'Seite 3 von 3' },
  };
}
