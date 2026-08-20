// SPDX-License-Identifier: MIT OR AGPL-3.0-only
import { constants as fsConstants } from 'node:fs';
import {
  link,
  lstat,
  mkdir,
  open,
  readdir,
  rm,
  stat,
  unlink,
  writeFile,
} from 'node:fs/promises';
import { createHash, randomBytes } from 'node:crypto';
import { posix as pathPosix, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { deflateRawSync } from 'node:zlib';

export const SHELL_SCHEMA = 'ctox.business-os-shell.v1';
export const ROOT_RUNTIME_FILES = Object.freeze([
  'index.html',
  'app.js',
  'app.css',
  'system-apps.json',
]);
export const RUNTIME_TREES = Object.freeze([
  'app-starter',
  'assets',
  'desktop-apps',
  'installed-modules',
  'modules',
  'office-engine',
  'public',
  'rxdb',
  'shared',
  'template-store',
  'vendor',
]);
export const MAX_RUNTIME_FILE_COUNT = 20_000;
export const MAX_RUNTIME_FILE_BYTES = 512 * 1024 * 1024;
export const MAX_RUNTIME_TOTAL_BYTES = 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRY_COUNT = MAX_RUNTIME_FILE_COUNT * 3;

const STRICT_SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/;
const SOURCE_COMMIT = /^[0-9a-f]{40}$/;
const CONTROL_CHARACTER = /[\u0000-\u001f\u007f-\u009f]/;
const TEST_FILE_TOKEN = /(?:^|[._-])(?:test|tests|spec|specs|smoke|fixture|fixtures)(?:[._-]|$)/i;
const RUNTIME_NOTICE_FILE = /(?:^|[._-])(?:license|licenses|licence|licences|notice|notices|copying|provenance)(?:[._-]|$)/i;
const PACKAGE_LOCKS = new Set([
  'package-lock.json',
  'npm-shrinkwrap.json',
  'yarn.lock',
  'pnpm-lock.yaml',
  'bun.lock',
  'bun.lockb',
]);
const EXCLUDED_DIRECTORIES = new Set([
  'node_modules',
  'qa',
  'scripts',
  'tests',
  '__tests__',
  'test',
  'spec',
  'specs',
  'fixture',
  'fixtures',
  'build',
  'release',
  'releases',
  'cache',
  '.cache',
  'coverage',
  '.nyc_output',
  'local-state',
  'local_state',
  '.local-state',
  'tmp',
  'temp',
]);
const GENERATED_DIRECTORY = /^(?:build|release|cache|local[-_]?state)(?:[._-].*)?$/i;
const GENERATED_FILE_TOKEN = /(?:^|[._-])(?:build|release|cache|local[-_]?state)(?:[._-]|$)/i;

export function validateSemVer(version) {
  if (typeof version !== 'string' || !STRICT_SEMVER.test(version)) {
    throw new Error(`Invalid strict SemVer: ${JSON.stringify(version)}`);
  }
  return version;
}

export function validateSourceCommit(sourceCommit) {
  if (typeof sourceCommit !== 'string' || !SOURCE_COMMIT.test(sourceCommit)) {
    throw new Error('Source commit must be exactly 40 lowercase hexadecimal characters');
  }
  return sourceCommit;
}

export function validateRelativePath(relativePath) {
  if (typeof relativePath !== 'string' || relativePath.length === 0) {
    throw new Error('Archive paths must be non-empty strings');
  }
  if (relativePath.includes('\\')) {
    throw new Error(`Archive path contains a backslash: ${JSON.stringify(relativePath)}`);
  }
  if (CONTROL_CHARACTER.test(relativePath)) {
    throw new Error(`Archive path contains a control character: ${JSON.stringify(relativePath)}`);
  }
  if (pathPosix.isAbsolute(relativePath) || /^[A-Za-z]:/.test(relativePath)) {
    throw new Error(`Archive path must be relative: ${JSON.stringify(relativePath)}`);
  }
  if (relativePath !== relativePath.normalize('NFC')) {
    throw new Error(`Archive path is not Unicode-normalized: ${JSON.stringify(relativePath)}`);
  }
  const segments = relativePath.split('/');
  if (segments.some((segment) => segment === '' || segment === '.' || segment === '..')) {
    throw new Error(`Archive path is not canonical: ${JSON.stringify(relativePath)}`);
  }
  if (pathPosix.normalize(relativePath) !== relativePath) {
    throw new Error(`Archive path is not canonical: ${JSON.stringify(relativePath)}`);
  }
  return relativePath;
}

export function isExcludedRuntimePath(relativePath, { directory = false } = {}) {
  validateRelativePath(relativePath);
  const segments = relativePath.split('/');
  const basename = segments.at(-1);
  const lowerSegments = segments.map((segment) => segment.toLowerCase());

  if (lowerSegments.some((segment) => segment.startsWith('.git'))) return true;
  if (lowerSegments.slice(0, -1).some((segment) => EXCLUDED_DIRECTORIES.has(segment) || GENERATED_DIRECTORY.test(segment))) {
    return true;
  }
  if (directory && (EXCLUDED_DIRECTORIES.has(basename.toLowerCase()) || GENERATED_DIRECTORY.test(basename))) {
    return true;
  }
  if (basename === '.DS_Store' || PACKAGE_LOCKS.has(basename.toLowerCase())) return true;
  if (/^(?:agents|claude)(?:\..*)?$/i.test(basename)) return true;
  if (!directory && RUNTIME_NOTICE_FILE.test(basename)) return false;
  if (!directory && (TEST_FILE_TOKEN.test(basename) || GENERATED_FILE_TOKEN.test(basename))) return true;
  return false;
}

export function sha256(data) {
  return createHash('sha256').update(data).digest('hex');
}

function comparePaths(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}

function octalField(value, width, label) {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error(`Invalid ${label} value for USTAR: ${value}`);
  }
  const octal = value.toString(8);
  if (octal.length > width - 1) {
    throw new Error(`${label} is too large for USTAR: ${value}`);
  }
  return `${octal.padStart(width - 1, '0')}\0`;
}

function writeUtf8Field(header, offset, length, value, label) {
  const bytes = Buffer.from(value, 'utf8');
  if (bytes.length > length) {
    throw new Error(`${label} is too long for USTAR: ${value}`);
  }
  bytes.copy(header, offset);
}

function splitUstarPath(archivePath) {
  const bytes = Buffer.byteLength(archivePath);
  if (bytes <= 100) return { name: archivePath, prefix: '' };

  const searchFrom = archivePath.endsWith('/') ? archivePath.length - 2 : archivePath.length - 1;
  for (let slash = archivePath.lastIndexOf('/', searchFrom); slash > 0; slash = archivePath.lastIndexOf('/', slash - 1)) {
    const prefix = archivePath.slice(0, slash);
    const name = archivePath.slice(slash + 1);
    if (name && Buffer.byteLength(prefix) <= 155 && Buffer.byteLength(name) <= 100) {
      return { name, prefix };
    }
  }
  throw new Error(`Archive path cannot be represented in USTAR: ${archivePath}`);
}

function createTarHeader(archivePath, type, size) {
  const { name, prefix } = splitUstarPath(archivePath);
  const header = Buffer.alloc(512);
  writeUtf8Field(header, 0, 100, name, 'name');
  writeUtf8Field(header, 100, 8, octalField(type === 'directory' ? 0o755 : 0o644, 8, 'mode'), 'mode');
  writeUtf8Field(header, 108, 8, octalField(0, 8, 'uid'), 'uid');
  writeUtf8Field(header, 116, 8, octalField(0, 8, 'gid'), 'gid');
  writeUtf8Field(header, 124, 12, octalField(size, 12, 'size'), 'size');
  writeUtf8Field(header, 136, 12, octalField(0, 12, 'mtime'), 'mtime');
  header.fill(0x20, 148, 156);
  header[156] = type === 'directory' ? 0x35 : 0x30;
  writeUtf8Field(header, 257, 6, 'ustar\0', 'magic');
  writeUtf8Field(header, 263, 2, '00', 'version');
  writeUtf8Field(header, 345, 155, prefix, 'prefix');

  const checksum = header.reduce((sum, byte) => sum + byte, 0);
  const checksumText = checksum.toString(8).padStart(6, '0');
  if (checksumText.length !== 6) throw new Error(`USTAR checksum overflow: ${checksum}`);
  writeUtf8Field(header, 148, 8, `${checksumText}\0 `, 'checksum');
  return header;
}

export function createTarArchive(entries) {
  if (!Array.isArray(entries)) throw new Error('USTAR entries must be an array');
  if (entries.length > MAX_ARCHIVE_ENTRY_COUNT) {
    throw new Error(`USTAR entry count exceeds the ${MAX_ARCHIVE_ENTRY_COUNT} entry limit`);
  }
  const prepared = entries.map((entry) => {
    if (!entry || (entry.type !== 'file' && entry.type !== 'directory')) {
      throw new Error('USTAR entries must be regular files or directories');
    }
    const path = validateRelativePath(entry.path);
    const data = entry.type === 'file'
      ? (Buffer.isBuffer(entry.data) ? entry.data : Buffer.from(entry.data ?? ''))
      : Buffer.alloc(0);
    if (data.length > MAX_RUNTIME_FILE_BYTES) {
      throw new Error(`USTAR file exceeds the ${MAX_RUNTIME_FILE_BYTES} byte limit: ${path}`);
    }
    const archivePath = entry.type === 'directory' ? `${path}/` : path;
    splitUstarPath(archivePath);
    return { ...entry, path, archivePath, data };
  }).sort((left, right) => comparePaths(left.archivePath, right.archivePath));

  const seen = new Set();
  const chunks = [];
  let totalFileBytes = 0;
  for (const entry of prepared) {
    if (seen.has(entry.path)) throw new Error(`Duplicate USTAR entry: ${entry.path}`);
    seen.add(entry.path);
    chunks.push(createTarHeader(entry.archivePath, entry.type, entry.data.length));
    if (entry.type === 'file') {
      totalFileBytes += entry.data.length;
      if (totalFileBytes > MAX_RUNTIME_TOTAL_BYTES) {
        throw new Error(`USTAR payload exceeds the ${MAX_RUNTIME_TOTAL_BYTES} byte limit`);
      }
      chunks.push(entry.data);
      const padding = (512 - (entry.data.length % 512)) % 512;
      if (padding) chunks.push(Buffer.alloc(padding));
    }
  }
  chunks.push(Buffer.alloc(1024));
  return Buffer.concat(chunks);
}

const CRC32_TABLE = Uint32Array.from({ length: 256 }, (_, index) => {
  let value = index;
  for (let bit = 0; bit < 8; bit += 1) {
    value = (value >>> 1) ^ ((value & 1) ? 0xedb88320 : 0);
  }
  return value >>> 0;
});

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) crc = (crc >>> 8) ^ CRC32_TABLE[(crc ^ byte) & 0xff];
  return (crc ^ 0xffffffff) >>> 0;
}

export function createDeterministicGzip(buffer) {
  const input = Buffer.isBuffer(buffer) ? buffer : Buffer.from(buffer);
  const header = Buffer.from([0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff]);
  const compressed = deflateRawSync(input, { level: 9 });
  const trailer = Buffer.alloc(8);
  trailer.writeUInt32LE(crc32(input), 0);
  trailer.writeUInt32LE(input.length >>> 0, 4);
  return Buffer.concat([header, compressed, trailer]);
}

async function assertDirectoryNoSymlink(absolutePath, label) {
  const details = await lstat(absolutePath);
  if (details.isSymbolicLink()) throw new Error(`Symlinks are forbidden: ${label}`);
  if (!details.isDirectory()) throw new Error(`Required directory is missing: ${label}`);
}

async function readRegularFileNoFollow(absolutePath, relativePath, budget) {
  const before = await lstat(absolutePath);
  if (before.isSymbolicLink()) throw new Error(`Symlinks are forbidden: ${relativePath}`);
  if (!before.isFile()) throw new Error(`Only regular files and directories are allowed: ${relativePath}`);
  if (before.size > MAX_RUNTIME_FILE_BYTES) {
    throw new Error(`Runtime file exceeds the ${MAX_RUNTIME_FILE_BYTES} byte limit: ${relativePath}`);
  }

  const noFollow = fsConstants.O_NOFOLLOW ?? 0;
  const handle = await open(absolutePath, fsConstants.O_RDONLY | noFollow);
  try {
    const opened = await handle.stat();
    if (!opened.isFile() || opened.dev !== before.dev || opened.ino !== before.ino) {
      throw new Error(`Source file changed while reading: ${relativePath}`);
    }
    const data = await handle.readFile();
    const after = await lstat(absolutePath);
    if (after.isSymbolicLink() || after.dev !== opened.dev || after.ino !== opened.ino) {
      throw new Error(`Source file changed while reading: ${relativePath}`);
    }
    if (budget !== undefined) {
      const nextFileCount = budget.fileCount + 1;
      const nextTotalBytes = budget.totalBytes + data.length;
      if (nextFileCount > MAX_RUNTIME_FILE_COUNT) {
        throw new Error(`Runtime file count exceeds the ${MAX_RUNTIME_FILE_COUNT} file limit`);
      }
      if (nextTotalBytes > MAX_RUNTIME_TOTAL_BYTES) {
        throw new Error(`Runtime payload exceeds the ${MAX_RUNTIME_TOTAL_BYTES} byte limit`);
      }
      budget.fileCount = nextFileCount;
      budget.totalBytes = nextTotalBytes;
    }
    return data;
  } finally {
    await handle.close();
  }
}

async function collectTree(sourceRoot, relativeDirectory, directories, files, budget) {
  validateRelativePath(relativeDirectory);
  const absoluteDirectory = resolve(sourceRoot, ...relativeDirectory.split('/'));
  await assertDirectoryNoSymlink(absoluteDirectory, relativeDirectory);
  directories.push(relativeDirectory);

  const children = await readdir(absoluteDirectory);
  children.sort(comparePaths);
  for (const name of children) {
    const relativePath = `${relativeDirectory}/${name}`;
    validateRelativePath(relativePath);
    const absolutePath = resolve(absoluteDirectory, name);
    const details = await lstat(absolutePath);
    if (details.isSymbolicLink()) throw new Error(`Symlinks are forbidden: ${relativePath}`);
    if (!details.isFile() && !details.isDirectory()) {
      throw new Error(`Only regular files and directories are allowed: ${relativePath}`);
    }
    if (isExcludedRuntimePath(relativePath, { directory: details.isDirectory() })) continue;
    if (details.isDirectory()) {
      await collectTree(sourceRoot, relativePath, directories, files, budget);
    } else {
      files.push({ path: relativePath, data: await readRegularFileNoFollow(absolutePath, relativePath, budget) });
    }
  }
}

export async function collectRuntimePayload(sourceRoot) {
  const absoluteSourceRoot = resolve(sourceRoot);
  await assertDirectoryNoSymlink(absoluteSourceRoot, absoluteSourceRoot);
  const directories = [];
  const files = [];
  const budget = { fileCount: 0, totalBytes: 0 };

  for (const relativePath of ROOT_RUNTIME_FILES) {
    const absolutePath = resolve(absoluteSourceRoot, relativePath);
    try {
      files.push({ path: relativePath, data: await readRegularFileNoFollow(absolutePath, relativePath, budget) });
    } catch (error) {
      if (error?.code === 'ENOENT') throw new Error(`Required runtime file is missing: ${relativePath}`);
      throw error;
    }
  }
  for (const relativeDirectory of RUNTIME_TREES) {
    try {
      await collectTree(absoluteSourceRoot, relativeDirectory, directories, files, budget);
    } catch (error) {
      if (error?.code === 'ENOENT') throw new Error(`Required runtime directory is missing: ${relativeDirectory}`);
      throw error;
    }
  }

  files.sort((left, right) => comparePaths(left.path, right.path));
  directories.sort(comparePaths);
  return { directories, files };
}

export function createEmbeddedManifest({ version, sourceCommit, archiveRoot, files }) {
  validateSemVer(version);
  validateSourceCommit(sourceCommit);
  validateRelativePath(archiveRoot);
  const records = files.map(({ path, data }) => ({
    path: validateRelativePath(path),
    byteSize: data.length,
    sha256: sha256(data),
  })).sort((left, right) => comparePaths(left.path, right.path));
  return {
    schema: SHELL_SCHEMA,
    version,
    sourceCommit,
    entry: 'index.html',
    archiveRoot,
    files: records,
  };
}

async function pathExists(targetPath) {
  try {
    await stat(targetPath);
    return true;
  } catch (error) {
    if (error?.code === 'ENOENT') return false;
    throw error;
  }
}

export async function commitStagedOutputs(stagedOutputs, operations = {}) {
  const linkFile = operations.link ?? link;
  const unlinkFile = operations.unlink ?? unlink;
  const removePath = operations.rm ?? rm;
  const published = [];
  try {
    for (const { stagedPath, finalPath } of stagedOutputs) {
      // link(2) fails with EEXIST instead of replacing another concurrent
      // publisher's final output. Staged and final files live in one directory,
      // so the hard-link publication is atomic on every supported filesystem.
      await linkFile(stagedPath, finalPath);
      published.push(finalPath);
      await unlinkFile(stagedPath);
    }
  } catch (error) {
    await Promise.allSettled(published.map((finalPath) => removePath(finalPath, { force: true })));
    await Promise.allSettled(stagedOutputs.map(({ stagedPath }) => removePath(stagedPath, { force: true })));
    throw error;
  }
}

export async function buildShellArtifact({ sourceRoot, outputDir, version, sourceCommit }) {
  validateSemVer(version);
  validateSourceCommit(sourceCommit);
  if (typeof sourceRoot !== 'string' || sourceRoot.length === 0) throw new Error('sourceRoot is required');
  if (typeof outputDir !== 'string' || outputDir.length === 0) throw new Error('outputDir is required');

  const archiveRoot = `ctox-business-os-shell-${version}`;
  validateRelativePath(archiveRoot);
  const archiveFilename = `${archiveRoot}.tar.gz`;
  const manifestFilename = `${archiveRoot}.manifest.json`;
  const checksumFilename = `${archiveFilename}.sha256`;
  const payload = await collectRuntimePayload(sourceRoot);
  const embeddedManifest = createEmbeddedManifest({ version, sourceCommit, archiveRoot, files: payload.files });
  const embeddedManifestBytes = Buffer.from(`${JSON.stringify(embeddedManifest, null, 2)}\n`);

  const tarEntries = [
    { path: archiveRoot, type: 'directory' },
    ...payload.directories.map((relativePath) => ({ path: `${archiveRoot}/${relativePath}`, type: 'directory' })),
    ...payload.files.map(({ path, data }) => ({ path: `${archiveRoot}/${path}`, type: 'file', data })),
    { path: `${archiveRoot}/ctox-shell-manifest.json`, type: 'file', data: embeddedManifestBytes },
  ];
  const archiveBytes = createDeterministicGzip(createTarArchive(tarEntries));
  const archiveSha256 = sha256(archiveBytes);
  const detachedManifest = {
    ...embeddedManifest,
    archiveFilename,
    archiveByteLength: archiveBytes.length,
    archiveSha256,
    embeddedManifestSha256: sha256(embeddedManifestBytes),
  };
  const detachedManifestBytes = Buffer.from(`${JSON.stringify(detachedManifest, null, 2)}\n`);
  const checksumBytes = Buffer.from(`${archiveSha256}  ${archiveFilename}\n`);

  const absoluteOutputDir = resolve(outputDir);
  await mkdir(absoluteOutputDir, { recursive: true });
  const nonce = `${process.pid}-${randomBytes(8).toString('hex')}`;
  const outputs = [
    { filename: archiveFilename, data: archiveBytes },
    { filename: manifestFilename, data: detachedManifestBytes },
    { filename: checksumFilename, data: checksumBytes },
  ].map((output) => ({
    ...output,
    finalPath: resolve(absoluteOutputDir, output.filename),
    stagedPath: resolve(absoluteOutputDir, `.${output.filename}.${nonce}.tmp`),
  }));

  for (const { finalPath } of outputs) {
    if (await pathExists(finalPath)) throw new Error(`Refusing to overwrite existing artifact output: ${finalPath}`);
  }

  try {
    for (const { stagedPath, data } of outputs) await writeFile(stagedPath, data, { flag: 'wx', mode: 0o600 });
    await commitStagedOutputs(outputs);
  } catch (error) {
    await Promise.allSettled(outputs.map(({ stagedPath }) => rm(stagedPath, { force: true })));
    throw error;
  }

  return {
    archivePath: outputs[0].finalPath,
    manifestPath: outputs[1].finalPath,
    checksumPath: outputs[2].finalPath,
    archiveFilename,
    manifestFilename,
    checksumFilename,
    archiveByteLength: archiveBytes.length,
    archiveSha256,
  };
}

function parseCliArguments(argv) {
  const allowed = new Set(['--source-root', '--output-dir', '--version', '--source-commit']);
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    if (!allowed.has(flag)) throw new Error(`Unknown CLI argument: ${flag ?? '<missing>'}`);
    if (value === undefined || allowed.has(value)) throw new Error(`Missing value for ${flag}`);
    if (values.has(flag)) throw new Error(`Duplicate CLI argument: ${flag}`);
    values.set(flag, value);
  }
  for (const flag of allowed) {
    if (!values.has(flag)) throw new Error(`Required CLI argument is missing: ${flag}`);
  }
  return {
    sourceRoot: values.get('--source-root'),
    outputDir: values.get('--output-dir'),
    version: values.get('--version'),
    sourceCommit: values.get('--source-commit'),
  };
}

const isMain = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  try {
    const result = await buildShellArtifact(parseCliArguments(process.argv.slice(2)));
    process.stdout.write(`${JSON.stringify(result)}\n`);
  } catch (error) {
    process.stderr.write(`build-shell-artifact: ${error?.message ?? error}\n`);
    process.exitCode = 1;
  }
}
