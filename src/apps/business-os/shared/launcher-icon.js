import { operatorIconFor } from './operator-icon-selection.js';
import { grokShellIconFor } from './grok-shell-icon-selection.js';

export function resolveLauncherIcon(target, { fallbackSvg = '' } = {}) {
  const kind = String(target?.kind || '');
  const id = String(target?.id || '');
  const moduleLayout = target?.module?.layout || {};
  const selectedIcon = kind === 'module' ? operatorIconFor(id) || grokShellIconFor(id) : null;
  const rasterAsset = String(
    selectedIcon?.asset
      || (kind === 'module' ? moduleLayout.icon_asset : target?.app?.iconAsset)
      || '',
  ).trim();

  if (rasterAsset) return { kind: 'raster', asset: rasterAsset };

  const svg = String(
    (kind === 'module' ? moduleLayout.icon_svg : '')
      || fallbackSvg
      || '',
  ).trim();
  if (svg) return { kind: 'svg', markup: svg };

  return {
    kind: 'text',
    text: String(target?.glyph || target?.title?.charAt?.(0) || '◻'),
  };
}
