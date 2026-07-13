async (page) => {
  const base = '/src/apps/business-os/office-engine/oracle/business-os-spreadsheet-open-render-sheets.html';
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto(`${base}?locale=en&shell=macos`);
  await page.waitForFunction(() => document.querySelector('#status')?.textContent === 'document-ready', null, { timeout: 60000 });
  const englishFrame = page.frames().find((value) => value.url().includes('/spreadsheeteditor/main/index.html'));
  if (!englishFrame || await englishFrame.getByRole('tab', { name: 'File' }).count() !== 1) {
    throw new Error('English CTOX Spreadsheets did not become ready');
  }
  const first = await page.locator('iframe[data-ctox-office-kind="spreadsheet"]').count();
  await page.evaluate(() => window.businessOsSpreadsheetEvidence.remount('legacy'));
  await page.waitForFunction(() => document.body.textContent.includes('Legacy-Engine unterstützt XLSX nicht.'));
  const afterLegacy = await page.locator('iframe[data-ctox-office-kind="spreadsheet"]').count();
  await page.evaluate(() => window.businessOsSpreadsheetEvidence.remount('ctox_spreadsheets'));
  await page.waitForFunction(() => document.querySelector('iframe[data-ctox-office-kind="spreadsheet"]'));
  const afterCtox = await page.locator('iframe[data-ctox-office-kind="spreadsheet"]').count();
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => document.querySelector('#status')?.textContent === 'document-ready', null, { timeout: 60000 });
  const afterReload = await page.locator('iframe[data-ctox-office-kind="spreadsheet"]').count();
  if (first !== 1 || afterLegacy !== 0 || afterCtox !== 1 || afterReload !== 1) {
    throw new Error(`Spreadsheet lifecycle leaked an iframe: ${JSON.stringify({ first, afterLegacy, afterCtox, afterReload })}`);
  }
  await page.goto(`${base}?locale=de&shell=windows`);
  await page.waitForFunction(() => document.querySelector('#status')?.textContent === 'document-ready', null, { timeout: 60000 });
  const germanFrame = page.frames().find((value) => value.url().includes('/spreadsheeteditor/main/index.html'));
  if (!germanFrame || await germanFrame.getByRole('tab', { name: 'Datei' }).count() !== 1) {
    throw new Error('German CTOX Spreadsheets did not become ready');
  }
  return { first, afterLegacy, afterCtox, afterReload, locales: ['en', 'de'], shell_styles: ['macos', 'windows'] };
}
