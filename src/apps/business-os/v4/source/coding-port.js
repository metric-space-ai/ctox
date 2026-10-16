// Shell v4 – Coding-Turn-Vertrag (§16.44) über die App-API der Shell (Strang AGENT).
//
//   createCodingPort(api, sourceFn) → { command, turns, draftRef, readPatch }
//     api       App-API der Shell (kernel.appApi('shell', ver)): command(), query('coding_turns'), get('files'), files.get(ref), actor.id
//     sourceFn  kernel.source (Quelltext-API je App)
//
//   command(type, payload, opts?) → Promise<{status:'running', turn_id} | {status:'completed', turn_id} | {status:'rejected', code, reason}>
//     Der Core prüft beim Write des Befehlsdokuments (exakte Codes im WriteResult, §16.44); angenommen = Turn läuft.
//   turns       Live-Collection coding_turns (Typ CodingTurn, index.d.ts ab Stub-Pin 51b1e33)
//   draftRef(appId) → Promise<null | {unsupported:true} | {draft_id, rev}>
//     draft_id = app_drafts-Zeile '<actor.id>/<app_id>', rev = draft.info().rev (= base_draft_rev). Kennt der Kern
//     §16.44 nicht (info() ohne rev), liefert die Funktion {unsupported:true}. Ein frisch angelegter Entwurf hat erst nach
//     der Core-Bestätigung eine Revision; darauf wird kurz gewartet.
//   readPatch(turn) → Promise<string>  Patch-Datei files/coding-patch:<turn_id> (Inhalt über files.get(ref))

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export function createCodingPort(api, sourceFn) {
  return Object.freeze({
    async command(type, payload, opts) {
      // opts.record_id (§16.19 Nr. 8): Bezug des Befehls, z. B. die Meldung, die der Lauf bearbeitet (Strang LOOP).
      const handle = opts ? api.command(type, payload, opts) : api.command(type, payload);
      const r = await handle.result;
      if (r?.status === 'rejected') return { status: 'rejected', code: r.code ?? 'rejected', reason: r.reason ?? '' };
      return { status: type === 'ctox.coding.cancel' ? 'completed' : 'running', turn_id: payload?.turn_id ?? null, command_id: handle.id };
    },
    turns: api.query('coding_turns', {}),
    async draftRef(appId, { waitMs = 3000 } = {}) {
      const src = sourceFn(appId);
      const t0 = Date.now();
      for (;;) {
        const info = await src.draft.info();
        if (!info) return null;
        if (!('rev' in info)) return { unsupported: true };
        if (info.rev) return { draft_id: `${api.actor.id}/${appId}`, rev: info.rev, base_app_ver: info.base_app_ver };
        if (Date.now() - t0 > waitMs) return { draft_id: `${api.actor.id}/${appId}`, rev: null, base_app_ver: info.base_app_ver };
        await sleep(40);
      }
    },
    async readPatch(turn) {
      let ref = turn?.patch?.ref ?? null;
      if (!ref && turn?.patch?.file_id) ref = api.get('files', turn.patch.file_id).get()?.ref ?? null;
      if (!ref) throw Object.assign(new Error('Patch-Datei fehlt'), { code: 'not_found' });
      const blob = await api.files.get(ref);
      return blob.text();
    },
  });
}
