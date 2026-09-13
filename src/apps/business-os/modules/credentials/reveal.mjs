import { CREDENTIAL_REVEAL_METHOD } from '../../shared/native-request-privacy.mjs';

// Preserve exact bytes: copying must not trim or otherwise alter a password.
// These aliases mirror native resolve_web_stack_credential. Unknown formats
// remain available as the complete raw value, including scalar API keys.
export function credentialFields(value) {
  let object;
  try { object = JSON.parse(value); } catch { /* scalar */ }
  if (object && typeof object === 'object' && !Array.isArray(object)) {
    const first = keys => keys.find(key => typeof object[key] === 'string');
    const password = first(['password', 'credential', 'secret']);
    const username = first(['username', 'email', 'login', 'login_hint']);
    if (password) return [
      ...(username ? [{ label: 'username', value: object[username] }] : []),
      { label: 'password', value: object[password] },
    ];
  }
  return [{ label: 'valueLabel', value }];
}

// No storage/command/notification surface receives a value. Only explicit
// display and clipboard actions do. Clearing drops references; JS cannot
// promise physical zeroization of strings or the user's clipboard history.
export function mountCredentialReveal({ host, name, allowed, sync, t,
  clipboard = globalThis.navigator?.clipboard,
  windowTarget = globalThis.window, documentTarget = globalThis.document,
  schedule = globalThis.setTimeout, cancel = globalThis.clearTimeout,
}) {
  const doc = host.ownerDocument || documentTarget;
  let epoch = 0, disposed = false, pending = false, timer = null;
  let fields = null;
  function node(tag, content) {
    const el = doc.createElement(tag);
    if (content !== undefined) el.textContent = content;
    return el;
  }
  function button(label, action, index) {
    const el = node('button', t(label));
    el.type = 'button';
    el.className = 'ctox-button';
    el.dataset.revealAction = action;
    if (index !== undefined) el.dataset.revealIndex = String(index);
    el.disabled = !allowed || (pending && action !== 'hide');
    return el;
  }
  function render(message = '') {
    host.replaceChildren();
    if (disposed) return;
    const actions = node('div');
    actions.className = 'ctox-compact-form__actions';
    actions.append(button(fields || pending ? 'hide_btn' : 'show_btn', fields || pending ? 'hide' : 'show'));
    host.append(actions);
    if (fields) fields.forEach((field, index) => {
      const row = node('div');
      row.className = 'ctox-compact-field';
      row.append(node('span', t(field.label)));
      const content = node('pre', field.value);
      content.style.whiteSpace = 'pre-wrap';
      content.style.overflowWrap = 'anywhere';
      // textContent, not HTML, input defaults, attributes, or a data record.
      const copy = button('copy_btn', 'copy', index);
      copy.setAttribute('aria-label', `${t('copy_btn')} ${t(field.label)}`);
      row.append(content, copy);
      host.append(row);
    });
    else host.append(node('span', '••••••••'), button('copy_btn', 'copy-raw'));
    if (message) {
      const status = node('p', t(message));
      status.setAttribute('role', 'status');
      host.append(status);
    }
  }
  function hide() {
    epoch += 1;
    fields = null;
    if (timer !== null) cancel(timer);
    timer = null;
    render();
  }
  async function read() {
    // Never fall back to requestNative: an old shell can relay its result via
    // an old leader, which cannot know the new method is private.
    if (typeof sync?.requestPrivateNative !== 'function') {
      const error = new Error('An updated private native channel is required.');
      error.code = 'credential_reveal_private_channel_required';
      throw error;
    }
    const result = await sync.requestPrivateNative(CREDENTIAL_REVEAL_METHOD, { name }, {
      collection: 'business_commands', timeoutMs: 10000,
    });
    if (result?.schema !== 'ctox.credential-reveal.v1' || result.name !== name
      || typeof result.value !== 'string' || result.value.length > 65536) {
      throw new Error('credential_reveal_invalid_response');
    }
    return result.value;
  }
  async function onClick(event) {
    const target = event.target?.closest?.('[data-reveal-action]');
    if (!target || !host.contains(target) || !allowed || disposed) return;
    const action = target.dataset.revealAction;
    if (action === 'hide') { hide(); return; }
    if (pending) return;
    if (action === 'copy' && fields) {
      const field = fields[Number(target.dataset.revealIndex)];
      if (!field) return;
      const turn = epoch;
      try {
        if (!clipboard?.writeText) throw new Error('clipboard unavailable');
        await clipboard.writeText(field.value);
        if (!disposed && turn === epoch) render('copied');
      } catch { if (!disposed && turn === epoch) render('copy_failed'); }
      return;
    }
    if (action !== 'show' && action !== 'copy-raw') return;
    const turn = ++epoch;
    pending = true;
    render();
    let message = '';
    try {
      const value = await read();
      if (disposed || turn !== epoch) return;
      if (action === 'show') {
        fields = credentialFields(value);
        timer = schedule(hide, 30000);
      } else {
        if (!clipboard?.writeText) throw new Error('clipboard unavailable');
        await clipboard.writeText(value);
        message = 'copied';
      }
    } catch (error) {
      message = error?.code === 'credential_reveal_direct_tab_required'
        ? 'direct_tab_required' : error?.code === 'credential_reveal_private_channel_required'
          ? 'private_channel_required' : action === 'copy-raw' ? 'copy_failed' : 'reveal_failed';
    } finally {
      pending = false;
      if (!disposed) render(turn === epoch ? message : '');
    }
  }
  const onVisibility = () => { if (documentTarget?.hidden) hide(); };
  host.addEventListener('click', onClick);
  windowTarget?.addEventListener?.('blur', hide);
  windowTarget?.addEventListener?.('pagehide', hide);
  documentTarget?.addEventListener?.('visibilitychange', onVisibility);
  render();
  return () => {
    disposed = true;
    hide();
    host.removeEventListener('click', onClick);
    windowTarget?.removeEventListener?.('blur', hide);
    windowTarget?.removeEventListener?.('pagehide', hide);
    documentTarget?.removeEventListener?.('visibilitychange', onVisibility);
  };
}
