export default {
  id: 'K08', title: 'First app/window reads the already held local rows',
  steps: ['Hold a complete local collection before opening the fixture app', 'Open the app for the first time through Shell openModule', 'Inspect actual module query and visible rows'],
  criterion: { visibleMs: 1000, missingRows: 0, emptyRender: false },
  async run(h) {
    const page = await h.page();
    await h.warm(page); await h.releaseWarmLease(page);
    const measured = await h.openApp(page);
    return { measured, pass: measured.visibleMs <= 1000 && !measured.emptyRender && measured.missing.length === 0 };
  },
};
