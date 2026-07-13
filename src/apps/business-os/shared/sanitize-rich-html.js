const ALLOWED_ELEMENTS = new Set([
  'a', 'abbr', 'b', 'blockquote', 'br', 'code', 'col', 'colgroup', 'del', 'details', 'div', 'em',
  'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'hr', 'i', 'img', 'input', 'ins', 'kbd', 'li', 'mark',
  'ol', 'p', 'pre', 's', 'samp', 'small', 'span', 'strong', 'sub', 'summary', 'sup', 'table',
  'tbody', 'td', 'tfoot', 'th', 'thead', 'tr', 'u', 'ul',
]);

const DROP_CONTENT_ELEMENTS = new Set([
  'base',
  'embed',
  'iframe',
  'link',
  'math',
  'meta',
  'object',
  'script',
  'style',
  'svg',
]);

const ALLOWED_ATTRIBUTES = new Set([
  'alt', 'checked', 'class', 'colspan', 'disabled', 'height', 'href', 'id', 'open', 'rel',
  'reversed', 'rowspan', 'scope', 'src', 'start', 'style', 'target', 'title', 'type', 'width',
]);

const URL_ATTRIBUTES = new Set(['action', 'formaction', 'href', 'poster', 'src', 'xlink:href']);

function safeUrl(value, attribute) {
  const candidate = String(value || '').trim();
  if (!candidate) return true;
  if (candidate.startsWith('#') || candidate.startsWith('/') || candidate.startsWith('./') || candidate.startsWith('../')) {
    return true;
  }
  if (attribute === 'src' && /^data:image\/(?:avif|gif|jpeg|png|webp);base64,/i.test(candidate)) {
    return true;
  }
  try {
    const url = new URL(candidate, window.location.href);
    return ['http:', 'https:', 'mailto:', 'tel:'].includes(url.protocol);
  } catch {
    return false;
  }
}

function safeStyle(value) {
  const style = String(value || '');
  return !/(?:url\s*\(|expression\s*\(|@import|behavior\s*:|-moz-binding)/i.test(style);
}

export function sanitizeRichHtml(value) {
  const parser = new DOMParser();
  const document = parser.parseFromString(`<body>${String(value || '')}</body>`, 'text/html');
  const elements = [...document.body.querySelectorAll('*')];

  for (const element of elements) {
    const tagName = element.tagName.toLowerCase();
    if (!ALLOWED_ELEMENTS.has(tagName)) {
      if (DROP_CONTENT_ELEMENTS.has(tagName)) element.remove();
      else element.replaceWith(...element.childNodes);
      continue;
    }
    for (const attribute of [...element.attributes]) {
      const name = attribute.name.toLowerCase();
      if (!ALLOWED_ATTRIBUTES.has(name) && !name.startsWith('aria-') && !name.startsWith('data-')) {
        element.removeAttribute(attribute.name);
        continue;
      }
      if (name.startsWith('on') || name === 'srcdoc') {
        element.removeAttribute(attribute.name);
        continue;
      }
      if (URL_ATTRIBUTES.has(name) && !safeUrl(attribute.value, name)) {
        element.removeAttribute(attribute.name);
        continue;
      }
      if (name === 'style' && !safeStyle(attribute.value)) {
        element.removeAttribute(attribute.name);
      }
    }
    if (element.getAttribute('target') === '_blank') {
      const rel = new Set((element.getAttribute('rel') || '').split(/\s+/).filter(Boolean));
      rel.add('noopener');
      rel.add('noreferrer');
      element.setAttribute('rel', [...rel].join(' '));
    }
    if (tagName === 'input' && element.getAttribute('type')?.toLowerCase() !== 'checkbox') {
      element.remove();
    }
  }
  return document.body.innerHTML;
}
