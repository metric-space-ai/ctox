// Shell-facing pins keep the reviewed raster icons effective even when an
// installed module catalog still contains an older SVG-only definition.
export const GROK_SHELL_ICON_SELECTION = Object.freeze({
  creator: Object.freeze({ asset: 'shared/assets/workjet-icons/grok-shell-v1/creator.png', sha256: 'acb089e621a25ecdd20a6fe10f289d61f340938326c2dae1252ded10af0e0fb3' }),
  desktop: Object.freeze({ asset: 'shared/assets/workjet-icons/grok-shell-v1/desktop.png', sha256: 'a29aad670cdcbb65e8864c81e30bb6b5713cdb5e27dc0f738a5534bf6d40c8f3' }),
  explorer: Object.freeze({ asset: 'shared/assets/workjet-icons/grok-shell-v1/explorer.png', sha256: '7ef0e24a291ce31f1bb6d86283cb0b90bd14a2023589bb9be6452553c84846b1' }),
  'file-viewer': Object.freeze({ asset: 'shared/assets/workjet-icons/grok-shell-v1/file-viewer.png', sha256: '78311622fe7fd9a6a63f09f39ed7cc87da3cc25c7a8449f770211b8c2d6169f8' }),
});

export function grokShellIconFor(moduleId) {
  const normalized = String(moduleId || '').replace(/^module:|^desktop-app:/, '');
  return GROK_SHELL_ICON_SELECTION[normalized] || null;
}
