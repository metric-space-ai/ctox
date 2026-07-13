async (page) => {
  const browser = page.context().browser();
  const cases = [
    {
      kind: 'document',
      url: '/src/apps/business-os/office-engine/oracle/business-os-document-production-lifecycle.html?locale=de&shell=windows&clean=1',
      evidence: 'businessOsDocumentEvidence',
      editorPath: '/documenteditor/main/index.html',
      tab: 'Datei',
    },
    {
      kind: 'spreadsheet',
      url: '/src/apps/business-os/office-engine/oracle/business-os-spreadsheet-open-render-sheets.html?locale=en&shell=macos&clean=1',
      evidence: 'businessOsSpreadsheetEvidence',
      editorPath: '/spreadsheeteditor/main/index.html',
      tab: 'File',
    },
  ];
  const results = [];
  for (const testCase of cases) {
    const context = await browser.newContext({ viewport: { width: 1600, height: 900 } });
    const cleanPage = await context.newPage();
    const errors = [];
    cleanPage.on('console', (message) => { if (message.type() === 'error' || message.type() === 'warning') errors.push(message.text()); });
    cleanPage.on('pageerror', (error) => errors.push(error.message));
    await cleanPage.goto(testCase.url);
    await cleanPage.waitForFunction(() => document.querySelector('#status')?.textContent === 'document-ready', null, { timeout: 60000 });
    const initialStorage = await cleanPage.evaluate((name) => window[name].initialStorage, testCase.evidence);
    const originalFrame = cleanPage.frames().find((value) => value.url().includes(testCase.editorPath));
    if (initialStorage.localStorageKeys.length || initialStorage.sessionStorageKeys.length || !originalFrame || await originalFrame.getByRole('tab', { name: testCase.tab }).count() !== 1 || errors.length) {
      throw new Error(`Clean-profile ${testCase.kind} failed: ${JSON.stringify({ initialStorage, errors })}`);
    }
    results.push({ kind: testCase.kind, initialStorage, consoleErrors: errors.length });
    await context.close();
  }
  return results;
}
