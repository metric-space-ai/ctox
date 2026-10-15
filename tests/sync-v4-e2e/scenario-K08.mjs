import { completeFirstWindow } from './criteria.mjs';
export default {
  id: 'K08', title: 'First app/window reads the already held local rows',
  coverage: 'Controller slice: first module window after retiring the warm lease. Not multi-tab K19 or all K08 roles.',
  steps: ['Hold a complete local collection before opening the fixture app', 'Open the app for the first time through Shell openModule', 'Inspect actual module query and visible rows'],
  criterion: { visibleMs: 1000, missingRows: 0, emptyRender: false },
  async run(h) {
    const page = await h.page();
    await h.warm(page); await h.releaseWarmLease(page);
    const measured = await h.openApp(page);
    return { measured, pass: completeFirstWindow(measured) };
  },
};
