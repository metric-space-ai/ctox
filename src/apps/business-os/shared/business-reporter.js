const REPORTER_STYLE_ID = 'ctox-business-reporter-style';

export function initBusinessReporter({
  session,
  getActiveModule,
  authHeaders,
  endpoint = '/api/business-os/reports',
}) {
  if (!session?.authenticated || document.querySelector('[data-ctox-reporter]')) return;
  installReporterStyles();

  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'ctox-report-fab';
  button.dataset.ctoxReporter = 'true';
  button.textContent = 'Report';
  button.addEventListener('click', () => openReporterDialog({
    getActiveModule,
    authHeaders,
    endpoint,
  }));
  document.body.append(button);
}

function openReporterDialog({ getActiveModule, authHeaders, endpoint }) {
  const module = getActiveModule?.() || { id: 'ctox', title: 'CTOX' };
  const backdrop = document.createElement('div');
  backdrop.className = 'ctox-report-backdrop';
  backdrop.innerHTML = `
    <form class="ctox-report-dialog" data-report-form>
      <header>
        <div>
          <strong>Report an CTOX</strong>
          <span>${escapeHtml(module.title || module.id || 'Business OS')}</span>
        </div>
        <button type="button" data-close aria-label="Schliessen">x</button>
      </header>
      <div class="ctox-report-grid">
        <label>
          <span>Typ</span>
          <select name="kind">
            <option value="bug">Bug</option>
            <option value="feature">Feature-Wunsch</option>
          </select>
        </label>
        <label>
          <span>Prioritaet</span>
          <select name="severity">
            <option value="medium">Mittel</option>
            <option value="high">Hoch</option>
            <option value="low">Niedrig</option>
          </select>
        </label>
      </div>
      <label>
        <span>Titel</span>
        <input name="title" required placeholder="Kurz beschreiben" />
      </label>
      <label>
        <span>Beschreibung</span>
        <textarea name="summary" rows="5" placeholder="Was ist passiert oder was wird gebraucht?"></textarea>
      </label>
      <label>
        <span>Erwartung</span>
        <textarea name="expected" rows="3" placeholder="Was sollte CTOX tun oder pruefen?"></textarea>
      </label>
      <footer>
        <span data-status></span>
        <button type="submit">An CTOX senden</button>
      </footer>
    </form>
  `;
  backdrop.querySelector('[data-close]')?.addEventListener('click', () => backdrop.remove());
  backdrop.addEventListener('click', (event) => {
    if (event.target === backdrop) backdrop.remove();
  });
  backdrop.querySelector('[data-report-form]')?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    const status = form.querySelector('[data-status]');
    const submit = form.querySelector('button[type="submit"]');
    submit.disabled = true;
    status.textContent = 'Sende...';
    const data = new FormData(form);
    try {
      await fetchJson(endpoint, {
        method: 'POST',
        headers: authHeaders({ 'Content-Type': 'application/json' }),
        body: JSON.stringify({
          module_id: module.id || 'ctox',
          kind: data.get('kind')?.toString() || 'bug',
          severity: data.get('severity')?.toString() || 'medium',
          title: data.get('title')?.toString().trim() || 'Business OS report',
          summary: data.get('summary')?.toString().trim() || '',
          expected: data.get('expected')?.toString().trim() || '',
          client_context: {
            source: 'business-os-reporter',
            module_id: module.id || '',
            url: location.href,
            app_version: document.documentElement.dataset.appVersion || '',
            viewport: { width: innerWidth, height: innerHeight },
            user_agent: navigator.userAgent,
            created_at: new Date().toISOString(),
          },
        }),
      });
      status.textContent = 'Als CTOX Task angelegt.';
      setTimeout(() => backdrop.remove(), 650);
    } catch (error) {
      submit.disabled = false;
      status.textContent = error.message || String(error);
    }
  });
  document.body.append(backdrop);
  backdrop.querySelector('input[name="title"]')?.focus();
}

async function fetchJson(url, options = {}) {
  const res = await fetch(url, { cache: 'no-store', ...options });
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return res.json();
}

function installReporterStyles() {
  if (document.getElementById(REPORTER_STYLE_ID)) return;
  const style = document.createElement('style');
  style.id = REPORTER_STYLE_ID;
  style.textContent = `
    .ctox-report-fab {
      position: fixed;
      right: 18px;
      bottom: 18px;
      z-index: 40;
      border: 1px solid rgba(135, 153, 170, .34);
      background: #20252b;
      color: #e5e9ee;
      border-radius: 7px;
      padding: 9px 12px;
      font: 600 12px/1.1 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      box-shadow: 0 12px 32px rgba(0, 0, 0, .35);
    }
    .ctox-report-backdrop {
      position: fixed;
      inset: 0;
      z-index: 80;
      display: grid;
      place-items: center;
      background: rgba(5, 8, 12, .62);
    }
    .ctox-report-dialog {
      width: min(560px, calc(100vw - 32px));
      display: grid;
      gap: 14px;
      background: #181c21;
      color: #e5e9ee;
      border: 1px solid rgba(112, 131, 151, .32);
      border-radius: 8px;
      padding: 18px;
      box-shadow: 0 20px 60px rgba(0, 0, 0, .42);
      font: 13px/1.35 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    .ctox-report-dialog header,
    .ctox-report-dialog footer,
    .ctox-report-grid {
      display: flex;
      gap: 12px;
      align-items: center;
      justify-content: space-between;
    }
    .ctox-report-dialog header span,
    .ctox-report-dialog label span,
    .ctox-report-dialog footer span {
      display: block;
      color: #9aa4af;
      font-size: 12px;
    }
    .ctox-report-dialog label {
      display: grid;
      gap: 6px;
    }
    .ctox-report-grid label {
      flex: 1;
    }
    .ctox-report-dialog input,
    .ctox-report-dialog textarea,
    .ctox-report-dialog select {
      width: 100%;
      box-sizing: border-box;
      border: 1px solid rgba(133, 148, 163, .34);
      border-radius: 6px;
      background: #101318;
      color: #edf1f5;
      padding: 9px 10px;
      font: inherit;
    }
    .ctox-report-dialog button {
      border: 0;
      border-radius: 6px;
      background: #596a78;
      color: #f5f7f9;
      padding: 8px 11px;
      font: inherit;
      cursor: pointer;
    }
    .ctox-report-dialog header button {
      background: transparent;
      color: #a9b1ba;
      padding: 4px 7px;
    }
  `;
  document.head.append(style);
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (char) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;',
  }[char]));
}
