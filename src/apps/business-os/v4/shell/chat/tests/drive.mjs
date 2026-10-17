// CHAT4: gemeinsame Bedienung der Chatleiste für chat.test.mjs (v4-Rahmen) und geometry.test.mjs (V2-Bezug).
// Nur echte Maus und Tastatur (page.mouse / page.keyboard), keine DOM-Klicks.

export const SEED = { id: 'chat_seed_chat4', title: 'Angebot Müller prüfen', text: 'Bitte das Angebot Müller prüfen.' };
export const TYPED = 'Bitte die offenen Rechnungen von heute zusammenfassen.';

async function clickCenter(P, handle, dx = null) {
  const b = await handle?.boundingBox();
  if (!b) return false;
  await P.mouse.click(b.x + (dx == null ? b.width / 2 : Math.min(dx, b.width / 2)), b.y + b.height / 2);
  return true;
}

const areaSel = (id) => `[data-ctox-chat-root] [data-chat-id="${id}"] textarea[name="message"]`;

export async function areaVisible(P, id = SEED.id) {
  return P.evaluate((sel) => { const el = document.querySelector(sel); const r = el?.getBoundingClientRect(); return !!r && r.width > 0 && r.height > 0; }, areaSel(id));
}

/** Leiste über die Pille aufklappen; liefert, ob der gesäte Chat als Eintrag steht. */
export async function openDock(P, id = SEED.id) {
  await clickCenter(P, await P.$('[data-ctox-chat-root] [data-chat-open]'), 24);
  await P.waitForSelector(`[data-ctox-chat-root] [data-chat-focus="${id}"]`, { timeout: 5000 }).catch(() => {});
  return P.evaluate((cid) => {
    const chip = document.querySelector(`[data-ctox-chat-root] [data-chat-focus="${cid}"]`);
    return { chip: !!chip, title: chip?.getAttribute('title') || '', text: chip?.textContent?.replace(/\s+/g, ' ').trim() || '' };
  }, id);
}

/** Chat nach vorn holen (falls das Aufklappen ihn nicht schon geöffnet hat), Text tippen; optional senden. */
export async function typeInChat(P, { id = SEED.id, text = TYPED, send = true } = {}) {
  if (!(await areaVisible(P, id))) await clickCenter(P, await P.$(`[data-ctox-chat-root] [data-chat-focus="${id}"]`));
  await P.waitForSelector(areaSel(id), { timeout: 5000 }).catch(() => {});
  const ta = await P.$(areaSel(id));
  if (!ta) return false;
  await clickCenter(P, ta);
  await P.keyboard.type(text, { delay: 5 });
  if (send) await clickCenter(P, await P.$(`[data-ctox-chat-root] [data-chat-id="${id}"] [data-chat-send]`));
  return true;
}

/** Geometrie der Leiste und des offenen Chats (gerundet), dazu die Schriftart der Pille. */
export async function measure(P, id = SEED.id) {
  return P.evaluate((cid) => {
    const r = (sel) => { const el = document.querySelector(sel); if (!el) return null; const b = el.getBoundingClientRect(); if (!b.width || !b.height) return null; return [Math.round(b.left), Math.round(b.top), Math.round(b.width), Math.round(b.height)]; };
    const w = `[data-ctox-chat-root] [data-chat-id="${cid}"]`;
    const fab = document.querySelector('[data-ctox-chat-root] .ctox-chat-fab-label');
    return {
      dock: r('[data-ctox-chat-root] [data-chat-dock]'),
      window: r(w),
      messages: r(`${w} .ctox-chat-messages`),
      textarea: r(`${w} textarea[name="message"]`),
      send: r(`${w} [data-chat-send]`),
      chip: r(`[data-ctox-chat-root] [data-chat-focus="${cid}"]`),
      font: fab ? getComputedStyle(fab).fontFamily : null,
      windowBg: document.querySelector(w) ? getComputedStyle(document.querySelector(w)).backgroundColor : null,
      overflowX: document.querySelector(`${w} .ctox-chat-messages`) ? (() => { const m = document.querySelector(`${w} .ctox-chat-messages`); return m.scrollWidth - m.clientWidth; })() : null,
    };
  }, id);
}
