// Crew-Leiste (FR6d) unten links wie die Shell-V2 (shared/business-chat.js renderChatRoot, eingeklappter Zustand):
// Pille „Crew“ mit den Avataren der Mitglieder (höchstens 6) und daneben der Kalender-Navigator ‹ 📅 ›. Gleiche Klassen
// und Struktur; CSS wörtlich aus business-chat.js installChatStyles und app.css über shell-v2-desktop.css.
// Mitglieder = Apps mit layout.presentation.taskbar_group „crew“ (dieselbe Gruppe wie der Reiter „Crew“). Klick auf ein
// Mitglied öffnet dessen Chat (onMember), Klick auf die Pille daneben holt die Crew nach vorn (onCrew).
// Lücke: Die Shell-V2 liest die Crew aus ihren Chat-Collections (crewMembers mit Gestalt, Farbe, Last, Stimmung) und
// zeichnet sie mit crew-renderer.js. Der v4-Kernel liefert dem Rahmen nichts davon; die Leiste zeigt deshalb das, was da
// ist: die Crew-Apps mit ihrem App-Bild als Avatar. Der Kalender-Navigator hält den gewählten Tag (heute ± n); einen
// Verbraucher dafür (Chats eines Tages) gibt es im v4-Kernel noch nicht.

const PREV_SVG = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"></polyline></svg>';
const NEXT_SVG = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg>';
const CAL_SVG = '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="18" rx="2" ry="2"></rect><line x1="16" y1="2" x2="16" y2="6"></line><line x1="8" y1="2" x2="8" y2="6"></line><line x1="3" y1="10" x2="21" y2="10"></line></svg>';
const MAX_MEMBERS = 6; // business-chat.js: state.crewMembers.slice(0, 6)

export function crewBarMarkup() {
  return `
    <section class="ctox-chat-dock is-collapsed has-no-chats has-no-nav" data-v4-crew-dock>
      <button class="ctox-chat-fab" type="button" data-v4-crew-open>
        <span class="ctox-chat-fab-label" data-v4-crew-label>Crew</span>
        <span class="ctox-chat-fab-creatures" data-v4-crew-members></span>
      </button>
      <div class="ctox-chat-date-pill">
        <button class="ctox-date-nav-btn" type="button" data-v4-crew-prev>${PREV_SVG}</button>
        <div class="ctox-date-picker-trigger" role="button" tabindex="0" data-v4-crew-cal>
          ${CAL_SVG}
          <input type="date" class="ctox-date-native-picker" data-v4-crew-date tabindex="-1" aria-hidden="true" />
        </div>
        <button class="ctox-date-nav-btn" type="button" data-v4-crew-next>${NEXT_SVG}</button>
      </div>
    </section>`;
}

// Ein Mitglied wie business-chat.js crewPoolSlotHtml (Platz .ctox-chat-crew-slot, Körper .ctox-crew-creature.is-fab);
// statt der gezeichneten Kreatur steht das App-Bild im Körper.
export function crewSlotMarkup() {
  return '<span class="ctox-crew-creature is-idle is-fab" data-v4-icon aria-hidden="true"></span>';
}

const pad = (n) => String(n).padStart(2, '0');
const isoDay = (d) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

export function createCrewBar({ host, t, lang = 'de', iconFor, onMember, onCrew }) {
  const el = document.createElement('div');
  el.className = 'v4f-crew';
  el.dataset.v4V2 = '';
  el.dataset.shellStyle = 'ctox';
  el.innerHTML = `<div class="ctox-chat-root is-collapsed" data-v4-crew-bar>${crewBarMarkup()}</div>`;
  host.appendChild(el);
  const bar = el.querySelector('[data-v4-crew-bar]');
  const fab = el.querySelector('[data-v4-crew-open]');
  const membersEl = el.querySelector('[data-v4-crew-members]');
  const cal = el.querySelector('[data-v4-crew-cal]');
  const dateInput = el.querySelector('[data-v4-crew-date]');
  el.querySelector('[data-v4-crew-label]').textContent = t('crew', 'Crew');
  el.querySelector('[data-v4-crew-prev]').setAttribute('aria-label', t('crew.prevDay', 'Vorheriger Tag'));
  el.querySelector('[data-v4-crew-next]').setAttribute('aria-label', t('crew.nextDay', 'Nächster Tag'));
  fab.setAttribute('aria-label', t('crew.open', 'Crew öffnen'));

  // ---------- Tag (heute ± n), nie in der Zukunft (business-chat.js max = heute) ----------
  let day = new Date();
  day.setHours(12, 0, 0, 0);
  function setDay(next) {
    const today = new Date();
    today.setHours(12, 0, 0, 0);
    day = next > today ? today : next;
    const iso = isoDay(day);
    dateInput.value = iso;
    dateInput.max = isoDay(today);
    bar.dataset.v4CrewDay = iso;
    const label = day.toLocaleDateString(lang === 'en' ? 'en-US' : 'de-DE', { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' });
    cal.setAttribute('aria-label', label);
    cal.title = label;
  }
  setDay(day);
  const shift = (n) => { const d = new Date(day); d.setDate(d.getDate() + n); setDay(d); };
  el.querySelector('[data-v4-crew-prev]').addEventListener('click', () => shift(-1));
  el.querySelector('[data-v4-crew-next]').addEventListener('click', () => shift(1));
  const openPicker = () => { try { dateInput.showPicker?.(); } catch { dateInput.focus(); } };
  cal.addEventListener('click', (event) => { if (event.target !== dateInput) openPicker(); });
  cal.addEventListener('keydown', (event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); openPicker(); } });
  dateInput.addEventListener('change', () => { if (dateInput.value) setDay(new Date(`${dateInput.value}T12:00:00`)); });

  // ---------- Mitglieder ----------
  const slots = new Map(); // app_id -> slot
  fab.addEventListener('click', (event) => {
    const slot = event.target.closest('[data-v4-crew]');
    if (slot && membersEl.contains(slot)) onMember?.(slot.dataset.v4Crew, { anchor: slot, event });
    else onCrew?.({ anchor: fab, event });
  });

  function setMembers(list) {
    const members = (list || []).slice(0, MAX_MEMBERS);
    const keep = new Set(members.map((a) => a.id));
    for (const [id, slot] of slots) if (!keep.has(id)) { slot.remove(); slots.delete(id); }
    members.forEach((app, i) => {
      let slot = slots.get(app.id);
      if (!slot) {
        slot = document.createElement('span');
        slot.className = 'ctox-chat-crew-slot';
        slot.dataset.v4Crew = app.id;
        slot.innerHTML = crewSlotMarkup();
        slots.set(app.id, slot);
      }
      const name = String(app.title || app.id);
      slot.title = name;
      slot.setAttribute('aria-label', name);
      iconFor(app, slot.querySelector('[data-v4-icon]'));
      if (membersEl.children[i] !== slot) membersEl.insertBefore(slot, membersEl.children[i] || null);
    });
    membersEl.classList.toggle('is-members', members.length > 0);
    el.hidden = members.length === 0;
  }

  return {
    element: el,
    setMembers,
    get day() { return dateInput.value; },
    destroy() { el.remove(); },
  };
}
