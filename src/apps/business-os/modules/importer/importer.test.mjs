import test from "node:test";
import assert from "node:assert/strict";
import { parseGitHubUrl, shouldSkipPath, isTextFile, validModuleId } from "./index.js";

test("parseGitHubUrl handles repo, tree refs and subdirs", () => {
  assert.deepEqual(parseGitHubUrl("https://github.com/acme/po-tracker"), {
    owner: "acme", repo: "po-tracker", ref: null, subdir: "",
  });
  assert.deepEqual(parseGitHubUrl("https://github.com/acme/po-tracker.git"), {
    owner: "acme", repo: "po-tracker", ref: null, subdir: "",
  });
  assert.deepEqual(parseGitHubUrl("https://github.com/acme/mono/tree/main/apps/tracker"), {
    owner: "acme", repo: "mono", ref: "main", subdir: "apps/tracker",
  });
  assert.equal(parseGitHubUrl("https://gitlab.com/acme/x"), null);
  assert.equal(parseGitHubUrl("not a url"), null);
  assert.equal(parseGitHubUrl("https://github.com/onlyowner"), null);
});

test("shouldSkipPath drops build artifacts and vcs noise", () => {
  for (const path of [
    "node_modules/react/index.js",
    "src/node_modules/x.js",
    ".git/HEAD",
    "dist/bundle.js",
    "build/main.js",
    ".next/app.js",
    "yarn.lock",
    "package-lock.json",
    "src/main.js.map",
  ]) {
    assert.equal(shouldSkipPath(path), true, path);
  }
  for (const path of ["src/main.tsx", "src/App.tsx", "index.html", "src/lib/format.ts"]) {
    assert.equal(shouldSkipPath(path), false, path);
  }
});

test("isTextFile allows source and asset text, rejects binaries", () => {
  assert.equal(isTextFile("src/main.tsx"), true);
  assert.equal(isTextFile("styles/app.css"), true);
  assert.equal(isTextFile("logo.svg"), true);
  assert.equal(isTextFile("logo.png"), false);
  assert.equal(isTextFile("font.woff2"), false);
  assert.equal(isTextFile("Makefile"), false);
});

test("validModuleId enforces the launcher slug contract", () => {
  assert.equal(validModuleId("po-tracker"), true);
  assert.equal(validModuleId("a1"), true);
  assert.equal(validModuleId("-bad"), false);
  assert.equal(validModuleId("Bad"), false);
  assert.equal(validModuleId("x"), false);
  assert.equal(validModuleId("with space"), false);
});
