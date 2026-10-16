// FR5-3 Negativfall english-raw-term: Codes, Kommentare, Ausnahme, Fehlerklassenname.
// Open, Pending, Analytics im Kommentar zählen nicht.
export const code = 'open';
export const code2 = 'in_progress';
export const sentence = 'Offen seit gestern';
export const en = { open: 'Open' }; // v4-allow: en
export const err = new Error('x').name === 'Error';
