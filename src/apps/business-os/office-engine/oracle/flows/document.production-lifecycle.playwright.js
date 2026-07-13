async (page) => {
  const origin = page.url().match(/^(https?:\/\/[^/]+)/)?.[1];
  if (!origin) throw new Error(`Document lifecycle requires an HTTP page, got ${page.url()}`);
  const base = `${origin}/src/apps/business-os/office-engine/oracle/business-os-document-production-lifecycle.html`;
  const waitForCtoxDocumentsFrame = async (tabName) => {
    for (let attempt = 0; attempt < 600; attempt += 1) {
      const frame = page.frames().find((value) => value.url().includes('/documenteditor/main/index.html'));
      if (frame && await frame.getByRole('tab', { name: tabName }).count() === 1) return frame;
      await page.waitForTimeout(100);
    }
    throw new Error(`${tabName} in CTOX Documents did not become ready`);
  };
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto(`${base}?locale=en&shell=macos`);
  await page.waitForFunction(() => document.querySelector('#status')?.textContent === 'document-ready', null, { timeout: 60000 });
  await waitForCtoxDocumentsFrame('File');
  const first = await page.locator('iframe[data-ctox-office-kind="document"]').count();
  await page.evaluate(() => window.businessOsDocumentEvidence.remount('legacy'));
  await page.waitForFunction(() => !document.querySelector('iframe[data-ctox-office-kind="document"]'));
  const afterLegacy = await page.locator('iframe[data-ctox-office-kind="document"]').count();
  await page.evaluate(() => window.businessOsDocumentEvidence.remount('ctox_documents'));
  await page.waitForFunction(() => document.querySelector('iframe[data-ctox-office-kind="document"]'), null, { timeout: 60000 });
  const afterCtox = await page.locator('iframe[data-ctox-office-kind="document"]').count();
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => document.querySelector('#status')?.textContent === 'document-ready', null, { timeout: 60000 });
  const afterReload = await page.locator('iframe[data-ctox-office-kind="document"]').count();
  if (first !== 1 || afterLegacy !== 0 || afterCtox !== 1 || afterReload !== 1) throw new Error(`Document lifecycle leaked an iframe: ${JSON.stringify({ first, afterLegacy, afterCtox, afterReload })}`);
  await page.goto(`${base}?locale=de&shell=windows`);
  await page.waitForFunction(() => document.querySelector('#status')?.textContent === 'document-ready', null, { timeout: 60000 });
  const germanFrame = await waitForCtoxDocumentsFrame('Datei');
  const commentsButton = germanFrame.getByRole('button', { name: /Kommentare/ }).first();
  if (!await commentsButton.isEnabled()) throw new Error('Business OS did not grant comment permission to CTOX Documents');
  await commentsButton.click();
  const addCommentButton = germanFrame.getByRole('button', { name: 'Kommentar hinzufügen' });
  if (!await addCommentButton.isEnabled()) throw new Error('CTOX Documents comment UI is not writable');
  await germanFrame.getByRole('tab', { name: 'Zusammenarbeit' }).click();
  const trackChangesButton = germanFrame.getByRole('button', { name: 'Nachverfolgen von Änderungen' }).first();
  if (!await trackChangesButton.isEnabled()) throw new Error('Business OS did not grant review permission to CTOX Documents');
  const reviewModeButton = germanFrame.getByRole('button', { name: 'Überprüfung' });
  if (await reviewModeButton.count() !== 1) {
    await trackChangesButton.click();
    const enableForMe = germanFrame.getByText('AKTIVIERT für mich', { exact: true });
    if (await enableForMe.count() > 0) await enableForMe.first().click();
  }
  let reviewModeEntered = false;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (await reviewModeButton.count() === 1) {
      reviewModeEntered = true;
      break;
    }
    await page.waitForTimeout(100);
  }
  if (!reviewModeEntered) throw new Error('CTOX Documents did not enter review mode');
  await page.screenshot({
    fullPage: true,
    path: 'output/playwright/ctox-office/rollout/document-lifecycle/comments-review-business-os.png',
  });
  return {
    first,
    afterLegacy,
    afterCtox,
    afterReload,
    locales: ['en', 'de'],
    shell_styles: ['macos', 'windows'],
    permissions: { comments: true, review: true },
  };
}
