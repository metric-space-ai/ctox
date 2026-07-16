"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");

const {
  dragBytes,
  isManagedInstanceSender,
  sanitizeDragFilename,
} = require("../src/main/file-drag.cjs");

test("native file drag accepts only managed instance senders", () => {
  const sender = { id: 42, startDrag() {} };
  const views = new Map([["skf", { webContents: { id: 42 } }]]);
  assert.equal(isManagedInstanceSender(sender, views), true);
  assert.equal(isManagedInstanceSender({ id: 7, startDrag() {} }, views), false);
  assert.equal(isManagedInstanceSender({ id: 42 }, views), false);
});

test("native file drag sanitizes names and preserves binary bytes", () => {
  assert.equal(sanitizeDragFilename("../unsafe:report?.csv"), "_unsafe_report_.csv");
  const source = new Uint8Array([0, 127, 255]);
  assert.deepEqual([...dragBytes(source)], [0, 127, 255]);
  assert.throws(() => dragBytes("not-bytes"), /invalid/);
});
