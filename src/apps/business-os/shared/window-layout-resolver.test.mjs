import assert from 'node:assert/strict';
import { pointerThresholds, resolveWindowLayout } from './window-layout-resolver.js';

const workRect = { left: 0, top: 0, width: 1200, height: 800 };

assert.deepEqual(pointerThresholds('mouse'), { enter: 16, exit: 30, switch: 8 });
assert.deepEqual(pointerThresholds('touch'), { enter: 24, exit: 44, switch: 12 });

const left = resolveWindowLayout({
  sourceRect: { left: 10, top: 180, width: 500, height: 420 },
  workRect,
});
assert.equal(left.id, 'workspace:left');
assert.deepEqual(left.rect, { left: 0, top: 0, width: 600, height: 800 });

const corner = resolveWindowLayout({
  sourceRect: { left: 8, top: 9, width: 500, height: 420 },
  workRect,
});
assert.equal(corner.id, 'workspace:top-left');

for (const [zone, sourceRect] of Object.entries({
  left: { left: 7, top: 180, width: 500, height: 420 },
  right: { left: 693, top: 180, width: 500, height: 420 },
  top: { left: 350, top: 7, width: 500, height: 420 },
  bottom: { left: 350, top: 373, width: 500, height: 420 },
  'top-left': { left: 7, top: 7, width: 500, height: 420 },
  'top-right': { left: 693, top: 7, width: 500, height: 420 },
  'bottom-left': { left: 7, top: 373, width: 500, height: 420 },
  'bottom-right': { left: 693, top: 373, width: 500, height: 420 },
})) {
  assert.equal(resolveWindowLayout({ sourceRect, workRect })?.id, `workspace:${zone}`);
}

const appDock = resolveWindowLayout({
  sourceRect: { left: 394, top: 120, width: 400, height: 420 },
  workRect,
  targetRects: [{ id: 'desktop-app:tickets', rect: { left: 800, top: 100, width: 360, height: 500 } }],
});
assert.equal(appDock.id, 'app:desktop-app:tickets:right:left');
assert.equal(appDock.rect.left, 400);

const appDockBelow = resolveWindowLayout({
  sourceRect: { left: 780, top: 394, width: 360, height: 200 },
  workRect,
  targetRects: [{ id: 'desktop-app:tickets', rect: { left: 760, top: 100, width: 400, height: 300 } }],
});
assert.equal(appDockBelow.id, 'app:desktop-app:tickets:top:bottom');
assert.equal(appDockBelow.rect.top, 400);

const appDockLeft = resolveWindowLayout({
  sourceRect: { left: 406, top: 120, width: 340, height: 360 },
  workRect,
  targetRects: [{ id: 'desktop-app:tickets', rect: { left: 60, top: 100, width: 340, height: 420 } }],
});
assert.equal(appDockLeft.id, 'app:desktop-app:tickets:left:right');
assert.equal(appDockLeft.rect.left, 400);

const appDockAbove = resolveWindowLayout({
  sourceRect: { left: 300, top: 94, width: 360, height: 200 },
  workRect,
  targetRects: [{ id: 'desktop-app:tickets', rect: { left: 280, top: 300, width: 400, height: 300 } }],
});
assert.equal(appDockAbove.id, 'app:desktop-app:tickets:bottom:top');
assert.equal(appDockAbove.rect.top, 100);

const touchOnly = resolveWindowLayout({
  sourceRect: { left: 22, top: 180, width: 500, height: 420 },
  workRect,
  pointerType: 'touch',
});
assert.equal(touchOnly.id, 'workspace:left');
assert.equal(resolveWindowLayout({
  sourceRect: { left: 22, top: 180, width: 500, height: 420 },
  workRect,
  pointerType: 'mouse',
}), null);
assert.equal(resolveWindowLayout({
  sourceRect: { left: 10, top: 180, width: 500, height: 420 },
  workRect,
  allowWorkspaceSnap: false,
}), null);

const previous = resolveWindowLayout({
  sourceRect: { left: 12, top: 180, width: 500, height: 420 },
  workRect,
});
const retained = resolveWindowLayout({
  sourceRect: { left: 28, top: 180, width: 500, height: 420 },
  workRect,
  previousCandidate: previous,
});
assert.equal(retained.id, previous.id);

assert.equal(resolveWindowLayout({
  sourceRect: { left: 45, top: 180, width: 500, height: 420 },
  workRect,
  previousCandidate: previous,
}), null);

console.log('window layout resolver contract OK');
