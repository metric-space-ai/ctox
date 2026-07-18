"use strict";

const fs = require("node:fs");
const path = require("node:path");
const yaml = require("js-yaml");

const MAC_MANIFEST_NAMES = new Set(["alpha-mac.yml", "beta-mac.yml", "latest-mac.yml"]);

function sameBytes(paths) {
  const first = fs.readFileSync(paths[0]);
  return paths.slice(1).every((candidate) => first.equals(fs.readFileSync(candidate)));
}

function mergeMacManifests(manifestPaths) {
  if (manifestPaths.length < 2) {
    throw new Error(`expected both macOS architecture manifests, found ${manifestPaths.length}`);
  }
  const documents = manifestPaths.map((manifestPath) => ({
    manifestPath,
    value: yaml.load(fs.readFileSync(manifestPath, "utf8")),
  }));
  const versions = new Set(documents.map(({ value }) => value?.version));
  if (versions.size !== 1 || versions.has(undefined)) {
    throw new Error(`macOS update manifest versions disagree: ${[...versions].join(", ")}`);
  }

  const filesByUrl = new Map();
  for (const { manifestPath, value } of documents) {
    if (!Array.isArray(value.files) || value.files.length === 0) {
      throw new Error(`macOS update manifest has no files: ${manifestPath}`);
    }
    for (const file of value.files) {
      if (!file?.url || !file.sha512) {
        throw new Error(`macOS update manifest file is incomplete: ${manifestPath}`);
      }
      const existing = filesByUrl.get(file.url);
      if (existing && JSON.stringify(existing) !== JSON.stringify(file)) {
        throw new Error(`macOS update manifest file metadata disagrees for ${file.url}`);
      }
      filesByUrl.set(file.url, file);
    }
  }

  const files = [...filesByUrl.values()].sort((left, right) => left.url.localeCompare(right.url));
  const zipArchitectures = new Set(
    files
      .filter((file) => file.url.endsWith(".zip"))
      .map((file) => (file.url.includes("arm64") ? "arm64" : file.url.includes("x64") ? "x64" : "unknown")),
  );
  if (!zipArchitectures.has("arm64") || !zipArchitectures.has("x64")) {
    throw new Error(`merged macOS update manifest lacks both ZIP architectures: ${[...zipArchitectures].join(", ")}`);
  }

  const fallbackDocument =
    documents.find(({ value }) => String(value.path || "").includes("x64")) || documents[0];
  const releaseDate = documents
    .map(({ value }) => value.releaseDate)
    .filter(Boolean)
    .sort()
    .at(-1);
  return {
    ...fallbackDocument.value,
    files,
    path: fallbackDocument.value.path,
    sha512: fallbackDocument.value.sha512,
    ...(releaseDate ? { releaseDate } : {}),
  };
}

function prepareReleaseArtifacts(inputRoot, outputRoot) {
  fs.mkdirSync(outputRoot, { recursive: true });
  const artifactDirectories = fs
    .readdirSync(inputRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .sort((left, right) => left.name.localeCompare(right.name));
  if (artifactDirectories.length === 0) {
    throw new Error(`no downloaded artifacts found under ${inputRoot}`);
  }

  const filesByName = new Map();
  for (const directory of artifactDirectories) {
    const directoryPath = path.join(inputRoot, directory.name);
    for (const entry of fs.readdirSync(directoryPath, { withFileTypes: true })) {
      if (!entry.isFile()) continue;
      const files = filesByName.get(entry.name) || [];
      files.push({ artifact: directory.name, source: path.join(directoryPath, entry.name) });
      filesByName.set(entry.name, files);
    }
  }

  for (const [name, sources] of [...filesByName.entries()].sort(([left], [right]) => left.localeCompare(right))) {
    const destination = path.join(outputRoot, name);
    if (MAC_MANIFEST_NAMES.has(name)) {
      const merged = mergeMacManifests(sources.map(({ source }) => source));
      fs.writeFileSync(destination, yaml.dump(merged, { lineWidth: -1, noRefs: true }));
      continue;
    }
    if (sources.length === 1 || sameBytes(sources.map(({ source }) => source))) {
      fs.copyFileSync(sources[0].source, destination);
      continue;
    }
    if (name === "builder-debug.yml") {
      for (const { artifact, source } of sources) {
        fs.copyFileSync(source, path.join(outputRoot, `${artifact}-${name}`));
      }
      continue;
    }
    throw new Error(
      `release artifacts contain conflicting files named ${name}: ${sources.map(({ artifact }) => artifact).join(", ")}`,
    );
  }
}

if (require.main === module) {
  const [, , inputRoot, outputRoot] = process.argv;
  if (!inputRoot || !outputRoot) {
    throw new Error("usage: prepare-release-artifacts.cjs <download-root> <output-root>");
  }
  prepareReleaseArtifacts(path.resolve(inputRoot), path.resolve(outputRoot));
}

module.exports = { mergeMacManifests, prepareReleaseArtifacts };
