const assert = require("node:assert/strict");
const { createHash } = require("node:crypto");
const { mkdtemp, readFile, rm } = require("node:fs/promises");
const http = require("node:http");
const { createRequire } = require("node:module");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");

test("builder downloader verifies bytes, rejects a wrong checksum and reuses its cache", async () => {
  const builderRequire = createRequire(require.resolve("app-builder-lib/package.json"));
  const { downloadArtifact, CacheMode } = builderRequire("@electron/get");
  // Load the actual consumer too: an incompatible export fails before a download.
  assert.equal(typeof require("app-builder-lib/out/binDownload").downloadArtifact, "function");
  assert.equal(typeof downloadArtifact, "function");
  assert.equal(typeof CacheMode.ReadWrite, "string");
  assert.equal(typeof CacheMode.Bypass, "string");
  const bytes = Buffer.from("synthetic builder download fixture\n");
  const checksum = createHash("sha256").update(bytes).digest("hex");
  const filename = "fixture.tar.gz";
  const root = await mkdtemp(path.join(os.tmpdir(), "ctox-builder-download-"));
  let requests = 0;
  const server = http.createServer((request, response) => {
    if (request.url !== "/fixture/" + filename) {
      response.writeHead(404).end();
      return;
    }
    requests++;
    response.writeHead(200, { "Content-Length": bytes.length });
    response.end(bytes);
  });
  try {
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(0, "127.0.0.1", resolve);
    });
    const options = {
      version: "9.9.9",
      artifactName: filename,
      isGeneric: true,
      cacheRoot: root,
      cacheMode: CacheMode.ReadWrite,
      checksums: { [filename]: checksum },
      downloadOptions: { getProgressCallback: async () => {} },
      mirrorOptions: {
        mirror: "http://127.0.0.1:" + server.address().port + "/",
        customDir: "fixture",
        customFilename: filename,
      },
    };
    const first = await downloadArtifact(options);
    assert.deepEqual(await readFile(first), bytes);
    assert.equal(requests, 1);
    const cached = await downloadArtifact(options);
    assert.deepEqual(await readFile(cached), bytes);
    assert.equal(requests, 1, "a warm cache must not fetch the artifact twice");
    await assert.rejects(
      downloadArtifact({ ...options, cacheMode: CacheMode.Bypass,
        checksums: { [filename]: "0".repeat(64) } }),
      /checksum|does not match|digest/i,
    );
    assert.equal(requests, 2, "checksum rejection must exercise a fresh response");
  } finally {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
    await rm(root, { recursive: true, force: true });
  }
});
