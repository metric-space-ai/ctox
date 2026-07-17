import assert from 'node:assert/strict';
import { performance } from 'node:perf_hooks';
import test from 'node:test';

import { buildResearchGraphProjection } from '../research-graph-data.mjs';

test('derives a clustered semantic graph with visual weights from research rows', () => {
  const projection = buildResearchGraphProjection({
    task: {
      id: 'research_1',
      title: 'Autonomous AI Employee Platforms',
      prompt: 'Compare agent orchestration, enterprise readiness, evidence quality and workflow automation.',
    },
    sourceModels: [
      {
        id: 'source_a',
        title: 'Enterprise agent orchestration benchmark',
        subtitle: 'Scholarly source',
        note: 'Agent workflows, governance and audit evidence for enterprise automation.',
        score: 92,
        row: { summary: 'Autonomous agents coordinate tools, workflows and human approvals.' },
      },
      {
        id: 'source_b',
        title: 'Workflow automation market evidence',
        subtitle: 'Market report',
        note: 'Enterprise workflow adoption, integrations, pricing and customer evidence.',
        score: 81,
        row: { summary: 'Workflow platforms combine orchestration, integrations and governance.' },
      },
    ],
    measurementRows: [
      { source_id: 'source_a', fact_label: 'Governance coverage', fact_value: 88, quote: 'Audit controls cover agent workflows.' },
    ],
    visibleLimit: 120,
  });

  assert.equal(projection.origin, 'derived');
  assert.ok(projection.nodes.length > 8);
  assert.ok(projection.links.length > 5);
  assert.ok(projection.topics.length >= 1);
  assert.ok(projection.nodes.every((node) => Number.isFinite(node.importance) && node.visualSize > 0 && node.labelSize > 0));
  assert.ok(projection.nodes.every((node) => node.visualSize <= 10.6 && node.labelSize <= 9));
  assert.ok(projection.links.every((link) => Number.isFinite(link.curvature) && link.visualWidth > 0));
  assert.ok(projection.nodes.some((node) => node.primary));
});

test('prefers persisted graph rows and preserves source provenance', () => {
  const provenance = JSON.stringify({ table: 'semantic_graph_nodes', method: 'evidence_grounded' });
  const projection = buildResearchGraphProjection({
    graphNodeRows: [
      { node_id: 'concept:graph', label: 'Graph', kind: 'concept', confidence: 0.9, provenance_json: provenance, cluster_id: 'visualization', occurrences: 12, source_ids_json: '["source_a"]' },
      { node_id: 'concept:research', label: 'Research', kind: 'concept', confidence: 0.86, provenance_json: provenance, cluster_id: 'visualization', occurrences: 9, source_ids_json: '["source_a","source_b"]' },
      { node_id: 'concept:evidence', label: 'Evidence', kind: 'concept', confidence: 0.82, provenance_json: provenance, cluster_id: 'quality', occurrences: 7, source_ids_json: '["source_b"]' },
    ],
    graphEdgeRows: [
      { edge_id: 'edge_1', source_id: 'concept:graph', target_id: 'concept:research', relation_type: 'supports', confidence: 0.8, provenance_json: provenance, weight: 8, source_ids_json: '["source_a"]' },
      { edge_id: 'edge_2', source_id: 'concept:research', target_id: 'concept:evidence', relation_type: 'co_occurs', confidence: 0.7, provenance_json: provenance, weight: 4, source_ids_json: '["source_b"]' },
    ],
    verifiedSourceIds: ['source_a', 'source_b'],
    visibleLimit: 120,
  });

  assert.equal(projection.origin, 'persisted');
  assert.equal(projection.nodes.length, 3);
  assert.equal(projection.links.length, 2);
  assert.deepEqual(projection.nodes.find((node) => node.id === 'concept:research').sourceIds, ['source_a', 'source_b']);
  assert.equal(projection.metrics.clusterCount, 2);
  assert.equal(projection.nodes[0].kind, 'concept');
  assert.equal(projection.links[0].relationType, 'supports');
  assert.equal(projection.links[0].provenance.method, 'evidence_grounded');
});

test('rejects persisted graph rows with invalid relation, provenance, or source binding', () => {
  const projection = buildResearchGraphProjection({
    graphNodeRows: [{ node_id: 'concept:bad', label: 'Bad concept', kind: 'concept', confidence: 0.8, source_ids_json: '["unverified"]' }],
    graphEdgeRows: [{ edge_id: 'edge:bad', source_id: 'concept:bad', target_id: 'concept:bad', relation_type: 'made_up', confidence: 0.8, source_ids_json: '["unverified"]' }],
    verifiedSourceIds: ['verified'],
  });
  assert.equal(projection.status, 'invalid_graph_contract');
  assert.equal(projection.origin, 'invalid');
  assert.deepEqual(projection.nodes, []);
  assert.ok(projection.errors.some((error) => error.includes('relation_type')));
});

test('derived graph excludes task prompt concepts and marks fallback relations explicitly', () => {
  const projection = buildResearchGraphProjection({
    task: { id: 'task-prompt', title: 'Unsubstantiated Roadmap Hypothesis', prompt: 'Secret roadmap token must never rank.' },
    sourceModels: [{ id: 'source_verified', evidenceEligible: true, title: 'Governance evidence study', score: 88, row: { summary: 'Governance evidence supports audit controls.' } }],
    verifiedSourceIds: ['source_verified'],
    visibleLimit: 36,
  });
  assert.doesNotMatch(projection.nodes.map((node) => node.label).join(' '), /Unsubstantiated|roadmap|secret/i);
  assert.ok(projection.links.every((link) => link.relationType === 'co_occurs' && link.provenance?.kind === 'derived'));
});

test('adds source and evidence layers without exceeding graph limits', () => {
  const sourceModels = Array.from({ length: 36 }, (_, index) => ({
    id: `source_${index}`,
    title: `Research source ${index} about agents workflows integration evidence`,
    note: 'agent orchestration governance automation benchmark',
    score: 80 - index,
    row: { summary: 'enterprise workflow evidence and autonomous agent integration' },
  }));
  const measurementRows = sourceModels.flatMap((source, sourceIndex) => Array.from({ length: 3 }, (_, index) => ({
    source_id: source.id,
    fact_label: `Evidence ${sourceIndex}-${index}`,
    quote: 'Measured workflow automation evidence',
  })));
  const projection = buildResearchGraphProjection({
    task: { id: 'layer_task', title: 'Agent research', prompt: 'agent workflow evidence' },
    sourceModels,
    measurementRows,
    graphLayer: 'evidence',
    visibleLimit: 80,
  });

  assert.ok(projection.nodes.length <= 80);
  assert.ok(projection.links.length <= 6000);
  assert.ok(projection.nodes.some((node) => node.kind === 'source'));
  assert.ok(projection.nodes.some((node) => node.kind === 'evidence'));
  const nodeIds = new Set(projection.nodes.map((node) => node.id));
  assert.ok(projection.links.every((link) => nodeIds.has(link.source) && nodeIds.has(link.target)));
});

test('does not turn technical metadata keys into research concepts', () => {
  const projection = buildResearchGraphProjection({
    task: { id: 'bearing', title: 'UAV propeller bearing loads', prompt: 'Evaluate measured thrust and torque.' },
    sourceModels: [{
      id: 'source_a',
      title: 'Measured propeller thrust and torque',
      score: 96,
      row: {
        summary: 'Wind tunnel measurements connect propeller thrust, torque and rotational speed.',
        tags: ['propeller performance', 'bearing load'],
        source_id: 'source_a',
        snapshot_hash: 'sha256:deadbeef',
        canonical_url: 'https://example.test/data',
        verification_status: 'verified',
      },
    }],
    measurementRows: [{
      source_id: 'source_a',
      fact_label: 'Measured torque',
      quote: 'Torque was measured across the complete rotational speed range.',
      snapshot_path: '/runtime/snapshots/source_a.html',
      extracted_at: '2026-07-17T00:00:00Z',
    }],
    detailLevel: 'standard',
  });
  const labels = projection.nodes.map((node) => node.label.toLocaleLowerCase()).join(' ');
  assert.match(labels, /propeller|torque|bearing/);
  assert.doesNotMatch(labels, /snapshot|canonical|verification|extracted|sha256|source id/);
  assert.ok(projection.nodes.some((node) => node.label === 'Propeller Performance'));
});

test('detail levels are nested and preserve the requested source and evidence layers', () => {
  const sourceModels = Array.from({ length: 80 }, (_, index) => ({
    id: `source_${index}`,
    title: `Propeller measurement study ${index}`,
    score: 100 - index / 2,
    row: { summary: 'Measured thrust torque rotational speed vibration bearing load.', tags: ['propeller load', 'bearing design'] },
  }));
  const measurementRows = sourceModels.flatMap((source, index) => Array.from({ length: 3 }, (_, fact) => ({
    source_id: source.id,
    fact_label: `Torque point ${index}-${fact}`,
    quote: 'Measured torque and thrust.',
    confidence: 0.95,
  })));
  const build = (detailLevel) => buildResearchGraphProjection({
    task: { id: 'bearing', title: 'Drone bearing loads', prompt: 'Measured propeller loads.' },
    sourceModels,
    measurementRows,
    graphLayer: 'evidence',
    detailLevel,
  });
  const overview = build('overview');
  const standard = build('standard');
  const deep = build('deep');
  assert.ok(overview.nodes.length <= 36);
  assert.ok(standard.nodes.length <= 64);
  assert.ok(deep.nodes.length <= 120);
  for (const projection of [overview, standard, deep]) {
    assert.ok(projection.nodes.some((node) => node.kind === 'source'));
    assert.ok(projection.nodes.some((node) => node.kind === 'evidence'));
  }
  const standardIds = new Set(standard.nodes.map((node) => node.id));
  const deepIds = new Set(deep.nodes.map((node) => node.id));
  assert.ok(overview.nodes.every((node) => standardIds.has(node.id)));
  assert.ok(standard.nodes.every((node) => deepIds.has(node.id)));
  assert.equal(new Set(deep.nodes.map((node) => node.id)).size, deep.nodes.length);
  assert.ok(overview.nodes.length <= 36 && standard.nodes.length <= 64 && deep.nodes.length <= 120);
});

test('projects SKF-sized research data within an interactive budget', () => {
  const sourceModels = Array.from({ length: 322 }, (_, index) => ({
    id: `source_${index}`,
    title: `UAV propeller dataset ${index}`,
    score: 95 - index / 20,
    row: {
      summary: 'Measured propeller thrust torque rpm vibration and bearing loads under test conditions.',
      tags: ['propeller performance', 'rotor dynamics', 'bearing load'],
    },
  }));
  const measurementRows = Array.from({ length: 816 }, (_, index) => ({
    source_id: `source_${index % sourceModels.length}`,
    fact_label: `Measured load point ${index}`,
    quote: 'Direct experimental thrust and torque measurement.',
    confidence: 0.94,
  }));
  const startedAt = performance.now();
  const projection = buildResearchGraphProjection({
    task: { id: 'skf', title: 'SKF UAV bearing design', prompt: 'Evaluate measured rotor loads.' },
    sourceModels,
    measurementRows,
    graphLayer: 'evidence',
    detailLevel: 'deep',
  });
  const elapsedMs = performance.now() - startedAt;
  assert.ok(projection.nodes.length <= 120);
  assert.ok(projection.links.length <= 1800);
  assert.ok(elapsedMs < 1200, `projection took ${elapsedMs.toFixed(1)} ms`);
});
