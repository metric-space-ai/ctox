"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { summarizeAccessRevocationBlock } = require("../scripts/ctox-dev-live-contract.cjs");

test("access revocation contract accepts needs_auth with local launch block", () => {
  assert.deepEqual(
    summarizeAccessRevocationBlock({
      postRevocationInstance: { status: "needs_auth" },
      launchAfterRevocationError: "ctox.dev managed instance is not launchable: needs_auth",
    }),
    {
      postRevocationStatus: "needs_auth",
      launchAfterRevocationBlocked: true,
      launchAfterRevocationError: "ctox.dev managed instance is not launchable: needs_auth",
    },
  );
});

test("access revocation contract accepts removed tenant with server launch denial", () => {
  assert.deepEqual(
    summarizeAccessRevocationBlock({
      postRevocationInstance: null,
      launchAfterRevocationError: "ctox.dev launch token failed: 403",
    }),
    {
      postRevocationStatus: "removed",
      launchAfterRevocationBlocked: true,
      launchAfterRevocationError: "ctox.dev launch token failed: 403",
    },
  );
});

test("access revocation contract rejects still-launchable tenants", () => {
  assert.throws(
    () => summarizeAccessRevocationBlock({
      postRevocationInstance: { status: "available" },
      launchAfterRevocationError: "ctox.dev launch token failed: 403",
    }),
    /did not become needs_auth or disappear/,
  );
});

test("access revocation contract rejects successful or unrelated launches", () => {
  assert.throws(
    () => summarizeAccessRevocationBlock({
      postRevocationInstance: { status: "needs_auth" },
      launchAfterRevocationError: "",
    }),
    /unexpectedly succeeded/,
  );
  assert.throws(
    () => summarizeAccessRevocationBlock({
      postRevocationInstance: null,
      launchAfterRevocationError: "network timeout",
    }),
    /unexpected reason/,
  );
});
