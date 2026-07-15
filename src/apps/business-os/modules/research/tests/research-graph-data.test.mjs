import assert from 'node:assert/strict';
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
  assert.ok(projection.links.every((link) => Number.isFinite(link.curvature) && link.visualWidth > 0));
  assert.ok(projection.nodes.some((node) => node.primary));
});

test('prefers persisted graph rows and preserves source provenance', () => {
  const projection = buildResearchGraphProjection({
    graphNodeRows: [
      { node_id: 'concept:graph', label: 'Graph', cluster_id: 'visualization', occurrences: 12, source_ids_json: '["source_a"]' },
      { node_id: 'concept:research', label: 'Research', cluster_id: 'visualization', occurrences: 9, source_ids_json: '["source_a","source_b"]' },
      { node_id: 'concept:evidence', label: 'Evidence', cluster_id: 'quality', occurrences: 7, source_ids_json: '["source_b"]' },
    ],
    graphEdgeRows: [
      { edge_id: 'edge_1', source_id: 'concept:graph', target_id: 'concept:research', weight: 8, source_ids_json: '["source_a"]' },
      { edge_id: 'edge_2', source_id: 'concept:research', target_id: 'concept:evidence', weight: 4, source_ids_json: '["source_b"]' },
    ],
    visibleLimit: 120,
  });

  assert.equal(projection.origin, 'persisted');
  assert.equal(projection.nodes.length, 3);
  assert.equal(projection.links.length, 2);
  assert.deepEqual(projection.nodes.find((node) => node.id === 'concept:research').sourceIds, ['source_a', 'source_b']);
  assert.equal(projection.metrics.clusterCount, 2);
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
