async (page) => {
  const cases = [
    { kind: 'document', url: '/src/apps/business-os/office-engine/oracle/business-os-document-production-lifecycle.html?reconnect=1', evidence: 'businessOsDocumentEvidence', feature: 'document.open-render-zoom' },
    { kind: 'spreadsheet', url: '/src/apps/business-os/office-engine/oracle/business-os-spreadsheet-open-render-sheets.html?reconnect=1', evidence: 'businessOsSpreadsheetEvidence', feature: 'spreadsheet.open-render-sheets' },
  ];
  const results = [];
  for (const testCase of cases) {
    await page.goto(testCase.url);
    await page.waitForFunction(() => document.querySelector('#status')?.textContent === 'document-ready', null, { timeout: 60000 });
    await page.context().setOffline(true);
    await page.waitForFunction(() => navigator.onLine === false);
    await page.evaluate(async ({ kind, evidenceName, feature }) => {
      const { createBusinessOsOfficeBridge } = await import('/src/apps/business-os/office-engine/src/business-os-bridge.mjs');
      const evidence = window[evidenceName];
      const row = evidence.chunks.find((value) => value.blob_id === evidence.version.editor_blob_id);
      const bytes = Uint8Array.from(atob(row.data), (value) => value.charCodeAt(0));
      evidence.pendingReconnectCommit = createBusinessOsOfficeBridge(evidence.ctx, kind).commit({
        recordId: evidence.record.id, baseVersionId: evidence.version.id,
        editorProtocol: evidence.version.editor_protocol, editorProtocolVersion: evidence.version.editor_protocol_version,
        implementedFeatures: [feature], reason: 'offline-reconnect', bytes,
      }).then((value) => (evidence.reconnectResult = value));
    }, { kind: testCase.kind, evidenceName: testCase.evidence, feature: testCase.feature });
    await page.waitForFunction((name) => window[name].syncEvents.some((value) => value.event === 'start' && value.replicationUp === false), testCase.evidence);
    const before = await page.evaluate((name) => ({ commands: window[name].commands.length, chunks: window[name].chunks.length }), testCase.evidence);
    await page.context().setOffline(false);
    await page.waitForFunction((name) => window[name].reconnectResult, testCase.evidence, { timeout: 30000 });
    const after = await page.evaluate((name) => ({ commands: window[name].commands.length, chunks: window[name].chunks.length, syncEvents: window[name].syncEvents }), testCase.evidence);
    if (before.commands !== 0 || after.commands !== 1 || after.chunks <= 2 || !after.syncEvents.some((value) => value.event === 'online')) throw new Error(`${testCase.kind} offline/reconnect failed`);
    results.push({ kind: testCase.kind, before, after });
  }
  return results;
}
