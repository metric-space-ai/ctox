// Schlussantwort eines Agenten-Tasks lesen (Rückruf §16.30: payload {task_id, status, result, error?}).
// ref: index.js leseFreitextAntwort / ersterJsonBlock / freitextAntwortText (vor SKL-C :4928-4996, Browser) — die
// Auswertung lief im Browser nach Status-Polling; jetzt im Complete-Handler.
import { isObject } from './olg.js';

/** Antworttext aus dem Rückruf-Ergebnis (string | {text} | {answer|response|outbound_text}). */
export function replyText(result) {
  if (typeof result === 'string') return result;
  if (isObject(result)) {
    const text = [result.text, result.answer, result.response, result.outbound_text].find((v) => typeof v === 'string' && v.includes('{'));
    if (text) return text;
    if (typeof result.text === 'string') return result.text;
  }
  return '';
}

/** Fehlertext eines failed/blocked/cancelled-Rückrufs. */
export function failureText(payload) {
  const note = [payload?.error, payload?.status_note, payload?.result?.status_note, payload?.result?.error, payload?.result?.message]
    .find((v) => typeof v === 'string' && v.trim() !== '');
  if (note) return note.trim();
  if (isObject(payload?.error) && typeof payload.error.message === 'string') return payload.error.message;
  return `agent task ${payload?.status || 'failed'}`;
}

/** ref: index.js ersterJsonBlock — erstes vollständiges Objekt ab der ersten `{` (Strings beachtet). */
export function firstJsonObject(text) {
  const quelle = String(text || '');
  const anfang = quelle.indexOf('{');
  if (anfang < 0) return '';
  let tiefe = 0;
  let inText = false;
  for (let index = anfang; index < quelle.length; index += 1) {
    const zeichen = quelle[index];
    if (inText) {
      if (zeichen === '\\') index += 1;
      else if (zeichen === '"') inText = false;
    } else if (zeichen === '"') {
      inText = true;
    } else if (zeichen === '{') {
      tiefe += 1;
    } else if (zeichen === '}') {
      tiefe -= 1;
      if (tiefe === 0) return quelle.slice(anfang, index + 1);
    }
  }
  return '';
}

/**
 * ref: index.js leseFreitextAntwort — Kandidaten: ```json-Blöcke (der letzte zuerst), dann der Bereich erste `{` bis
 * letzte `}`; je Kandidat erst als Ganzes, dann das erste vollständige Objekt. `accept(wert)` entscheidet.
 */
export function parseJsonBlock(text, accept) {
  const quelle = String(text || '');
  const kandidaten = [...quelle.matchAll(/```(?:json)?\s*([\s\S]*?)```/g)].map((treffer) => treffer[1]).reverse();
  const anfang = quelle.indexOf('{');
  const ende = quelle.lastIndexOf('}');
  if (anfang >= 0 && ende > anfang) kandidaten.push(quelle.slice(anfang, ende + 1));
  for (const kandidat of kandidaten) {
    for (const teil of [kandidat, firstJsonObject(kandidat)]) {
      if (!teil) continue;
      try {
        const wert = JSON.parse(teil);
        if (accept(wert)) return wert;
      } catch { /* nächster Kandidat */ }
    }
  }
  return null;
}
