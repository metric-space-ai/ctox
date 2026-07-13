export async function createOfficeFrameRuntime({ root, bridge, permissions, emit }) {
  let access = { ...permissions };
  let recordId = '';
  let versionId = '';
  let dirty = false;
  root.innerHTML = `
    <section style="display:grid;grid-template-rows:auto 1fr;height:100%;background:#fff;color:#17211f">
      <header style="padding:10px 14px;border-bottom:1px solid #dce3e1"><strong>CTOX Fork Test Runtime</strong></header>
      <div contenteditable="true" data-test-editor style="padding:24px;outline:none">Waiting for document</div>
    </section>`;
  const editor = root.querySelector('[data-test-editor]');
  editor.addEventListener('input', () => {
    dirty = true;
    emit('dirty', { recordId, versionId, dirty });
  });
  return {
    async open(request = {}) {
      recordId = request.recordId || '';
      const loaded = await bridge.loadVersion(request);
      versionId = loaded.version?.id || request.versionId || '';
      editor.textContent = new TextDecoder().decode(loaded.canonicalBytes || new Uint8Array());
      dirty = false;
      emit('opened', { recordId, versionId });
      return this.inspect();
    },
    async save(request = {}) {
      if (!access.write) throw Object.assign(new Error('read only'), { code: 'permission_denied' });
      const bytes = new TextEncoder().encode(editor.textContent || '');
      const result = await bridge.commit({ recordId, baseVersionId: versionId, reason: request.reason || 'test', bytes });
      versionId = result.version_id || versionId;
      dirty = false;
      emit('saved', { recordId, versionId });
      return result;
    },
    export(request = {}) { return bridge.export({ recordId, versionId, format: request.format || 'docx' }); },
    focus() { editor.focus(); return { focused: true }; },
    setPermissions(next = {}) { access = { ...access, ...next }; return { permissions: access }; },
    inspect() { return { schema_version: 'ctox-office-editor-inspection-v1', kind: 'document', runtime: 'oracle-fake', record_id: recordId, version_id: versionId, open: Boolean(recordId), dirty }; },
    destroy() { root.replaceChildren(); return { destroyed: true }; },
  };
}
