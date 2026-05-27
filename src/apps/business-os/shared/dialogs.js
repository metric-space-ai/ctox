const DIALOG_STYLE_ID = 'ctox-business-dialog-style';
let fallbacksInstalled = false;

export function installBusinessDialogFallbacks() {
  if (fallbacksInstalled) return;
  fallbacksInstalled = true;
  const nativeAlert = window.alert?.bind(window);
  window.__ctoxNativeAlert = window.__ctoxNativeAlert || nativeAlert;
  window.alert = (message) => {
    showBusinessAlert(message);
  };
}

export function showBusinessAlert(message, options = {}) {
  return openBusinessDialog({
    ...options,
    message,
    confirmLabel: options.confirmLabel || 'OK',
    cancelLabel: '',
    kind: options.kind || 'info',
  }).then(() => undefined);
}

export function showBusinessConfirm(message, options = {}) {
  return openBusinessDialog({
    ...options,
    message,
    confirmLabel: options.confirmLabel || 'Bestätigen',
    cancelLabel: options.cancelLabel || 'Abbrechen',
    kind: options.kind || 'danger',
  });
}

export function showBusinessPrompt(message, options = {}) {
  return openBusinessDialog({
    ...options,
    message,
    confirmLabel: options.confirmLabel || 'Übernehmen',
    cancelLabel: options.cancelLabel || 'Abbrechen',
    defaultValue: options.defaultValue || '',
    prompt: true,
    kind: options.kind || 'info',
  });
}

function openBusinessDialog({
  title = '',
  message = '',
  confirmLabel = 'OK',
  cancelLabel = '',
  defaultValue = '',
  requireText = '',
  prompt = false,
  kind = 'info',
} = {}) {
  installDialogStyles();
  const confirmationText = String(requireText || '').trim();
  const layer = document.createElement('div');
  layer.className = `business-dialog-layer is-${kind}`;
  layer.innerHTML = `
    <section class="business-dialog" role="dialog" aria-modal="true" aria-labelledby="businessDialogTitle">
      <div class="business-dialog-copy">
        <h2 id="businessDialogTitle">${escapeHtml(title || dialogTitleForKind(kind, prompt))}</h2>
        <p>${escapeHtml(message).replace(/\n/g, '<br>')}</p>
      </div>
      ${prompt ? `<input class="business-dialog-input" data-dialog-input value="${escapeAttr(defaultValue)}">` : ''}
      ${confirmationText ? `
        <label class="business-dialog-confirmation">
          <span>${escapeHtml(confirmTextLabel(kind))}</span>
          <code>${escapeHtml(confirmationText)}</code>
          <input class="business-dialog-input" data-dialog-confirmation autocomplete="off" spellcheck="false">
        </label>
      ` : ''}
      <div class="business-dialog-actions">
        ${cancelLabel ? `<button class="business-dialog-secondary" type="button" data-dialog-cancel>${escapeHtml(cancelLabel)}</button>` : ''}
        <button class="business-dialog-primary" type="button" data-dialog-confirm${confirmationText ? ' disabled' : ''}>${escapeHtml(confirmLabel)}</button>
      </div>
    </section>
  `;
  document.body.append(layer);

  const panel = layer.querySelector('.business-dialog');
  const input = layer.querySelector('[data-dialog-input]');
  const confirmationInput = layer.querySelector('[data-dialog-confirmation]');
  const confirm = layer.querySelector('[data-dialog-confirm]');
  const cancel = layer.querySelector('[data-dialog-cancel]');

  confirmationInput?.addEventListener('input', () => {
    confirm.disabled = confirmationInput.value.trim() !== confirmationText;
  });

  return new Promise((resolve) => {
    let done = false;
    const close = (value) => {
      if (done) return;
      done = true;
      document.removeEventListener('keydown', onKeydown);
      layer.classList.add('is-closing');
      window.setTimeout(() => {
        layer.remove();
        resolve(value);
      }, 120);
    };
    const onKeydown = (event) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        close(prompt ? null : false);
      }
      if (event.key === 'Enter' && canConfirmWithEnter({ kind, prompt, input, confirmationInput, confirm })) {
        event.preventDefault();
        close(prompt ? input.value : true);
      }
    };
    document.addEventListener('keydown', onKeydown);
    confirm?.addEventListener('click', () => {
      if (confirm.disabled) return;
      close(prompt ? input.value : true);
    });
    cancel?.addEventListener('click', () => close(prompt ? null : false));
    layer.addEventListener('pointerdown', (event) => {
      if (event.target === layer) close(prompt ? null : false);
    });
    window.requestAnimationFrame(() => {
      layer.classList.add('is-open');
      (input || confirmationInput || (kind === 'danger' && cancel ? cancel : confirm) || panel)?.focus?.();
      input?.select?.();
    });
  });
}

function canConfirmWithEnter({ kind, prompt, input, confirmationInput, confirm }) {
  if (prompt) return document.activeElement === input;
  if (confirmationInput) return document.activeElement === confirmationInput && !confirm?.disabled;
  if (kind === 'danger') return document.activeElement === confirm;
  return true;
}

function dialogTitleForKind(kind, prompt) {
  if (prompt) return 'Eingabe';
  if (kind === 'danger') return 'Destruktive Aktion bestätigen';
  return 'Hinweis';
}

function confirmTextLabel(kind) {
  return kind === 'danger'
    ? 'Zum Fortfahren exakt eingeben:'
    : 'Zur Bestätigung eingeben:';
}

function installDialogStyles() {
  if (document.getElementById(DIALOG_STYLE_ID)) return;
  const style = document.createElement('style');
  style.id = DIALOG_STYLE_ID;
  style.textContent = `
    .business-dialog-layer {
      position: fixed;
      inset: 0;
      z-index: 240;
      display: grid;
      place-items: center;
      padding: 24px;
      background: color-mix(in srgb, var(--bg, #101418) 42%, transparent);
      opacity: 0;
      pointer-events: none;
      transition: opacity 120ms ease-out;
    }
    .business-dialog-layer.is-open {
      opacity: 1;
      pointer-events: auto;
    }
    .business-dialog-layer.is-closing {
      opacity: 0;
      pointer-events: none;
    }
    .business-dialog {
      width: min(420px, calc(100vw - 48px));
      border: 1px solid var(--hairline, var(--line));
      border-radius: 12px;
      background: color-mix(in srgb, var(--surface) 96%, var(--bg));
      color: var(--text);
      box-shadow: 0 26px 80px rgba(0, 0, 0, .44);
      padding: 16px;
      transform: translateY(8px) scale(.98);
      transition: transform 120ms ease-out;
    }
    .business-dialog-layer.is-open .business-dialog {
      transform: translateY(0) scale(1);
    }
    .business-dialog-copy {
      display: grid;
      gap: 8px;
    }
    .business-dialog h2 {
      margin: 0;
      color: var(--text);
      font-size: 14px;
      line-height: 1.2;
      font-weight: 800;
      letter-spacing: 0;
    }
    .business-dialog p {
      margin: 0;
      color: var(--muted);
      font-size: 12px;
      line-height: 1.45;
    }
    .business-dialog-input {
      width: 100%;
      margin-top: 14px;
      border: 1px solid var(--hairline, var(--line));
      border-radius: 8px;
      background: color-mix(in srgb, var(--surface-2) 72%, var(--surface));
      color: var(--text);
      padding: 9px 10px;
      font: inherit;
    }
    .business-dialog-confirmation {
      display: grid;
      gap: 6px;
      margin-top: 14px;
      color: var(--muted);
      font-size: 11px;
      font-weight: 720;
    }
    .business-dialog-confirmation code {
      width: fit-content;
      max-width: 100%;
      overflow-wrap: anywhere;
      border: 1px solid color-mix(in srgb, var(--danger, #ef7f78) 34%, var(--line));
      border-radius: 6px;
      background: color-mix(in srgb, var(--danger, #ef7f78) 10%, var(--surface));
      color: var(--text);
      padding: 3px 6px;
      font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
      font-size: 11px;
    }
    .business-dialog-confirmation .business-dialog-input {
      margin-top: 0;
    }
    .business-dialog-actions {
      display: flex;
      justify-content: flex-end;
      gap: 8px;
      margin-top: 16px;
    }
    .business-dialog-actions button {
      min-height: 32px;
      border-radius: 8px;
      padding: 0 12px;
      font: inherit;
      font-weight: 760;
      cursor: pointer;
    }
    .business-dialog-actions button:disabled {
      cursor: not-allowed;
      opacity: .46;
      filter: grayscale(.25);
    }
    .business-dialog-secondary {
      border: 1px solid var(--hairline, var(--line));
      background: transparent;
      color: var(--muted);
    }
    .business-dialog-primary {
      border: 1px solid color-mix(in srgb, var(--accent) 42%, var(--line));
      background: color-mix(in srgb, var(--accent) 16%, var(--surface));
      color: var(--accent);
    }
    .business-dialog-layer.is-danger .business-dialog-primary {
      border-color: color-mix(in srgb, #ef7f78 48%, var(--line));
      background: color-mix(in srgb, #ef7f78 14%, var(--surface));
      color: #ffaaa4;
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

function escapeAttr(value) {
  return escapeHtml(value).replace(/`/g, '&#96;');
}
