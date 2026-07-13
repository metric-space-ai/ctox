#!/usr/bin/env node

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCHEMA = 'ctox-office-release-candidate-evidence-v1';
const MATRIX_SCHEMA = 'ctox.business_os.smoke_matrix_summary.v1';
const MODES = {
  document: 'office-document-midflight-restart-browser-to-rust',
  spreadsheet: 'office-spreadsheet-midflight-restart-browser-to-rust',
};

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const compareSemver = (left, right) => {
  const a = left.split('.').map(Number);
  const b = right.split('.').map(Number);
  for (let index = 0; index < 3; index += 1) {
    if (a[index] !== b[index]) return a[index] - b[index];
  }
  return 0;
};
const isoTimestamp = (value, field) => {
  assert.ok(value, `${field} is required`);
  const parsed = new Date(value);
  assert.equal(Number.isNaN(parsed.valueOf()), false, `${field} must be an ISO timestamp`);
  return parsed.toISOString();
};

async function readEvidenceFile(path) {
  const bytes = await readFile(path);
  return { bytes, json: JSON.parse(bytes.toString('utf8')), sha256: sha256(bytes) };
}

function validateRestartMatrix(matrix, expectedMode, gitRevision) {
  assert.equal(matrix.schema, MATRIX_SCHEMA, `${expectedMode}: matrix schema`);
  assert.equal(matrix.schemaVersion, 1, `${expectedMode}: matrix schema version`);
  assert.equal(matrix.ok, true, `${expectedMode}: matrix must pass`);
  assert.equal(matrix.gitRevision, gitRevision, `${expectedMode}: git revision`);
  assert.equal(matrix.source?.commit, gitRevision, `${expectedMode}: source commit`);
  assert.equal(matrix.source?.dirty, false, `${expectedMode}: source must be clean`);
  assert.equal(matrix.configuration?.attempts, 1, `${expectedMode}: retries are forbidden`);
  assert.deepEqual(matrix.requestedModes, [expectedMode], `${expectedMode}: requested mode`);
  assert.equal(matrix.modes?.length, 1, `${expectedMode}: exactly one mode result`);
  const mode = matrix.modes[0];
  assert.equal(mode.mode, expectedMode, `${expectedMode}: result mode`);
  assert.equal(mode.ok, true, `${expectedMode}: mode must pass`);
  assert.equal(mode.attempts?.length, 1, `${expectedMode}: exactly one attempt`);
  const attempt = mode.attempts[0];
  assert.equal(attempt.attempt, 1, `${expectedMode}: attempt number`);
  assert.equal(attempt.ok, true, `${expectedMode}: attempt must pass`);
  assert.equal(attempt.status, 0, `${expectedMode}: process status`);
  assert.equal(attempt.signal, null, `${expectedMode}: process signal`);
  assert.equal(attempt.timedOut, false, `${expectedMode}: timeout`);
  assert.deepEqual(attempt.evidenceProblems, [], `${expectedMode}: evidence problems`);
}

function validateCorpusEvidence(evidence, expectedFeature) {
  assert.equal(evidence.schema_version, 'ctox-office-oracle-evidence-v1', `${expectedFeature}: evidence schema`);
  assert.equal(evidence.feature_id, expectedFeature, `${expectedFeature}: feature id`);
  assert.equal(evidence.status, 'differential_passed', `${expectedFeature}: status`);
}

export async function captureReleaseEvidence({
  version,
  lastPublishedReleaseVersion,
  highestExistingTagVersion,
  gitRevision,
  capturedAt,
  releaseWorkflowUrl,
  documentMatrixPath,
  spreadsheetMatrixPath,
  docxCorpusEvidencePath,
  xlsxCorpusEvidencePath,
  outputPath,
}) {
  const normalizedVersion = String(version || '').replace(/^v/, '');
  assert.match(normalizedVersion, /^\d+\.\d+\.\d+$/, 'version must be vMAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH');
  assert.match(String(lastPublishedReleaseVersion || ''), /^\d+\.\d+\.\d+$/, 'last published release version must be MAJOR.MINOR.PATCH');
  assert.match(String(highestExistingTagVersion || ''), /^\d+\.\d+\.\d+$/, 'highest existing tag version must be MAJOR.MINOR.PATCH');
  assert.ok(compareSemver(normalizedVersion, lastPublishedReleaseVersion) > 0,
    `release ${normalizedVersion} must follow published release ${lastPublishedReleaseVersion}`);
  assert.ok(compareSemver(normalizedVersion, highestExistingTagVersion) > 0,
    `release ${normalizedVersion} must follow existing tag ${highestExistingTagVersion}`);
  assert.match(String(gitRevision || ''), /^[0-9a-f]{40}$/i, 'git revision must be a full commit SHA');
  assert.match(String(releaseWorkflowUrl || ''), /^https:\/\/github\.com\/[^/]+\/[^/]+\/actions\/runs\/\d+(?:\/.*)?$/,
    'release workflow URL must identify a GitHub Actions run');

  const [documentMatrix, spreadsheetMatrix, docxCorpus, xlsxCorpus] = await Promise.all([
    readEvidenceFile(documentMatrixPath),
    readEvidenceFile(spreadsheetMatrixPath),
    readEvidenceFile(docxCorpusEvidencePath),
    readEvidenceFile(xlsxCorpusEvidencePath),
  ]);
  validateRestartMatrix(documentMatrix.json, MODES.document, gitRevision);
  validateRestartMatrix(spreadsheetMatrix.json, MODES.spreadsheet, gitRevision);
  validateCorpusEvidence(docxCorpus.json, 'document.docx-roundtrip-corpus');
  validateCorpusEvidence(xlsxCorpus.json, 'spreadsheet.xlsx-roundtrip-corpus');

  const evidence = {
    schema_version: SCHEMA,
    evidence_state: 'release_candidate_gates_passed',
    version: normalizedVersion,
    git_revision: gitRevision,
    candidate_captured_at: isoTimestamp(capturedAt, 'capturedAt'),
    release_workflow_url: releaseWorkflowUrl,
    release_workflow_status: 'pending_completion',
    office_document_restart_matrix_sha256: documentMatrix.sha256,
    office_spreadsheet_restart_matrix_sha256: spreadsheetMatrix.sha256,
    docx_corpus_evidence_sha256: docxCorpus.sha256,
    xlsx_corpus_evidence_sha256: xlsxCorpus.sha256,
    office_restart_retry_count: 0,
    validated_gates: [
      MODES.document,
      MODES.spreadsheet,
      'document.docx-roundtrip-corpus',
      'spreadsheet.xlsx-roundtrip-corpus',
    ],
    promotion_instruction: 'After the GitHub release completes successfully, copy the hash fields into rollout.json, set released_at to the published release timestamp, and set release_workflow_status to passed.',
  };
  if (outputPath) {
    await mkdir(dirname(outputPath), { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(evidence, null, 2)}\n`);
  }
  return evidence;
}

function parseArgs(argv) {
  const args = {};
  for (let index = 0; index < argv.length; index += 1) {
    const key = argv[index];
    if (!key.startsWith('--')) throw new Error(`unexpected argument ${key}`);
    if (key === '--self-test') {
      args.selfTest = true;
      continue;
    }
    const value = argv[index + 1];
    if (!value || value.startsWith('--')) throw new Error(`${key} requires a value`);
    args[key.slice(2)] = value;
    index += 1;
  }
  return args;
}

function syntheticMatrix(mode, gitRevision, attempts = 1) {
  return {
    schema: MATRIX_SCHEMA,
    schemaVersion: 1,
    gitRevision,
    source: { commit: gitRevision, dirty: false },
    requestedModes: [mode],
    configuration: { attempts },
    modes: [{
      mode,
      ok: true,
      attempts: [{ attempt: 1, status: 0, signal: null, timedOut: false, ok: true, evidenceProblems: [] }],
    }],
    ok: true,
  };
}

async function selfTest() {
  const root = await mkdtemp(resolve(tmpdir(), 'ctox-office-release-evidence-'));
  const revision = '0123456789abcdef0123456789abcdef01234567';
  const paths = {
    document: resolve(root, 'document.json'),
    spreadsheet: resolve(root, 'spreadsheet.json'),
    docx: resolve(root, 'docx.json'),
    xlsx: resolve(root, 'xlsx.json'),
    output: resolve(root, 'candidate.json'),
  };
  try {
    await Promise.all([
      writeFile(paths.document, JSON.stringify(syntheticMatrix(MODES.document, revision))),
      writeFile(paths.spreadsheet, JSON.stringify(syntheticMatrix(MODES.spreadsheet, revision))),
      writeFile(paths.docx, JSON.stringify({ schema_version: 'ctox-office-oracle-evidence-v1', feature_id: 'document.docx-roundtrip-corpus', status: 'differential_passed' })),
      writeFile(paths.xlsx, JSON.stringify({ schema_version: 'ctox-office-oracle-evidence-v1', feature_id: 'spreadsheet.xlsx-roundtrip-corpus', status: 'differential_passed' })),
    ]);
    const input = {
      version: 'v0.3.32',
      lastPublishedReleaseVersion: '0.3.27',
      highestExistingTagVersion: '0.3.31',
      gitRevision: revision,
      capturedAt: '2026-07-13T12:00:00Z',
      releaseWorkflowUrl: 'https://github.com/metric-space-ai/ctox/actions/runs/123',
      documentMatrixPath: paths.document,
      spreadsheetMatrixPath: paths.spreadsheet,
      docxCorpusEvidencePath: paths.docx,
      xlsxCorpusEvidencePath: paths.xlsx,
      outputPath: paths.output,
    };
    const evidence = await captureReleaseEvidence(input);
    assert.equal(evidence.office_restart_retry_count, 0);
    assert.equal(evidence.validated_gates.length, 4);
    assert.equal(JSON.parse(await readFile(paths.output, 'utf8')).schema_version, SCHEMA);

    await writeFile(paths.document, JSON.stringify(syntheticMatrix(MODES.document, revision, 2)));
    await assert.rejects(captureReleaseEvidence(input), /retries are forbidden/);
    await assert.rejects(captureReleaseEvidence({
      ...input,
      version: 'v0.3.31',
    }), /must follow existing tag/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
  console.log('CTOX product release evidence capture self-test OK');
}

const isMain = resolve(process.argv[1] || '') === fileURLToPath(import.meta.url);
if (isMain) {
  const args = parseArgs(process.argv.slice(2));
  if (args.selfTest) {
    await selfTest();
  } else {
    const required = ['version', 'git-revision', 'captured-at', 'release-workflow-url', 'document-matrix', 'spreadsheet-matrix', 'docx-corpus-evidence', 'xlsx-corpus-evidence', 'output'];
    for (const key of required) assert.ok(args[key], `--${key} is required`);
    const outputPath = resolve(args.output);
    const rollout = JSON.parse(await readFile(new URL('./rollout.json', import.meta.url), 'utf8'));
    const evidence = await captureReleaseEvidence({
      version: args.version,
      lastPublishedReleaseVersion: rollout.pre_switch_baseline.last_published_release_version,
      highestExistingTagVersion: rollout.pre_switch_baseline.highest_existing_tag_version,
      gitRevision: args['git-revision'],
      capturedAt: args['captured-at'],
      releaseWorkflowUrl: args['release-workflow-url'],
      documentMatrixPath: resolve(args['document-matrix']),
      spreadsheetMatrixPath: resolve(args['spreadsheet-matrix']),
      docxCorpusEvidencePath: resolve(args['docx-corpus-evidence']),
      xlsxCorpusEvidencePath: resolve(args['xlsx-corpus-evidence']),
      outputPath,
    });
    console.log(`CTOX product release candidate evidence written to ${outputPath} (${evidence.version})`);
  }
}
