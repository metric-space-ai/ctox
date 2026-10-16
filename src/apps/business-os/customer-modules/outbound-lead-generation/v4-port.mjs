// App-API 4.0 – app-lokale Bruecke der Outbound-App (Port Strang OLGUI, 15.10.2026).
// Der Fachcode in index.js arbeitete mit RxDB-Dokumenten, einem Befehlsbus mit Wartezielen und
// Shell-Dialogen. Diese Datei bildet genau diese Bedarfe auf die App-API 4.0 ab und sonst nichts:
//   - Daten: api.get/api.query (lokal, vollstaendig, live), api.put/api.remove.
//     `aendere` ersetzt RxDB `incrementalPatch` (flaches Zusammenfuehren auf oberster Ebene) als
//     vollstaendiges api.put, weil api.patch ein RFC-7396-Merge ist (tiefes Mischen, null loescht) –
//     das haette das Fachverhalten der Leads (payload wird als Ganzes ersetzt) veraendert.
//   - Befehle: api.command ohne Warten auf Annahme; der Status kommt gepusht (h.status). Wer ein
//     Ergebnis braucht (Lookups, Katalog), haengt sich an den gepushten Status – kein Polling,
//     keine Frist. Fehlschlaege gehen genau einmal in die Zustandszone (api.status.report).
//   - Dialoge: api.shell.dialog im App-Fenster.
// Zeilen der App-API sind tief eingefroren; der Fachcode veraendert gelesene Datensaetze stellenweise,
// deshalb liefern die Lesehelfer Kopien (wie RxDB toJSON()).

export const APP_ID = 'outbound-lead-generation';

function kopie(value) {
  return value == null ? value : structuredClone(value);
}

// Gelesene Datensaetze tragen ihre Collection unsichtbar mit (nicht aufzaehlbar, faellt bei JSON und
// structuredClone weg), damit `aendereDok(doc, patch)` die Stelle von RxDB `doc.incrementalPatch` einnimmt.
const SAMMLUNG = Symbol('olg.sammlung');
function markiere(name, row) {
  if (row && typeof row === 'object') Object.defineProperty(row, SAMMLUNG, { value: name, enumerable: false });
  return row;
}

// Befehlsstatus der App-API → Statuswoerter, die der Fachcode seit jeher auswertet.
const STATUS_ALT = { queued: 'queued', accepted: 'accepted', running: 'running', done: 'completed', failed: 'failed' };

export function createPort(api) {
  const abos = new Set();
  const befehlsCodes = new Map();

  function merkeAbo(off) {
    if (typeof off === 'function') abos.add(off);
    return () => { abos.delete(off); off?.(); };
  }

  function daten(name) {
    return {
      name,
      async eins(id) {
        if (!id) return null;
        return markiere(name, kopie(api.get(name, String(id)).get()));
      },
      async alle(query = {}) {
        return kopie(api.query(name, query).get()).map((row) => markiere(name, row));
      },
      jetzt(query = {}) {
        return api.query(name, query).get();
      },
      async schreibe(doc) {
        const handle = api.put(name, doc);
        await handle.local;
        return handle;
      },
      async aendere(id, patch) {
        const aktuell = api.get(name, String(id)).get();
        if (!aktuell) throw new Error(`${name}/${id} fehlt lokal.`);
        const handle = api.put(name, { ...kopie(aktuell), ...patch, id: aktuell.id });
        await handle.local;
        return handle;
      },
      async entferne(id) {
        const handle = api.remove(name, String(id));
        await handle.local;
        return handle;
      },
      abonniere(query, callback) {
        return merkeAbo(api.query(name, query).subscribe(callback));
      },
    };
  }

  // Normalisiert ein business_commands-Dokument auf die Felder, die der Fachcode liest.
  function befehlAlt(doc) {
    if (!doc) return null;
    const status = STATUS_ALT[doc.status] || String(doc.status || '');
    const payload = doc.payload || {};
    return {
      ...kopie(doc),
      id: String(payload.command_id || doc.id),
      kernel_id: doc.id,
      command_type: doc.type,
      record_id: doc.record_id || payload.record_id || '',
      status,
      terminal_status: status === 'completed' || status === 'failed' ? status : '',
      result: kopie(doc.result ?? null),
      error: doc.error || '',
    };
  }

  function befehlsDokument(eigeneId) {
    const id = String(eigeneId || '').trim();
    if (!id) return null;
    const direkt = api.get('business_commands', id).get();
    if (direkt) return befehlAlt(direkt);
    const treffer = api.query('business_commands', { selector: { 'payload.command_id': id }, limit: 1 }).get();
    return befehlAlt(treffer[0] || null);
  }

  function befehleFuerRecords(recordIds, typ) {
    const ids = [...new Set(recordIds.map(String).filter(Boolean))];
    if (!ids.length) return [];
    const selector = { record_id: { $in: ids } };
    if (typ) selector.type = typ;
    return api.query('business_commands', { selector }).get().map(befehlAlt);
  }

  function meldeStatus(typ, handle) {
    const code = `${APP_ID}.command.${typ}`;
    befehlsCodes.set(handle.id, code);
    let fertig = false;
    let off = null;
    const beobachte = (s) => {
      if (fertig) return;
      if (s.status === 'failed') {
        api.status.report({ level: 'error', code, text: `${typ}: ${s.error || 'abgelehnt'}`, ref: handle.id });
      } else if (s.status === 'done') {
        api.status.clear(code);
      } else return;
      fertig = true;
      if (off) { abos.delete(off); off(); }
    };
    off = handle.status.subscribe(beobachte);
    if (fertig) off(); else abos.add(off);
    handle.local.catch((error) => {
      api.status.report({ level: 'error', code, text: `${typ}: ${String(error?.message || error)}`, ref: handle.id });
    });
  }

  // Altes Befehlsobjekt ({id, command_type, record_id, payload, client_context, …}) → api.command.
  // `ergebnis: true` liefert ein Promise auf den Endstand (gepusht), sonst sofort die Quittung.
  function befehl(command, { ergebnis = false } = {}) {
    const typ = String(command?.command_type || '').trim();
    if (!typ) throw new Error('Befehlstyp fehlt.');
    const recordId = command.record_id != null ? String(command.record_id) : undefined;
    const payload = {
      ...(command.payload || {}),
      ...(command.id ? { command_id: String(command.id) } : {}),
      ...(recordId !== undefined ? { record_id: recordId } : {}),
      ...(command.client_context ? { client_context: command.client_context } : {}),
      module: command.module || APP_ID,
    };
    for (const key of ['title', 'instruction', 'prompt', 'text', 'user_message', 'priority', 'thread_key', 'mode', 'target',
      'required_skills', 'writeback_contract', 'response_channel', 'source_module', 'source_title', 'open', 'control_command']) {
      if (command[key] !== undefined && payload[key] === undefined) payload[key] = command[key];
    }
    const handle = api.command(typ, payload, recordId !== undefined ? { record_id: recordId } : {});
    meldeStatus(typ, handle);
    const quittung = { id: String(command.id || handle.id), command_id: String(command.id || handle.id), kernel_id: handle.id, handle };
    if (!ergebnis) return handle.local.then(() => quittung);
    return endstand(handle).then((s) => ({
      ...quittung,
      ok: s.status === 'done',
      status: STATUS_ALT[s.status] || s.status,
      terminal_status: STATUS_ALT[s.status] || s.status,
      result: kopie(s.result ?? null),
      error: s.error || '',
    }));
  }

  // Endstand eines Befehls aus dem gepushten Status (done|failed); keine Frist, kein Polling.
  function endstand(handle) {
    return new Promise((resolve) => {
      let fertig = false;
      let off = null;
      const sieh = (s) => {
        if (fertig || (s.status !== 'done' && s.status !== 'failed')) return;
        fertig = true;
        if (off) { abos.delete(off); off(); }
        resolve(s);
      };
      off = handle.status.subscribe(sieh);
      if (fertig) off(); else abos.add(off);
      handle.local.catch((error) => sieh({ status: 'failed', error: String(error?.message || error) }));
    });
  }

  // Ersetzt businessChat.submitTask: ein Agentenauftrag ist ein gewoehnlicher Befehl. Quittiert wird
  // mit der eigenen Auftrags-ID (command_id) und der Kernel-ID des Befehlsdokuments als Task-Bezug.
  async function auftrag(detail) {
    const typ = String(detail?.command_type || 'business_os.chat.task');
    const quittung = await befehl({ ...detail, command_type: typ, id: detail?.command_id || detail?.id });
    return { ...quittung, task_id: quittung.kernel_id, status: 'queued' };
  }

  // Wie shared/command-bus.js cancel(): Plattformbefehl ctox.command.cancel {target_command_id, reason}.
  function abbrechen(eigeneId, reason = '') {
    const doc = befehlsDokument(eigeneId);
    const ziel = String(doc?.kernel_id || eigeneId || '');
    return befehl({ command_type: 'ctox.command.cancel', record_id: ziel, payload: { target_command_id: ziel, reason } }, { ergebnis: true });
  }

  async function dialog(spec) {
    return api.shell.dialog(spec);
  }

  async function aendereDok(doc, patch) {
    const name = doc?.[SAMMLUNG];
    if (!name) throw new Error('Datensatz ohne Collection-Bezug.');
    return daten(name).aendere(doc.id, patch);
  }

  async function entferneDok(doc) {
    const name = doc?.[SAMMLUNG];
    if (!name) throw new Error('Datensatz ohne Collection-Bezug.');
    return daten(name).entferne(doc.id);
  }

  return {
    api,
    daten,
    aendereDok,
    entferneDok,
    befehl,
    endstand,
    befehlsDokument,
    befehleFuerRecords,
    auftrag,
    abbrechen,
    meldung: (text, opts = {}) => dialog({ kind: 'alert', title: opts.title || '', text: String(text ?? ''), confirm_label: opts.confirmLabel || 'OK' }),
    bestaetigung: async (text, opts = {}) => (await dialog({ kind: 'confirm', title: opts.title || '', text: String(text ?? ''), confirm_label: opts.confirmLabel || 'OK', cancel_label: opts.cancelLabel || 'Abbrechen', default: false })) === true,
    eingabe: async (text, opts = {}) => {
      const antwort = await dialog({ kind: 'prompt', title: opts.title || '', text: String(text ?? ''), value: opts.value ?? opts.defaultValue ?? '', placeholder: opts.placeholder || '', confirm_label: opts.confirmLabel || 'OK', default: null });
      return antwort == null || antwort === false ? null : String(antwort);
    },
    melde: (code, text, level = 'error', ref) => api.status.report({ level, code: `${APP_ID}.${code}`, text: String(text), ...(ref ? { ref } : {}) }),
    raeume: (code) => api.status.clear(`${APP_ID}.${code}`),
    abo: merkeAbo,
    abmelden() {
      for (const off of [...abos]) { try { off(); } catch { /* bereits abgemeldet */ } }
      abos.clear();
    },
    aboZahl: () => abos.size,
  };
}
