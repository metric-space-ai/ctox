export default {
  id: 'K02', title: 'Second load of a complete lead list',
  coverage: 'Controller slice: one complete-list reload. Not the full K02 background-write/quota matrix.',
  steps: ['Complete native → browser replication', 'Reload same context without clearing IDB', 'Render all local lead IDs through the app DB query'],
  criterion: { visibleMs: 1000, missingRows: 0, duplicateRows: 0 },
  async run(h) {
    const page = await h.page();
    await h.warm(page);
    const measured = await h.reloadAndRead(page);
    return { measured, pass: measured.visibleMs <= 1000 && measured.preQueryLocalCount === measured.rows && measured.missing.length === 0 && measured.duplicates === 0 };
  },
};
