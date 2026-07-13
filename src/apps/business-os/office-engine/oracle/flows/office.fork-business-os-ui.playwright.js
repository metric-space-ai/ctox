async (page, { baseUrl = '' } = {}) => {
  baseUrl ||= page.url().match(/^(https?:\/\/[^/]+)/)?.[1] || '';
  const browserErrors = [];
  page.on('console', (message) => { if (message.type() === 'error') browserErrors.push(`console: ${message.text()}`); });
  page.on('pageerror', (error) => browserErrors.push(`page: ${error.message}`));
  const cases = [
    { product: 'ctox-documents', kind: 'document', locale: 'de', theme: 'dark', width: 360, height: 800 },
    { product: 'ctox-documents', kind: 'document', locale: 'en', theme: 'light', width: 640, height: 800 },
    { product: 'ctox-documents', kind: 'document', locale: 'de', theme: 'light', width: 1600, height: 900 },
    { product: 'ctox-documents', kind: 'document', locale: 'en', theme: 'dark', width: 1600, height: 900 },
    { product: 'ctox-spreadsheets', kind: 'spreadsheet', locale: 'de', theme: 'dark', width: 360, height: 800 },
    { product: 'ctox-spreadsheets', kind: 'spreadsheet', locale: 'en', theme: 'light', width: 640, height: 800 },
    { product: 'ctox-spreadsheets', kind: 'spreadsheet', locale: 'de', theme: 'light', width: 1600, height: 900 },
    { product: 'ctox-spreadsheets', kind: 'spreadsheet', locale: 'en', theme: 'dark', width: 1600, height: 900 },
  ];
  const results = [];
  for (const item of cases) {
    await page.setViewportSize({ width: item.width, height: item.height });
    const harness = item.kind === 'document'
      ? 'business-os-document-production-lifecycle.html'
      : 'business-os-spreadsheet-open-render-sheets.html';
    await page.goto(`${baseUrl}/src/apps/business-os/office-engine/oracle/${harness}?locale=${item.locale}&shell=macos&theme=${item.theme}`);
    await page.waitForFunction(() => document.querySelector('#status')?.textContent === 'document-ready', null, { timeout: 60000 });
    const pathPart = item.kind === 'document' ? '/documenteditor/main/index.html' : '/spreadsheeteditor/main/index.html';
    const expectedTab = item.locale === 'de' ? 'Datei' : 'File';
    let editorFrame = null;
    for (let attempt = 0; attempt < 300; attempt += 1) {
      editorFrame = page.frames().find((frame) => frame.url().includes(pathPart));
      if (editorFrame && await editorFrame.getByRole('tab', { name: expectedTab, exact: true }).count() === 1) break;
      editorFrame = null;
      await page.waitForTimeout(100);
    }
    if (!editorFrame) throw new Error(`${item.product} frame did not mount`);
    const state = await editorFrame.evaluate(({ product, theme }) => {
      const style = getComputedStyle(document.body);
      const hidden = (selector) => {
        const node = document.querySelector(selector);
        return !node || getComputedStyle(node).display === 'none';
      };
      return {
        product: document.body.dataset.ctoxProduct,
        theme: document.documentElement.dataset.ctoxTheme,
        title: document.title,
        css: [...document.querySelectorAll('link[rel="stylesheet"]')].map((link) => link.href).filter((href) => href.includes('/forks/')),
        inheritedBrandHidden: hidden('#toolbar .extra.left'),
        inheritedAboutHidden: hidden('#left-btn-about'),
        inheritedTipsHidden: hidden('.synch-tip-root'),
        fontFamily: style.fontFamily,
        visibleForeignBrand: /onlyoffice|euro[ -]?office/i.test(document.body.innerText),
        expected: { product, theme },
      };
    }, { product: item.product, theme: item.theme });
    if (state.product !== item.product || state.theme !== item.theme) throw new Error(`${item.product} identity/theme mismatch: ${JSON.stringify(state)}`);
    if (!state.title.includes(item.product === 'ctox-documents' ? 'CTOX Documents' : 'CTOX Spreadsheets')) throw new Error(`${item.product} title is not product-owned: ${state.title}`);
    if (state.css.length !== 2 || !state.inheritedBrandHidden || !state.inheritedAboutHidden || !state.inheritedTipsHidden || state.visibleForeignBrand) {
      throw new Error(`${item.product} Business OS chrome gate failed: ${JSON.stringify(state)}`);
    }
    const alternateTheme = item.theme === 'dark' ? 'light' : 'dark';
    await page.evaluate((next) => { document.documentElement.dataset.theme = next; }, alternateTheme);
    await editorFrame.waitForFunction((next) => document.documentElement.dataset.ctoxTheme === next, alternateTheme);
    await page.evaluate((next) => { document.documentElement.dataset.theme = next; }, item.theme);
    await editorFrame.waitForFunction((next) => document.documentElement.dataset.ctoxTheme === next, item.theme);
    results.push({ ...item, ...state, live_theme_switch: true, status: 'passed' });
  }
  if (browserErrors.length) throw new Error(`Business OS product UI browser errors: ${JSON.stringify(browserErrors)}`);
  return { results, browser_errors: browserErrors };
}
