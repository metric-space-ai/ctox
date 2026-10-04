// Mail body safety pipeline.
//
// The Mail renderer must display real mail bodies, which the old detail
// pane used to flatten to plaintext because `renderTimelineMessage` ignored
// `body_html`. Two hostile surfaces show up immediately:
//
//   * `body_html` from any inbound mail source is attacker controlled.
//   * `body_text` may also carry URLs we should let the operator click.
//
// The renderer below keeps every text-creating step behind the DOM API
// (`createElement`, `textContent`, `setAttribute`, `appendChild`). Raw HTML
// is parsed only inside inert template contents, never in the live DOM. The result is a sanitized
// tree the shell can drop into a regular container, with no chance for
// `<script>`, inline `on*` handlers, `javascript:`/`data:`/`vbscript:` URLs,
// `<style>` `background:url(...)` fetches, `<iframe>`, `<form>`, `<object>`,
// or auto-loaded tracking pixels to run.
//
// Allowlist policy (deliberately conservative for an email client):
//
//   * Tags: a, p, br, strong, em, b, i, u, s, span, div, blockquote, code,
//     pre, ul, ol, li, table, thead, tbody, tfoot, tr, td, th, caption,
//     hr, h1, h2, h3, h4, h5, h6, dl, dt, dd, small, sub, sup, cite, q,
//     abbr, address.
//   * Attributes: href on <a>, colspan/rowspan/align on table cells, lang on
//     every text-bearing tag. No `style`, no `class`, no event handlers,
//     no `src`/`srcset`/`loading`/`background` etc.
//   * URLs: only http(s) and mailto; everything else becomes a plain text
//     replacement so it cannot be executed or tracked. No auto-loading of
//     remote images. Images are represented by their alternative text.
//
// Body selection and URL validation are pure helpers. HTML parsing and
// allowlisted copying use the supplied ownerDocument. Browser acceptance
// must also verify parser behavior and the absence of resource requests.

// Strict allowlist. Order does not matter; the sanitizer does not rely on it.
const ALLOWED_TAGS = new Set([
  'a', 'p', 'br', 'strong', 'em', 'b', 'i', 'u', 's', 'span', 'div',
  'blockquote', 'code', 'pre', 'ul', 'ol', 'li', 'table', 'thead', 'tbody',
  'tfoot', 'tr', 'td', 'th', 'caption', 'hr', 'h1', 'h2', 'h3', 'h4', 'h5',
  'h6', 'dl', 'dt', 'dd', 'small', 'sub', 'sup', 'cite', 'q', 'abbr',
  'address', 'img',
]);

// Attributes that are safe to forward. Every value is sanitized again
// before it lands on the element, so allowlisting the name is enough.
const SAFE_ATTRS = {
  '*': new Set(['lang', 'dir', 'title']),
  a: new Set(['href', 'name']),
  td: new Set(['colspan', 'rowspan', 'align']),
  th: new Set(['colspan', 'rowspan', 'align', 'scope']),
  tr: new Set(['align']),
  colgroup: new Set(['span']),
  col: new Set(['span']),
  caption: new Set(['align']),
  img: new Set(['alt', 'width', 'height']),
};

// Schemes that are safe to put in a clickable URL.
const SAFE_URL_PROTOCOLS = new Set(['http:', 'https:', 'mailto:']);

// Tags whose presence would break the isolation guarantees. Even if the
// underlying attribute allowlist would let them through, we drop them.
const FORBIDDEN_TAGS = new Set([
  'script', 'style', 'head', 'title', 'iframe', 'frame', 'frameset', 'object', 'embed',
  'applet', 'form', 'input', 'textarea', 'select', 'option', 'button',
  'link', 'meta', 'base', 'noscript', 'template', 'slot', 'svg', 'math',
  'video', 'audio', 'source', 'track', 'picture', 'canvas', 'map', 'area',
  'marquee', 'animation', 'portal', 'fencedframe',
]);

// Identify the body variant a message carries. The communication_messages
// projection carries multipart payloads; the body_html field may be null
// when the mail is text-only or when the parser projected only body_text.
export function selectMessageBody(message) {
  if (!message || typeof message !== 'object') {
    return { kind: 'empty', value: '', html: '', text: '' };
  }
  const html = typeof message.body_html === 'string' ? message.body_html.trim() : '';
  const text = typeof message.body_text === 'string' ? message.body_text
    : (typeof message.preview === 'string' ? message.preview : '');
  if (html) {
    return { kind: 'html', value: html, html, text };
  }
  if (text && text.trim()) {
    return { kind: 'text', value: text, html: '', text };
  }
  return { kind: 'empty', value: '', html: '', text: '' };
}

// Normalize a URL candidate. Anything other than a safe http/https/mailto
// scheme is rejected. The original token is preserved as plain text in
// the rendered body so the operator can still see what was attempted.
export function isSafeHttpUrl(value) {
  if (typeof value !== 'string') return false;
  const trimmed = value.trim();
  if (!trimmed) return false;
  if (/[\u0000-\u001f\u007f]/.test(trimmed)) return false;
  let parsed;
  try {
    parsed = new URL(trimmed);
    return SAFE_URL_PROTOCOLS.has(parsed.protocol.toLowerCase());
  } catch {
    const lower = trimmed.toLowerCase();
    const prefix = lower.split(':', 1)[0] + ':';
    if (!SAFE_URL_PROTOCOLS.has(prefix)) return false;
    if (prefix === 'mailto:') {
      return /^mailto:[^@\s]+@[^@\s]+$/i.test(trimmed);
  }
    // http/https + anything unparseable is still rejected; the operator gets
    // to see the literal text in the body instead of a clickable decoy.
    return false;
  }
}

export function extractSafeUrl(value) {
  if (typeof value !== 'string') return null;
  const trimmed = value.trim();
  if (!trimmed) return null;
  return isSafeHttpUrl(trimmed) ? trimmed : null;
}

const URL_REGEX = /\b((?:https?:\/\/|mailto:)[^\s<>"')]+)/gi;

function appendSafeText(target, value) {
  target.appendChild(target.ownerDocument.createTextNode(value));
}

function appendSafeAnchor(target, href, text) {
  const safeHref = extractSafeUrl(href);
  if (!safeHref) {
    appendSafeText(target, text);
    return;
  }
  const ownerDocument = target.ownerDocument;
  const anchor = ownerDocument.createElement('a');
  anchor.setAttribute('href', safeHref);
  anchor.setAttribute('rel', 'noopener noreferrer nofollow');
  anchor.setAttribute('target', '_blank');
  anchor.appendChild(ownerDocument.createTextNode(text));
  target.appendChild(anchor);
}

// Plaintext → safe DOM tree. Replaces bare URLs with clickable anchors
// and keeps line breaks. Used both as a fallback when body_html is missing
// and inside the sanitizer when an unsafe element gets reduced to its text.
function appendPlainTextNodes(target, text) {
  if (!text) return;
  const ownerDocument = target.ownerDocument;
  const lines = String(text).replace(/\r\n?/g, '\n').split('\n');
  lines.forEach((line, index) => {
    let cursor = 0;
    URL_REGEX.lastIndex = 0;
    let match;
    while ((match = URL_REGEX.exec(line)) !== null) {
      if (match.index > cursor) {
        appendSafeText(target, line.slice(cursor, match.index));
      }
      appendSafeAnchor(target, match[1], match[1]);
      cursor = match.index + match[0].length;
    }
    if (cursor < line.length) appendSafeText(target, line.slice(cursor));
    if (index < lines.length - 1) target.appendChild(ownerDocument.createElement('br'));
  });
}

// Parse raw HTML into inert template contents before copying allowlisted
// nodes. Parsing itself must not start remote resource requests.
export function sanitizeBodyHtml(html, ownerDocument = globalThis.document) {
  if (!html || typeof html !== 'string') return null;
  let parsed;
  try {
    // Template contents stay inert while parsing, including image resources.
    // A detached DOMParser document may start remote image requests before
    // its output is sanitized, so never use it for attacker-controlled mail.
    if (!ownerDocument?.createElement) return null;
    const template = ownerDocument.createElement('template');
    template.innerHTML = html;
    parsed = { body: template.content };
  } catch {
    return null;
  }
  if (!parsed || !parsed.body) return null;
  return parsed;
}

// Strip disallowed attributes. Returns the surviving attribute map so the
// caller can set them on a freshly created element via setAttribute.
function sanitizeAttributes(source, tagName) {
  const out = {};
  if (!source || !source.attributes) return out;
  const allowed = new Set([
    ...(SAFE_ATTRS['*'] || []),
    ...(SAFE_ATTRS[tagName] || []),
  ]);
  for (const attribute of Array.from(source.attributes)) {
    const name = attribute.name.toLowerCase();
    if (!allowed.has(name)) continue;
    if (name === 'href') {
      const safe = extractSafeUrl(attribute.value);
      if (safe) out.href = safe;
      continue;
    }
    if (name === 'colspan' || name === 'rowspan' || name === 'span') {
      const numeric = Number(attribute.value);
      if (Number.isFinite(numeric) && numeric > 0 && numeric <= 1000) {
        out[name] = String(Math.trunc(numeric));
      }
      continue;
    }
    if (name === 'align') {
      const lower = String(attribute.value).toLowerCase();
      if (['left', 'right', 'center', 'justify'].includes(lower)) {
        out.align = lower;
      }
      continue;
    }
    if (name === 'scope') {
      const lower = String(attribute.value).toLowerCase();
      if (['row', 'col', 'rowgroup', 'colgroup'].includes(lower)) {
        out.scope = lower;
      }
      continue;
    }
    if (name === 'width' || name === 'height') {
      const numeric = Number(attribute.value);
      if (Number.isFinite(numeric) && numeric > 0 && numeric <= 4000) {
        out[name] = String(Math.trunc(numeric));
      } else if (/^\d{1,4}(\.\d+)?%$/.test(attribute.value)) {
        out[name] = String(attribute.value).slice(0, 8);
      }
      continue;
    }
    if (name === 'lang' || name === 'dir' || name === 'title' || name === 'alt' || name === 'name') {
      out[name] = String(attribute.value).slice(0, 400);
    }
  }
  return out;
}

// Walk the parsed DOM and copy safe nodes into the target. Tags we do not
// recognize get reduced to their text content (recursively sanitized) but
// never re-emit the dangerous tag. There are no live event handlers and no
// remote fetches in this code path.
function appendSanitized(source, target) {
  if (!source || !target) return false;
  const ownerDocument = target.ownerDocument || target;
  let hasVisibleContent = false;
  const stack = [{ nodes: Array.from(source.childNodes || []), index: 0, target }];
  while (stack.length) {
    const frame = stack[stack.length - 1];
    if (frame.index >= frame.nodes.length) {
      stack.pop();
      continue;
    }
    const node = frame.nodes[frame.index++];
    if (node.nodeType === 3) {
      const text = String(node.nodeValue || '');
      appendSafeText(frame.target, text);
      hasVisibleContent ||= Boolean(text.trim());
      continue;
    }
    if (node.nodeType !== 1) continue;
    const tag = String(node.tagName || '').toLowerCase();
    if (FORBIDDEN_TAGS.has(tag)) {
      // Drop the subtree wholesale — scripts, styles, iframes, forms, etc.
      // cannot be expressed safely even with reduced content. This also
      // neutralizes any CSS-based fetch attempts in <iframe srcdoc>.
      continue;
    }
    if (tag === 'img') {
      const alt = String(node.getAttribute('alt') || '').trim();
      appendSafeText(frame.target, alt ? `[Bild blockiert] ${alt}` : '[Bild blockiert]');
      hasVisibleContent = true;
      continue;
    }
    if (!ALLOWED_TAGS.has(tag)) {
      // Preserve safe children without emitting the unknown element.
      stack.push({ nodes: Array.from(node.childNodes || []), index: 0, target: frame.target });
      continue;
    }
    const clone = ownerDocument.createElement(tag);
    const attrs = sanitizeAttributes(node, tag);
    for (const [key, value] of Object.entries(attrs)) clone.setAttribute(key, value);
    if (tag === 'a' && attrs.href) {
      clone.setAttribute('rel', 'noopener noreferrer nofollow');
      clone.setAttribute('target', '_blank');
      clone.setAttribute('referrerpolicy', 'no-referrer');
    }
    frame.target.appendChild(clone);
    stack.push({ nodes: Array.from(node.childNodes || []), index: 0, target: clone });
  }
  return hasVisibleContent;
}

// Public entry point: render a mail body into the supplied container. The
// container is wiped before we append so the caller can call this on every
// re-render. Remote images remain blocked for every body.
export function mountMailBody(container, message) {
  if (!container || !container.ownerDocument) return null;
  const doc = container.ownerDocument;
  const selection = selectMessageBody(message);

  while (container.firstChild) container.removeChild(container.firstChild);
  container.dataset.mailBodyKind = selection.kind;
  if (selection.kind === 'empty') {
    container.classList.add('is-empty');
    container.appendChild(doc.createTextNode(''));
    return selection;
  }
  container.classList.remove('is-empty');
  if (selection.kind === 'text') {
    const wrapper = doc.createElement('div');
    wrapper.className = 'mail-body-text';
    appendPlainTextNodes(wrapper, selection.text);
    container.appendChild(wrapper);
    return selection;
  }
  const sanitized = sanitizeBodyHtml(selection.html, doc);
  const wrapper = doc.createElement('div');
  const hasHtmlContent = sanitized && appendSanitized(sanitized.body, wrapper);
  if (!sanitized || (!hasHtmlContent && selection.text.trim())) {
    while (wrapper.firstChild) wrapper.removeChild(wrapper.firstChild);
    wrapper.className = 'mail-body-text';
    appendPlainTextNodes(wrapper, selection.text || '');
    container.appendChild(wrapper);
    container.dataset.mailBodyKind = 'text';
    return { kind: 'text', value: selection.text || '', html: '', text: selection.text || '' };
  }
  wrapper.className = 'mail-body-html';
  container.appendChild(wrapper);
  return selection;
}

export const __mailBodyTestHooks = {
  ALLOWED_TAGS,
  FORBIDDEN_TAGS,
  SAFE_URL_PROTOCOLS,
  SAFE_ATTRS,
  URL_REGEX,
  isSafeHttpUrl,
  extractSafeUrl,
  sanitizeBodyHtml,
};
