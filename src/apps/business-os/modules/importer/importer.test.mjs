import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
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

test("presentation collapses redundant card section captions", async () => {
  const html = await readFile(new URL("./index.html", import.meta.url), "utf8");
  const css = await readFile(new URL("./index.css", import.meta.url), "utf8");
  const js = await readFile(new URL("./index.js", import.meta.url), "utf8");
  const de = JSON.parse(await readFile(new URL("./locales/de.json", import.meta.url), "utf8"));
  const en = JSON.parse(await readFile(new URL("./locales/en.json", import.meta.url), "utf8"));

  // 1. No card carries a section-caption header — the pane title
  //    ("App Importer") already names the flow; the buttons and the report
  //    fields label their own cards implicitly.
  assert.doesNotMatch(html, /data-imp-source-title/);
  assert.doesNotMatch(html, /data-imp-report-title/);
  assert.doesNotMatch(html, /data-imp-done-title/);
  assert.doesNotMatch(html, /<header[^>]*>\s*Source\s*<\/header>/i);
  assert.doesNotMatch(html, /<header[^>]*>\s*Report\s*<\/header>/i);
  assert.doesNotMatch(html, /<header[^>]*>\s*Installed\s*<\/header>/i);

  // 2. JS no longer queries the deleted header hooks (would throw on mount).
  assert.doesNotMatch(js, /querySelector\(['"]\[data-imp-source-title\]['"]\)/);
  assert.doesNotMatch(js, /querySelector\(['"]\[data-imp-report-title\]['"]\)/);
  assert.doesNotMatch(js, /querySelector\(['"]\[data-imp-done-title\]['"]\)/);
  assert.doesNotMatch(js, /refs\.sourceTitle\.textContent/);
  assert.doesNotMatch(js, /refs\.reportTitle\.textContent/);
  assert.doesNotMatch(js, /refs\.doneTitle\.textContent/);

  // 3. Locale files dropped the three dead keys; everything else is still
  //    there (assertions below pin the surviving keys).
  assert.equal(de.sourceTitle, undefined);
  assert.equal(de.reportTitle, undefined);
  assert.equal(de.installedTitle, undefined);
  assert.equal(en.sourceTitle, undefined);
  assert.equal(en.reportTitle, undefined);
  assert.equal(en.installedTitle, undefined);
  assert.equal(typeof en.title, "string");
  assert.equal(typeof en.pickFolder, "string");
  assert.equal(typeof en.idLabel, "string");
  assert.equal(typeof en.install, "string");

  // 4. The card body now supplies the top padding the deleted header used
  //    to provide (kit default is 0 12px 12px, so the first row would sit
  //    flush against the surface step otherwise).
  assert.match(
    css,
    /\.importer-module \.imp-card-body\s*\{[^}]*padding-top:\s*12px/,
  );
});
