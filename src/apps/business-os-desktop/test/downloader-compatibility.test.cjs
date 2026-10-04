const assert = require("node:assert/strict");
const { createHash } = require("node:crypto");
const { mkdtemp, readFile, rm } = require("node:fs/promises");
const http = require("node:http");
const { createRequire } = require("node:module");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");

test("builder download verifies bytes, rejects a wrong checksum and reuses its cache", async () => {
  const builderRequire = createRequire(require.resolve("app-builder-lib/package.json"));
  const { ElectronDownloadCacheMode } = builderRequire("@electron/get");
  assert.equal(typeof ElectronDownloadCacheMode.ReadWrite, "number");
  const { download } = require("app-builder-lib/out/binDownload");
  assert.equal(typeof download, "function");
  const bytes = Buffer.from("synthetic builder download fixture\n");
  const checksum = createHash("sha256").update(bytes).digest("hex");
  const root = await mkdtemp(path.join(os.tmpdir(), "ctox-builder-download-"));
  const previousCache = process.env.ELECTRON_BUILDER_CACHE;
  process.env.ELECTRON_BUILDER_CACHE = root;
  let requests = 0;
  const server = http.createServer((request, response) => {
    if (!["/fixture.bin", "/bad.bin"].includes(request.url)) {
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
    const base = "http://127.0.0.1:" + server.address().port;
    const first = path.join(root, "first.bin");
    await download(base + "/fixture.bin", first, checksum);
    assert.deepEqual(await readFile(first), bytes);
    assert.equal(requests, 1);
    const cached = path.join(root, "cached.bin");
    await download(base + "/fixture.bin", cached, checksum);
    assert.deepEqual(await readFile(cached), bytes);
    assert.equal(requests, 1, "a warm cache must not fetch the artifact twice");
    const rejected = path.join(root, "rejected.bin");
    await assert.rejects(
      download(base + "/bad.bin", rejected, "0".repeat(64)),
      /checksum|does not match|digest/i,
    );
    assert.equal(requests, 2, "checksum rejection must exercise a fresh response");
    await assert.rejects(readFile(rejected), { code: "ENOENT" });
  } finally {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
    if (previousCache === undefined) delete process.env.ELECTRON_BUILDER_CACHE;
    else process.env.ELECTRON_BUILDER_CACHE = previousCache;
    await rm(root, { recursive: true, force: true });
  }
});
