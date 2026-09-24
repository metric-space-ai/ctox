const POINTER_THRESHOLDS = Object.freeze({
  mouse: Object.freeze({ enter: 16, exit: 30, switch: 8 }),
  pen: Object.freeze({ enter: 16, exit: 30, switch: 8 }),
  touch: Object.freeze({ enter: 24, exit: 44, switch: 12 }),
});

const WORKSPACE_ZONES = Object.freeze([
  'top-left',
  'top-right',
  'bottom-left',
  'bottom-right',
  'left',
  'right',
  'top',
  'bottom',
]);

function finite(value, fallback = 0) {
  const number = Number(value);
  return Number.isFinite(number) ? number : fallback;
}

export function normalizeRect(rect = {}) {
  const left = finite(rect.left ?? rect.x);
  const top = finite(rect.top ?? rect.y);
  const width = Math.max(0, finite(rect.width));
  const height = Math.max(0, finite(rect.height));
  return {
    left,
    top,
    width,
    height,
    right: finite(rect.right, left + width),
    bottom: finite(rect.bottom, top + height),
  };
}

function overlapLength(a0, a1, b0, b1) {
  return Math.max(0, Math.min(a1, b1) - Math.max(a0, b0));
}

function workspaceTargetRect(zone, workRect) {
  const work = normalizeRect(workRect);
  const halfWidth = Math.floor(work.width / 2);
  const halfHeight = Math.floor(work.height / 2);
  const rightWidth = work.width - halfWidth;
  const bottomHeight = work.height - halfHeight;
  const targets = {
    left: { left: work.left, top: work.top, width: halfWidth, height: work.height },
    right: { left: work.left + halfWidth, top: work.top, width: rightWidth, height: work.height },
    top: { left: work.left, top: work.top, width: work.width, height: halfHeight },
    bottom: { left: work.left, top: work.top + halfHeight, width: work.width, height: bottomHeight },
    'top-left': { left: work.left, top: work.top, width: halfWidth, height: halfHeight },
    'top-right': { left: work.left + halfWidth, top: work.top, width: rightWidth, height: halfHeight },
    'bottom-left': { left: work.left, top: work.top + halfHeight, width: halfWidth, height: bottomHeight },
    'bottom-right': { left: work.left + halfWidth, top: work.top + halfHeight, width: rightWidth, height: bottomHeight },
  };
  return targets[zone] || null;
}

function workspaceCandidates(sourceRect, workRect) {
  const source = normalizeRect(sourceRect);
  const work = normalizeRect(workRect);
  const distances = {
    left: Math.abs(source.left - work.left),
    right: Math.abs(source.right - work.right),
    top: Math.abs(source.top - work.top),
    bottom: Math.abs(source.bottom - work.bottom),
  };
  const result = [];
  for (const zone of WORKSPACE_ZONES) {
    const edges = zone.split('-');
    const distance = Math.max(...edges.map((edge) => distances[edge]));
    result.push({
      id: `workspace:${zone}`,
      kind: 'workspace',
      zone,
      targetId: 'workspace',
      distance,
      overlap: Number.POSITIVE_INFINITY,
      rect: workspaceTargetRect(zone, work),
      sourceEdge: edges.join('+'),
      targetEdge: edges.join('+'),
    });
  }
  return result;
}

function appCandidates(sourceRect, targetRects = [], workRect) {
  const source = normalizeRect(sourceRect);
  const work = normalizeRect(workRect);
  const result = [];
  for (const [index, entry] of targetRects.entries()) {
    if (!entry || entry.hidden || entry.minimized) continue;
    const target = normalizeRect(entry.rect || entry);
    const targetId = String(entry.ownerId || entry.id || `target-${index}`);
    const verticalOverlap = overlapLength(source.top, source.bottom, target.top, target.bottom);
    const horizontalOverlap = overlapLength(source.left, source.right, target.left, target.right);
    const requiredVertical = Math.min(96, Math.max(32, Math.min(source.height, target.height) * 0.25));
    const requiredHorizontal = Math.min(96, Math.max(32, Math.min(source.width, target.width) * 0.25));
    const definitions = [
      {
        sourceEdge: 'right', targetEdge: 'left', distance: Math.abs(source.right - target.left), overlap: verticalOverlap,
        required: requiredVertical, left: target.left - source.width, top: source.top,
      },
      {
        sourceEdge: 'left', targetEdge: 'right', distance: Math.abs(source.left - target.right), overlap: verticalOverlap,
        required: requiredVertical, left: target.right, top: source.top,
      },
      {
        sourceEdge: 'bottom', targetEdge: 'top', distance: Math.abs(source.bottom - target.top), overlap: horizontalOverlap,
        required: requiredHorizontal, left: source.left, top: target.top - source.height,
      },
      {
        sourceEdge: 'top', targetEdge: 'bottom', distance: Math.abs(source.top - target.bottom), overlap: horizontalOverlap,
        required: requiredHorizontal, left: source.left, top: target.bottom,
      },
    ];
    for (const definition of definitions) {
      if (definition.overlap < definition.required) continue;
      const left = Math.min(Math.max(definition.left, work.left), Math.max(work.left, work.right - source.width));
      const top = Math.min(Math.max(definition.top, work.top), Math.max(work.top, work.bottom - source.height));
      result.push({
        id: `app:${targetId}:${definition.sourceEdge}:${definition.targetEdge}`,
        kind: 'app',
        zone: '',
        targetId,
        targetIndex: index,
        distance: definition.distance,
        overlap: definition.overlap,
        sourceEdge: definition.sourceEdge,
        targetEdge: definition.targetEdge,
        rect: { left, top, width: source.width, height: source.height },
      });
    }
  }
  return result;
}

function candidateOrder(a, b) {
  if (a.kind !== b.kind) return a.kind === 'workspace' ? -1 : 1;
  const aCorner = a.kind === 'workspace' && a.zone.includes('-');
  const bCorner = b.kind === 'workspace' && b.zone.includes('-');
  if (aCorner !== bCorner) return aCorner ? -1 : 1;
  if (a.distance !== b.distance) return a.distance - b.distance;
  if (a.overlap !== b.overlap) return b.overlap - a.overlap;
  if ((a.targetIndex ?? -1) !== (b.targetIndex ?? -1)) return (b.targetIndex ?? -1) - (a.targetIndex ?? -1);
  return a.id.localeCompare(b.id);
}

export function resolveWindowLayout({
  sourceRect,
  workRect,
  targetRects = [],
  pointerType = 'mouse',
  previousCandidate = null,
  allowWorkspaceSnap = true,
} = {}) {
  if (!sourceRect || !workRect) return null;
  const thresholds = POINTER_THRESHOLDS[pointerType] || POINTER_THRESHOLDS.mouse;
  const candidates = [
    ...(allowWorkspaceSnap ? workspaceCandidates(sourceRect, workRect) : []),
    ...appCandidates(sourceRect, targetRects, workRect),
  ];
  const previousId = previousCandidate?.id || '';
  const previous = candidates.find((candidate) => candidate.id === previousId);
  const eligible = candidates.filter((candidate) => candidate.distance <= (
    candidate.id === previousId ? thresholds.exit : thresholds.enter
  ));
  if (!eligible.length) return null;
  eligible.sort(candidateOrder);
  const best = eligible[0];
  if (previous && previous.distance <= thresholds.exit && best.id !== previous.id) {
    if (best.distance + thresholds.switch >= previous.distance) return previous;
  }
  return best;
}

export function pointerThresholds(pointerType = 'mouse') {
  return { ...(POINTER_THRESHOLDS[pointerType] || POINTER_THRESHOLDS.mouse) };
}
