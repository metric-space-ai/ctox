// chat-ui — a self-contained, dependency-free chat transcript view for the
// Business OS "coding-agents" module. Hand-written, no upstream copy; the
// "fluid" streaming feel (keyed reconciliation, sticky auto-scroll, subtle
// enter animation, typing indicator) is built from scratch here.
//
// Zero dependencies. One file. Injects its own scoped <style> (prefix `cui-`)
// once per document. Uses ONLY shell design tokens (var(--surface), --line,
// --text, --muted, --accent, --control-radius, --danger, …) — never literal
// neutrals, gradients, or box-shadows (enforced by the module guard test).
//
// Public API:
//   createChatView(host, { onRetry } = {})
//     -> { update({ events, running, emptyText }), destroy() }
//
// An `event` is a plain object: { role, text, status, key?, id?, seq?, failed? }
//   role  'user'                     -> right-aligned accent bubble (plain text)
//   role  'assistant' | 'agent'      -> left surface bubble (safe markdown)
//   anything else (e.g. 'system')    -> one-line muted protocol row

const STYLE_ID = 'cui-style';

/* ------------------------------------------------------------------ *
 * Safe text + markdown subset
 * ------------------------------------------------------------------ */

// Escape EVERYTHING first. Every renderer below operates on already-escaped
// text and only introduces a fixed set of known-safe tags — raw input is never
// placed into innerHTML.
export function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

// Inline transforms on ALREADY-ESCAPED text: inline code, links (https/http
// only), bold, italic. Code spans are protected first so their contents are
// never re-processed.
function renderInline(escaped) {
  const codes = [];
  // Protect code spans behind a null-byte sentinel (cannot occur in escaped
  // text) so their contents are never re-processed by the transforms below,
  // then restore them last.
  let s = escaped.replace(/`([^`]+)`/g, (_m, code) => {
    codes.push(code);
    return `\u0000${codes.length - 1}\u0000`;
  });

  // Links: [text](https://…). The URL is already escaped, so a `"` is `&quot;`
  // and cannot break out of the attribute; a non-http(s) scheme simply never
  // matches and stays literal text.
  s = s.replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g, (_m, text, url) =>
    `<a class="cui-link" href="${url}" target="_blank" rel="noopener noreferrer">${text}</a>`,
  );

  s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  s = s.replace(/(^|[^*])\*([^*\s][^*]*?)\*/g, '$1<em>$2</em>');
  s = s.replace(/(^|[^\w_])_([^_\s][^_]*?)_(?=[^\w_]|$)/g, '$1<em>$2</em>');

  s = s.replace(/\u0000(\d+)\u0000/g, (_m, i) => `<code class="cui-code">${codes[Number(i)]}</code>`);
  return s;
}

// Block-level markdown subset: fenced code blocks, unordered lists,
// paragraphs. Returns a safe HTML string.
export function renderMarkdown(raw) {
  const lines = escapeHtml(raw).split('\n');
  const out = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Fenced code block ```
    if (/^\s*```/.test(line)) {
      i += 1;
      const code = [];
      while (i < lines.length && !/^\s*```\s*$/.test(lines[i])) {
        code.push(lines[i]);
        i += 1;
      }
      if (i < lines.length) i += 1; // consume closing fence
      out.push(
        '<div class="cui-codeblock">' +
          '<div class="cui-codebar"><button type="button" class="cui-copy" aria-label="Copy code">Copy</button></div>' +
          `<pre class="cui-pre"><code>${code.join('\n')}</code></pre>` +
        '</div>',
      );
      continue;
    }

    // Unordered list
    if (/^\s*[-*]\s+/.test(line)) {
      const items = [];
      while (i < lines.length && /^\s*[-*]\s+/.test(lines[i])) {
        items.push(`<li>${renderInline(lines[i].replace(/^\s*[-*]\s+/, ''))}</li>`);
        i += 1;
      }
      out.push(`<ul class="cui-list">${items.join('')}</ul>`);
      continue;
    }

    // Blank line
    if (/^\s*$/.test(line)) {
      i += 1;
      continue;
    }

    // Paragraph: gather until a blank line / list / fence
    const para = [];
    while (
      i < lines.length &&
      !/^\s*$/.test(lines[i]) &&
      !/^\s*```/.test(lines[i]) &&
      !/^\s*[-*]\s+/.test(lines[i])
    ) {
      para.push(lines[i]);
      i += 1;
    }
    out.push(`<p>${renderInline(para.join('\n'))}</p>`);
  }

  return out.join('');
}

/* ------------------------------------------------------------------ *
 * Row model (pure) — testable without a DOM
 * ------------------------------------------------------------------ */

export function classifyRole(event) {
  const role = String(event?.role || '').toLowerCase();
  if (role === 'user') return 'user';
  if (role === 'assistant' || role === 'agent') return 'assistant';
  return 'system';
}

function looksFailed(event) {
  if (event?.failed === true) return true;
  const status = String(event?.status || '').toLowerCase();
  return /fail|error|denied|abort/.test(status);
}

// Returns { cls, html } for one event row. `html` is the row's inner markup.
export function renderRowHtml(event) {
  const kind = classifyRole(event);
  const status = String(event?.status || '').trim();
  const text = String(event?.text ?? '');

  if (kind === 'user') {
    return {
      cls: 'cui-row cui-row--user',
      html: `<div class="cui-bubble cui-bubble--user"><span class="cui-user-text">${escapeHtml(text)}</span></div>`,
    };
  }

  if (kind === 'assistant') {
    const meta = status ? `<div class="cui-meta">${escapeHtml(status)}</div>` : '';
    return {
      cls: 'cui-row cui-row--assistant',
      html: `<div class="cui-bubble cui-bubble--assistant">${meta}<div class="cui-md">${renderMarkdown(text)}</div></div>`,
    };
  }

  const detail = status ? `${escapeHtml(text)} · ${escapeHtml(status)}` : escapeHtml(text);
  return {
    cls: `cui-row cui-row--system${looksFailed(event) ? ' is-failed' : ''}`,
    html: `<span class="cui-proto${looksFailed(event) ? ' is-failed' : ''}">${detail}</span>`,
  };
}

function keyOf(event, index, used) {
  const raw = event?.key ?? event?.id ?? event?._id ?? (event?.seq != null ? `s${event.seq}` : null);
  let key = raw != null ? String(raw) : `i${index}`;
  while (used.has(key)) key = `${key}~${index}`;
  used.add(key);
  return key;
}

/* ------------------------------------------------------------------ *
 * Styles (injected once)
 * ------------------------------------------------------------------ */

const STYLE = `
.cui-thread{display:flex;flex-direction:column;gap:8px;}
.cui-empty{color:var(--muted);font-size:12px;padding:8px 2px;}
.cui-row{display:flex;flex-direction:column;}
.cui-row--user{align-items:flex-end;}
.cui-row--assistant{align-items:flex-start;}
.cui-row--system{align-items:stretch;}
.cui-row--enter{animation:cui-in 180ms ease both;}
@keyframes cui-in{from{opacity:0;transform:translateY(4px);}to{opacity:1;transform:none;}}
.cui-bubble{max-width:82%;font-size:13px;line-height:1.5;border-radius:var(--control-radius);padding:7px 11px;overflow-wrap:anywhere;}
.cui-bubble--user{background:var(--accent);color:var(--accent-foreground,var(--surface));border-bottom-right-radius:2px;}
.cui-bubble--assistant{background:var(--surface-2,var(--surface));color:var(--text);border:1px solid var(--line);border-bottom-left-radius:2px;}
.cui-user-text{white-space:pre-wrap;}
.cui-meta{color:var(--muted);font-size:10px;margin-bottom:3px;}
.cui-md>p{margin:0 0 6px;}
.cui-md>p:last-child{margin-bottom:0;}
.cui-list{margin:4px 0;padding-left:16px;}
.cui-list li{margin:2px 0;}
.cui-code{font-family:var(--font-mono);font-size:12px;background:var(--surface);border:1px solid var(--line);border-radius:4px;padding:0 4px;}
.cui-link{color:var(--accent);text-decoration:underline;}
.cui-codeblock{margin:6px 0;border:1px solid var(--line);border-radius:var(--control-radius);background:var(--surface);overflow:hidden;}
.cui-codebar{display:flex;justify-content:flex-end;padding:3px 4px;border-bottom:1px solid var(--line);}
.cui-copy{font-family:var(--font-mono);font-size:10px;color:var(--muted);background:transparent;border:1px solid var(--line);border-radius:4px;padding:1px 6px;cursor:pointer;}
.cui-copy:hover{color:var(--text);}
.cui-copy.is-done{color:var(--accent);}
.cui-pre{margin:0;padding:8px 10px;overflow:auto;}
.cui-pre code{font-family:var(--font-mono);font-size:12px;color:var(--text);white-space:pre;}
.cui-proto{color:var(--muted);font-size:11px;padding:1px 2px;overflow-wrap:anywhere;}
.cui-proto.is-failed{color:var(--danger);}
.cui-typing{display:flex;gap:4px;align-items:center;align-self:flex-start;padding:8px 11px;}
.cui-dot{width:5px;height:5px;border-radius:999px;background:var(--muted);opacity:.35;animation:cui-pulse 1s ease-in-out infinite;}
.cui-dot:nth-child(2){animation-delay:.15s;}
.cui-dot:nth-child(3){animation-delay:.3s;}
@keyframes cui-pulse{0%,100%{opacity:.25;transform:translateY(0);}50%{opacity:.9;transform:translateY(-2px);}}
@media (prefers-reduced-motion:reduce){.cui-row--enter{animation:none;}.cui-dot{animation:none;opacity:.5;}}
`;

function ensureStyle() {
  const doc = typeof document !== 'undefined' ? document : null;
  if (!doc || doc.getElementById(STYLE_ID)) return;
  const style = doc.createElement('style');
  style.id = STYLE_ID;
  style.textContent = STYLE;
  (doc.head || doc.documentElement).appendChild(style);
}

/* ------------------------------------------------------------------ *
 * View
 * ------------------------------------------------------------------ */

const SCROLL_THRESHOLD = 40;

function prefersReducedMotion() {
  try {
    return Boolean(window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches);
  } catch {
    return false;
  }
}

function wireCopy(row) {
  row.querySelectorAll('.cui-copy').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const code = btn.closest('.cui-codeblock')?.querySelector('code');
      const text = code ? code.textContent : '';
      try {
        await navigator.clipboard?.writeText?.(text);
        const prev = btn.textContent;
        btn.textContent = 'Copied';
        btn.classList.add('is-done');
        setTimeout(() => {
          btn.textContent = prev;
          btn.classList.remove('is-done');
        }, 1200);
      } catch {
        /* clipboard unavailable — leave the button as-is */
      }
    });
  });
}

export function createChatView(host, { onRetry } = {}) {
  if (!host) throw new Error('createChatView requires a host element');
  ensureStyle();

  const doc = host.ownerDocument || document;
  const thread = doc.createElement('div');
  thread.className = 'cui-thread';
  host.replaceChildren(thread);

  const nodes = new Map(); // key -> row element
  let typingEl = null;
  let emptyEl = null;
  let firstPaint = true;

  function ensureThreadAttached() {
    if (thread.parentNode !== host) host.replaceChildren(thread);
  }

  function insertAt(node, prev) {
    const anchor = prev ? prev.nextSibling : thread.firstChild;
    if (anchor !== node) thread.insertBefore(node, anchor);
  }

  function clearEmpty() {
    if (emptyEl) {
      emptyEl.remove();
      emptyEl = null;
    }
  }

  function clearTyping() {
    if (typingEl) {
      typingEl.remove();
      typingEl = null;
    }
  }

  function update({ events = [], running = false, emptyText = '' } = {}) {
    ensureThreadAttached();

    const atBottom =
      host.scrollHeight - host.scrollTop - host.clientHeight < SCROLL_THRESHOLD;
    const reduced = prefersReducedMotion();

    const used = new Set();
    const desired = (Array.isArray(events) ? events : []).map((event, index) => ({
      key: keyOf(event, index, used),
      ...renderRowHtml(event),
    }));

    // Empty state
    if (!desired.length && !running) {
      clearTyping();
      for (const [, el] of nodes) el.remove();
      nodes.clear();
      if (!emptyEl) {
        emptyEl = doc.createElement('div');
        emptyEl.className = 'cui-empty';
        thread.appendChild(emptyEl);
      }
      emptyEl.textContent = emptyText || '';
      firstPaint = false;
      return;
    }
    clearEmpty();

    // Reconcile rows by key (append-mostly, streaming-friendly)
    const nextKeys = new Set(desired.map((d) => d.key));
    for (const [key, el] of nodes) {
      if (!nextKeys.has(key)) {
        el.remove();
        nodes.delete(key);
      }
    }

    let prev = null;
    for (const d of desired) {
      let node = nodes.get(d.key);
      if (!node) {
        node = doc.createElement('div');
        node.className = reduced ? d.cls : `${d.cls} cui-row--enter`;
        node.innerHTML = d.html;
        node.__html = d.html;
        node.__cls = d.cls;
        wireCopy(node);
        nodes.set(d.key, node);
        insertAt(node, prev);
      } else {
        if (node.__html !== d.html) {
          node.innerHTML = d.html;
          node.__html = d.html;
          wireCopy(node);
        }
        if (node.__cls !== d.cls) {
          node.className = d.cls; // drop the one-shot enter class on updates
          node.__cls = d.cls;
        }
        insertAt(node, prev);
      }
      prev = node;
    }

    // Typing indicator after the last row
    if (running) {
      if (!typingEl) {
        typingEl = doc.createElement('div');
        typingEl.className = 'cui-typing';
        typingEl.setAttribute('aria-label', 'Assistant is typing');
        typingEl.innerHTML = '<span class="cui-dot"></span><span class="cui-dot"></span><span class="cui-dot"></span>';
      }
      thread.appendChild(typingEl);
    } else {
      clearTyping();
    }

    if (atBottom || firstPaint) {
      host.scrollTop = host.scrollHeight;
    }
    firstPaint = false;
  }

  function destroy() {
    clearTyping();
    clearEmpty();
    for (const [, el] of nodes) el.remove();
    nodes.clear();
    thread.remove();
  }

  // `onRetry` is reserved for a future per-message retry affordance; accepted
  // now so callers can wire it without an API change.
  void onRetry;

  return { update, destroy };
}

export const __chatUiTestHooks = {
  escapeHtml,
  renderInline,
  renderMarkdown,
  classifyRole,
  renderRowHtml,
};
