const MAX_GRAPH_NODES = 240;
const MAX_GRAPH_LINKS = 1800;
export const GRAPH_DETAIL_LEVELS = Object.freeze({
  overview: 36,
  standard: 64,
  deep: 120,
});
export const GRAPH_NODE_KINDS = Object.freeze(['topic', 'concept', 'source', 'evidence', 'measurement']);
export const GRAPH_RELATION_TYPES = Object.freeze([
  'supports',
  'measures',
  'derived_from',
  'part_of',
  'contradicts',
  'correlates_with',
  'co_occurs',
]);
const SEMANTIC_TEXT_FIELDS = new Set([
  'title', 'subtitle', 'summary', 'description', 'abstract', 'note', 'contribution_note',
  'fact_label', 'quote', 'content', 'finding', 'conclusion', 'method', 'material',
]);
const SEMANTIC_PHRASE_FIELDS = new Set([
  'tags', 'keywords', 'topics', 'entities', 'subject_terms', 'controlled_terms', 'aliases', 'aliases_json',
]);
const TECHNICAL_TOKENS = new Set([
  'snapshot', 'snapshots', 'snapshotid', 'snapshothash', 'sha256', 'canonical', 'verification',
  'verified', 'verificationstatus', 'extracted', 'retrieved', 'sourceid', 'evidenceid', 'claimid',
]);
const CLUSTER_PALETTE = Object.freeze([
  '#58a9d8',
  '#79b85a',
  '#f0a13d',
  '#df554d',
  '#a985d8',
  '#79c9c3',
  '#d7c34d',
  '#d883b8',
]);

const STOP_WORDS = new Set([
  'about', 'after', 'again', 'against', 'alle', 'allem', 'allen', 'aller', 'alles', 'also', 'and', 'ander', 'andere',
  'anderen', 'anderer', 'anderes', 'auch', 'auf', 'aus', 'bei', 'beim', 'bereits', 'between', 'beyond', 'both',
  'can', 'could', 'dabei', 'damit', 'dann', 'das', 'dass', 'data', 'dem', 'den', 'der', 'des', 'die', 'dies',
  'diese', 'diesem', 'diesen', 'dieser', 'dieses', 'durch', 'each', 'eine', 'einem', 'einen', 'einer', 'eines',
  'evidence', 'for', 'from', 'fuer', 'für', 'gegen', 'haben', 'has', 'hat', 'hier', 'how', 'ihre', 'ihrem', 'ihren',
  'ihrer', 'ihres', 'into', 'ist', 'its', 'jede', 'jedem', 'jeden', 'jeder', 'jedes', 'kann', 'kein', 'keine',
  'mit', 'more', 'nach', 'nicht', 'noch', 'oder', 'ohne', 'only', 'other', 'research', 'schon', 'sein', 'seine',
  'seinem', 'seinen', 'seiner', 'seines', 'should', 'source', 'sources', 'sowie', 'such', 'than', 'that', 'the',
  'their', 'then', 'these', 'this', 'those', 'through', 'über', 'und', 'unter', 'use', 'using', 'von', 'vor',
  'was', 'werden', 'what', 'when', 'where', 'which', 'while', 'with', 'wird', 'would', 'zum', 'zur',
]);

export function buildResearchGraphProjection(input = {}) {
  const detailLevel = normalizeDetailLevel(input.detailLevel);
  const visibleLimit = clamp(
    Number(input.visibleLimit) || GRAPH_DETAIL_LEVELS[detailLevel],
    12,
    MAX_GRAPH_NODES,
  );
  if (input.graphContractStatus === 'invalid_graph_contract') {
    return emptyProjection(detailLevel, 'invalid_graph_contract', input.graphContractErrors || []);
  }
  const persisted = projectionFromPersisted(input);
  if (persisted?.status === 'invalid_graph_contract') {
    return emptyProjection(detailLevel, persisted.status, persisted.errors);
  }
  const documents = buildDocuments(input);
  const base = persisted || projectionFromResearchRows(input, documents);
  const layered = addRequestedLayers(base.nodes, base.links, documents, input.graphLayer, visibleLimit);
  const enriched = enrichGraph(layered.nodes, layered.links);
  return sliceResearchGraphProjection({
    // Keep one stable layout per layer and expose nested visibility slices to
    // the renderer. Detail changes can then update visibility without
    // replacing the force graph topology.
    layoutNodes: enriched.nodes,
    layoutLinks: enriched.links,
    origin: persisted ? 'persisted' : 'derived',
    status: 'ready',
  }, detailLevel, visibleLimit, input.graphLayer);
}

export function sliceResearchGraphProjection(projection, detailLevel = 'standard', visibleLimit, graphLayer = 'concepts') {
  const layoutNodes = Array.isArray(projection?.layoutNodes) ? projection.layoutNodes : (projection?.nodes || []);
  const layoutLinks = Array.isArray(projection?.layoutLinks) ? projection.layoutLinks : (projection?.links || []);
  const nodes = selectVisibleNodes(layoutNodes, graphLayer, Number(visibleLimit) || GRAPH_DETAIL_LEVELS[detailLevel] || GRAPH_DETAIL_LEVELS.standard);
  const visibleNodeIds = new Set(nodes.map((node) => node.id));
  const links = layoutLinks
    .filter((link) => visibleNodeIds.has(link.source) && visibleNodeIds.has(link.target))
    .slice(0, MAX_GRAPH_LINKS);
  return {
    ...projection,
    nodes,
    links,
    visibleNodeIds: [...visibleNodeIds],
    visibleLinkIds: links.map((link) => link.id),
    topics: summarizeTopics(nodes),
    detailLevel,
    availableNodeCount: layoutNodes.length,
    availableLinkCount: layoutLinks.length,
    metrics: {
      nodeCount: nodes.length,
      linkCount: links.length,
      clusterCount: new Set(nodes.map((node) => node.cluster)).size,
      sourceCount: new Set(nodes.flatMap((node) => node.sourceIds || [])).size,
      evidenceCount: nodes.filter((node) => node.kind === 'evidence' || node.kind === 'measurement').length,
    },
  };
}

function projectionFromPersisted(input) {
  const rawNodes = Array.isArray(input.graphNodeRows) ? input.graphNodeRows : [];
  const rawLinks = Array.isArray(input.graphEdgeRows) ? input.graphEdgeRows : [];
  if (!rawNodes.length && !rawLinks.length) return null;
  if (!rawNodes.length || !rawLinks.length) return { status: 'invalid_graph_contract', errors: ['nodes_and_edges_required'] };
  const validation = validatePersistedGraph(rawNodes, rawLinks, verifiedSourceIds(input));
  if (!validation.valid) return { status: 'invalid_graph_contract', errors: validation.errors };
  const nodes = rawNodes.map((row, index) => {
    const id = firstString(row, ['node_id', 'id', 'concept_id', 'key']);
    return {
      id,
      label: firstString(row, ['label', 'title', 'concept', 'term']) || formatConceptLabel(id),
      kind: firstString(row, ['kind', 'node_type', 'type']) || 'concept',
      description: firstString(row, ['description', 'definition', 'summary']),
      clusterLabel: firstString(row, ['cluster_label', 'topic_label', 'community_label']),
      clusterHint: firstString(row, ['cluster_id', 'cluster', 'community']) || '',
      occurrences: positiveNumber(row.occurrences ?? row.frequency ?? row.count, 1),
      centralityHint: normalizedUnit(row.betweenness_centrality ?? row.centrality ?? row.importance),
      confidence: normalizedUnit(row.confidence),
      evidenceCount: positiveNumber(row.evidence_count, 0),
      aliases: parseStringList(row.aliases_json ?? row.aliases),
      sourceIds: parseStringList(row.source_ids_json ?? row.source_ids ?? row.sources),
      provenance: parseProvenance(row.provenance_json ?? row.provenance),
    };
  });
  const nodeIds = new Set(nodes.map((node) => node.id));
  const links = rawLinks.map((row, index) => ({
    id: firstString(row, ['edge_id', 'id']),
    source: firstString(row, ['source_id', 'source', 'from']),
    target: firstString(row, ['target_id', 'target', 'to']),
    relationType: firstString(row, ['relation_type', 'relation', 'type']),
    label: firstString(row, ['label', 'relation_label']),
    weight: positiveNumber(row.weight ?? row.occurrences ?? row.count, 1),
    confidence: normalizedUnit(row.confidence),
    sourceIds: parseStringList(row.source_ids_json ?? row.source_ids ?? row.sources),
    provenance: parseProvenance(row.provenance_json ?? row.provenance),
  }));
  return links.length ? { nodes, links } : null;
}

function validatePersistedGraph(rawNodes, rawLinks, verifiedIds) {
  const errors = [];
  const sourceIds = new Set(verifiedIds);
  const nodeIds = new Set();
  for (const [index, row] of rawNodes.entries()) {
    const id = firstString(row, ['node_id', 'id', 'concept_id', 'key']);
    const label = firstString(row, ['label', 'title', 'concept', 'term']);
    const kind = firstString(row, ['kind', 'node_type', 'type']);
    const rowSourceIds = parseStringList(row.source_ids_json ?? row.source_ids ?? row.sources);
    if (!id || nodeIds.has(id)) errors.push(`node[${index}].node_id`);
    if (id) nodeIds.add(id);
    if (!isSemanticLabel(label)) errors.push(`node[${index}].label`);
    if (!GRAPH_NODE_KINDS.includes(kind)) errors.push(`node[${index}].kind`);
    if (!rowSourceIds.length || rowSourceIds.some((sourceId) => !sourceIds.has(sourceId))) errors.push(`node[${index}].source_ids`);
    if (!isConfidence(row.confidence)) errors.push(`node[${index}].confidence`);
    if (!parseProvenance(row.provenance_json ?? row.provenance)) errors.push(`node[${index}].provenance`);
  }
  for (const [index, row] of rawLinks.entries()) {
    const source = firstString(row, ['source_id', 'source', 'from']);
    const target = firstString(row, ['target_id', 'target', 'to']);
    const relation = firstString(row, ['relation_type', 'relation', 'type']);
    const rowSourceIds = parseStringList(row.source_ids_json ?? row.source_ids ?? row.sources);
    if (!firstString(row, ['edge_id', 'id'])) errors.push(`edge[${index}].edge_id`);
    if (!source || !target || source === target || !nodeIds.has(source) || !nodeIds.has(target)) errors.push(`edge[${index}].endpoints`);
    if (!GRAPH_RELATION_TYPES.includes(relation)) errors.push(`edge[${index}].relation_type`);
    if (!rowSourceIds.length || rowSourceIds.some((sourceId) => !sourceIds.has(sourceId))) errors.push(`edge[${index}].source_ids`);
    if (!isConfidence(row.confidence)) errors.push(`edge[${index}].confidence`);
    if (!parseProvenance(row.provenance_json ?? row.provenance)) errors.push(`edge[${index}].provenance`);
  }
  return { valid: errors.length === 0, errors };
}

function verifiedSourceIds(input) {
  if (input.verifiedSourceIds) return new Set(input.verifiedSourceIds);
  return new Set((input.sourceModels || []).filter((source) => source?.evidenceEligible === true).map((source) => String(source.id || '')).filter(Boolean));
}

function emptyProjection(detailLevel, status = 'ready', errors = []) {
  return {
    nodes: [],
    links: [],
    topics: [],
    origin: status === 'invalid_graph_contract' ? 'invalid' : 'derived',
    status,
    errors,
    detailLevel,
    availableNodeCount: 0,
    availableLinkCount: 0,
    metrics: { nodeCount: 0, linkCount: 0, clusterCount: 0, sourceCount: 0, evidenceCount: 0 },
  };
}

function projectionFromResearchRows(input, documents = buildDocuments(input)) {
  const nodeStats = new Map();
  const edgeStats = new Map();
  for (const document of documents) {
    const concepts = documentConcepts(document);
    const seen = new Set();
    for (const concept of concepts) {
      const current = nodeStats.get(concept.id) || {
        id: concept.id,
        label: concept.label,
        kind: concept.kind,
        occurrences: 0,
        documentCount: 0,
        sourceIds: new Set(),
      };
      current.occurrences += concept.weight;
      if (!seen.has(concept.id)) current.documentCount += 1;
      if (document.sourceId) current.sourceIds.add(document.sourceId);
      nodeStats.set(concept.id, current);
      seen.add(concept.id);
    }

    for (let index = 0; index < concepts.length; index += 1) {
      const from = concepts[index];
      const max = Math.min(concepts.length, index + 5);
      for (let peerIndex = index + 1; peerIndex < max; peerIndex += 1) {
        const to = concepts[peerIndex];
        if (from.id === to.id) continue;
        const [source, target] = from.id < to.id ? [from.id, to.id] : [to.id, from.id];
        const key = `${source}\u0000${target}`;
        const edge = edgeStats.get(key) || {
          id: `edge:${stableHash(key).toString(36)}`,
          source,
          target,
          weight: 0,
          sourceIds: new Set(),
          relationType: 'co_occurs',
          label: 'Co-occurs',
          provenance: { kind: 'derived', method: 'verified_source_cooccurrence' },
        };
        edge.weight += Math.max(0.5, Math.min(from.weight, to.weight));
        if (document.sourceId) edge.sourceIds.add(document.sourceId);
        edgeStats.set(key, edge);
      }
    }
  }

  let nodes = [...nodeStats.values()]
    .map((node) => ({
      ...node,
      sourceIds: [...node.sourceIds],
      rawScore: node.occurrences + node.documentCount * 2.8 + (node.kind === 'topic' ? 4 : 0),
      confidence: 0.5,
      provenance: { kind: 'derived', method: 'verified_source_text' },
    }))
    .sort((left, right) => right.rawScore - left.rawScore || left.id.localeCompare(right.id))
    .slice(0, MAX_GRAPH_NODES);

  const nodeIds = new Set(nodes.map((node) => node.id));
  let links = [...edgeStats.values()]
    .filter((link) => nodeIds.has(link.source) && nodeIds.has(link.target))
    .map((link) => ({ ...link, sourceIds: [...link.sourceIds] }))
    .sort((left, right) => right.weight - left.weight || left.id.localeCompare(right.id))
    .slice(0, MAX_GRAPH_LINKS);

  return { nodes, links };
}

function buildDocuments(input) {
  const sourceModels = Array.isArray(input.sourceModels) ? input.sourceModels : [];
  const evidenceBySource = new Map();
  for (const row of input.measurementRows || []) {
    const sourceId = firstString(row, ['source_id', 'sourceId', 'source', 'dataset_id']);
    if (!sourceId) continue;
    if (!evidenceBySource.has(sourceId)) evidenceBySource.set(sourceId, []);
    evidenceBySource.get(sourceId).push(row);
  }
  const documents = sourceModels
    .filter((source) => String(source?.id || '').trim())
    .map((source, index) => ({
    id: `document:${source.id || index}`,
    sourceId: String(source.id).trim(),
    title: String(source.title || source.row?.title || `Source ${index + 1}`),
    score: Number(source.score || 0),
    source,
    evidence: evidenceBySource.get(String(source.id || '')) || [],
    text: collectSemanticText([source, source.row, source.curated, evidenceBySource.get(String(source.id || '')) || []]),
    phrases: collectSemanticPhrases([source, source.row, source.curated]),
  }));
  return documents;
}

function documentConcepts(document) {
  const tokens = tokenize(document.text);
  const result = [];
  const titleTokens = tokenize(document.title).filter((token) => !STOP_WORDS.has(token.normalized));
  if (titleTokens.length >= 2) {
    const phraseTokens = titleTokens.slice(0, 4);
    const normalized = phraseTokens.map((token) => token.normalized).join(' ');
    result.push({
      id: `topic:${normalized}`,
      label: phraseTokens.map((token) => token.label).join(' '),
      kind: 'topic',
      weight: 4 + Math.min(4, document.score / 25),
    });
  }
  for (const phrase of document.phrases || []) {
    const phraseTokens = tokenize(phrase).slice(0, 6);
    if (!phraseTokens.length) continue;
    const normalized = phraseTokens.map((token) => token.normalized).join(' ');
    result.push({
      id: `topic:${normalized}`,
      label: phraseTokens.map((token) => token.label).join(' '),
      kind: 'topic',
      weight: 5,
    });
  }
  for (const token of tokens) {
    result.push({
      id: `concept:${token.normalized}`,
      label: token.label,
      kind: 'concept',
      weight: 1,
    });
  }
  return result.slice(0, 160);
}

function addRequestedLayers(nodes, links, documents, layer = 'concepts', _visibleLimit = GRAPH_DETAIL_LEVELS.standard) {
  if (layer !== 'sources' && layer !== 'evidence') return { nodes, links };
  const bySource = new Map();
  for (const node of nodes) {
    for (const sourceId of node.sourceIds || []) {
      if (!bySource.has(sourceId)) bySource.set(sourceId, []);
      bySource.get(sourceId).push(node);
    }
  }
  const extraNodes = [];
  const extraLinks = [];
  // Build one stable deep projection, then take nested slices for each detail
  // level. Otherwise changing detail also changes the force topology.
  const sourceBudget = layer === 'evidence'
    ? Math.floor(GRAPH_DETAIL_LEVELS.deep * 0.2)
    : Math.floor(GRAPH_DETAIL_LEVELS.deep * 0.34);
  const evidenceBudget = layer === 'evidence' ? Math.floor(GRAPH_DETAIL_LEVELS.deep * 0.3) : 0;
  const rankedDocuments = documents
    .filter((item) => item.sourceId)
    .sort((left, right) => right.score - left.score || left.sourceId.localeCompare(right.sourceId))
    .slice(0, sourceBudget);
  let remainingEvidence = evidenceBudget;
  for (const document of rankedDocuments) {
    const sourceNodeId = `source:${document.sourceId}`;
    if (!nodes.some((node) => node.id === sourceNodeId) && !extraNodes.some((node) => node.id === sourceNodeId)) {
      extraNodes.push({
        id: sourceNodeId,
        label: shortLabel(document.title, 42),
        kind: 'source',
        occurrences: 1,
        documentCount: 1,
        rawScore: 3 + document.score / 20,
        sourceIds: [document.sourceId],
        confidence: normalizedUnit(document.source?.row?.confidence) || 0.5,
        provenance: { kind: 'derived', method: 'source_catalog' },
      });
    }
    for (const concept of (bySource.get(document.sourceId) || []).slice(0, 6)) {
      extraLinks.push({
        id: `edge:${stableHash(`${sourceNodeId}\u0000${concept.id}`).toString(36)}`,
        source: sourceNodeId,
        target: concept.id,
        weight: 2,
        sourceIds: [document.sourceId],
        relationType: 'supports',
        label: 'Supports',
        confidence: normalizedUnit(document.source?.row?.confidence) || 0.5,
        provenance: { kind: 'derived', method: 'source_concept_binding' },
      });
    }
    if (layer === 'evidence') {
      const evidenceForSource = document.evidence.slice(0, Math.min(3, remainingEvidence));
      for (const [index, evidence] of evidenceForSource.entries()) {
        const evidenceRecordId = firstString(evidence, ['evidence_id', 'claim_id', 'id']);
        const evidenceId = `evidence:${document.sourceId}:${evidenceRecordId || index}`;
        extraNodes.push({
          id: evidenceId,
          label: shortLabel(firstString(evidence, ['fact_label', 'title', 'name']) || `Beleg ${index + 1}`, 48),
          kind: 'evidence',
          description: firstString(evidence, ['quote', 'summary', 'description']),
          occurrences: 1,
          documentCount: 1,
          rawScore: 2,
          sourceIds: [document.sourceId],
          confidence: normalizedUnit(evidence.confidence) || 0.5,
          provenance: { kind: 'derived', method: 'evidence_points', evidenceId: evidenceRecordId },
        });
        extraLinks.push({
          id: `edge:${stableHash(`${sourceNodeId}\u0000${evidenceId}`).toString(36)}`,
          source: sourceNodeId,
          target: evidenceId,
          weight: 1.5,
          sourceIds: [document.sourceId],
          relationType: 'measures',
          label: 'Measures',
          confidence: normalizedUnit(evidence.confidence) || 0.5,
          provenance: { kind: 'derived', method: 'evidence_binding', evidenceId: evidenceRecordId },
        });
      }
      remainingEvidence -= evidenceForSource.length;
    }
  }
  return {
    nodes: [...nodes.slice(0, Math.max(12, MAX_GRAPH_NODES - extraNodes.length)), ...extraNodes].slice(0, MAX_GRAPH_NODES),
    links: [...links, ...extraLinks].slice(0, MAX_GRAPH_LINKS),
  };
}

function selectVisibleNodes(nodes, layer, visibleLimit) {
  const take = (values, count) => values.slice(0, Math.max(0, count));
  const sources = nodes.filter((node) => node.kind === 'source');
  const evidence = nodes.filter((node) => node.kind === 'evidence' || node.kind === 'measurement');
  const concepts = nodes.filter((node) => !sources.includes(node) && !evidence.includes(node));
  let selected;
  if (layer === 'sources') {
    const sourceCount = Math.min(sources.length, Math.max(8, Math.floor(visibleLimit * 0.34)));
    selected = [...take(sources, sourceCount), ...take(concepts, visibleLimit - sourceCount)];
  } else if (layer === 'evidence') {
    const evidenceCount = Math.min(evidence.length, Math.max(10, Math.floor(visibleLimit * 0.3)));
    const sourceCount = Math.min(sources.length, Math.max(6, Math.floor(visibleLimit * 0.2)));
    selected = [
      ...take(evidence, evidenceCount),
      ...take(sources, sourceCount),
      ...take(concepts, visibleLimit - evidenceCount - sourceCount),
    ];
  } else {
    selected = take(concepts, visibleLimit);
  }
  return selected.sort((left, right) => left.rank - right.rank || left.id.localeCompare(right.id));
}

function enrichGraph(rawNodes, rawLinks) {
  const nodes = rawNodes.map((node) => ({ ...node }));
  const nodeById = new Map(nodes.map((node) => [node.id, node]));
  const links = rawLinks
    .map((link, index) => ({
      ...link,
      id: link.id || `edge_${index + 1}`,
      source: typeof link.source === 'object' ? link.source.id : link.source,
      target: typeof link.target === 'object' ? link.target.id : link.target,
      weight: positiveNumber(link.weight, 1),
      relationType: link.relationType || 'co_occurs',
      label: link.label || relationLabel(link.relationType || 'co_occurs'),
      confidence: isConfidence(link.confidence) ? normalizedUnit(link.confidence) : 0.5,
      provenance: parseProvenance(link.provenance) || { kind: 'derived', method: 'cooccurrence' },
    }))
    .filter((link) => link.source !== link.target && nodeById.has(link.source) && nodeById.has(link.target));
  const adjacency = new Map(nodes.map((node) => [node.id, new Map()]));
  for (const link of links) {
    adjacency.get(link.source).set(link.target, (adjacency.get(link.source).get(link.target) || 0) + link.weight);
    adjacency.get(link.target).set(link.source, (adjacency.get(link.target).get(link.source) || 0) + link.weight);
  }

  const clusterById = nodes.some((node) => node.clusterHint)
    ? hintedClusters(nodes)
    : louvainFirstPhase(nodes, adjacency);
  const betweenness = approximateBetweenness(nodes, adjacency);
  const maxDegree = Math.max(1, ...nodes.map((node) => [...adjacency.get(node.id).values()].reduce((sum, value) => sum + value, 0)));
  const maxBetweenness = Math.max(1, ...betweenness.values());
  for (const node of nodes) {
    const weightedDegree = [...adjacency.get(node.id).values()].reduce((sum, value) => sum + value, 0);
    const degreeUnit = weightedDegree / maxDegree;
    const centralityUnit = node.centralityHint || (betweenness.get(node.id) || 0) / maxBetweenness;
    node.centrality = centralityUnit;
    node.degree = weightedDegree;
    node.importance = clamp(centralityUnit * 0.74 + degreeUnit * 0.26, 0, 1);
    node.cluster = clusterById.get(node.id) || 0;
    node.color = CLUSTER_PALETTE[node.cluster % CLUSTER_PALETTE.length];
  }
  nodes.sort((left, right) => right.importance - left.importance || right.occurrences - left.occurrences || left.id.localeCompare(right.id));
  nodes.forEach((node, rank) => {
    node.rank = rank + 1;
    node.primary = node.kind === 'topic' || rank < Math.min(12, Math.ceil(nodes.length * 0.12));
    node.visualSize = Math.min(10.6, 2.2 + Math.pow(Math.max(0.015, node.importance), 0.72) * 8.4);
    node.labelSize = Math.min(9, node.primary
      ? 4.6 + Math.pow(Math.max(0.02, node.importance), 0.7) * 4.4
      : 3.2 + Math.pow(Math.max(0.02, node.importance), 0.8) * 2.4);
  });

  const maxWeight = Math.max(1, ...links.map((link) => link.weight));
  links.sort((left, right) => right.weight - left.weight || left.id.localeCompare(right.id));
  for (const [index, link] of links.entries()) {
    const unit = link.weight / maxWeight;
    const sourceNode = nodeById.get(link.source);
    link.color = sourceNode?.color || '#7f8a93';
    link.visualWidth = 0.18 + Math.pow(unit, 1.28) * 5.8;
    link.opacity = 0.16 + unit * 0.58;
    const curveSeed = ((stableHash(link.id) % 1000) / 1000) - 0.5;
    link.curvature = Math.abs(curveSeed) < 0.08 ? (index % 2 ? 0.12 : -0.12) : curveSeed * 0.72;
    link.particles = unit > 0.72 ? 2 : unit > 0.48 ? 1 : 0;
  }
  return { nodes, links };
}

function louvainFirstPhase(nodes, adjacency) {
  const community = new Map(nodes.map((node) => [node.id, node.id]));
  const degree = new Map(nodes.map((node) => [node.id, [...adjacency.get(node.id).values()].reduce((sum, value) => sum + value, 0)]));
  const totals = new Map(degree);
  const twiceWeight = Math.max(1, [...degree.values()].reduce((sum, value) => sum + value, 0));
  const ordered = [...nodes].sort((left, right) => (degree.get(right.id) - degree.get(left.id)) || left.id.localeCompare(right.id));
  for (let pass = 0; pass < 8; pass += 1) {
    let moved = false;
    for (const node of ordered) {
      const nodeId = node.id;
      const current = community.get(nodeId);
      const nodeDegree = degree.get(nodeId) || 0;
      const weightsByCommunity = new Map();
      for (const [neighborId, weight] of adjacency.get(nodeId)) {
        const neighborCommunity = community.get(neighborId);
        weightsByCommunity.set(neighborCommunity, (weightsByCommunity.get(neighborCommunity) || 0) + weight);
      }
      totals.set(current, (totals.get(current) || 0) - nodeDegree);
      let best = current;
      let bestGain = 0;
      for (const [candidate, internalWeight] of weightsByCommunity) {
        const gain = internalWeight - ((totals.get(candidate) || 0) * nodeDegree / twiceWeight);
        if (gain > bestGain + 1e-9 || (Math.abs(gain - bestGain) < 1e-9 && String(candidate) < String(best))) {
          best = candidate;
          bestGain = gain;
        }
      }
      community.set(nodeId, best);
      totals.set(best, (totals.get(best) || 0) + nodeDegree);
      if (best !== current) moved = true;
    }
    if (!moved) break;
  }
  return compactCommunities(community, degree);
}

function hintedClusters(nodes) {
  const hints = [...new Set(nodes.map((node) => node.clusterHint || 'unassigned'))].sort();
  const indexByHint = new Map(hints.map((hint, index) => [hint, index]));
  return new Map(nodes.map((node) => [node.id, indexByHint.get(node.clusterHint || 'unassigned')]));
}

function compactCommunities(community, degree) {
  const weights = new Map();
  for (const [nodeId, communityId] of community) {
    weights.set(communityId, (weights.get(communityId) || 0) + (degree.get(nodeId) || 0));
  }
  const ordered = [...weights.entries()].sort((left, right) => right[1] - left[1] || String(left[0]).localeCompare(String(right[0])));
  const compact = new Map(ordered.map(([id], index) => [id, index % CLUSTER_PALETTE.length]));
  return new Map([...community].map(([nodeId, communityId]) => [nodeId, compact.get(communityId)]));
}

function approximateBetweenness(nodes, adjacency) {
  const centrality = new Map(nodes.map((node) => [node.id, 0]));
  const ordered = [...nodes].sort((left, right) => adjacency.get(right.id).size - adjacency.get(left.id).size || left.id.localeCompare(right.id));
  const sampleCount = Math.min(nodes.length > 140 ? 12 : 20, ordered.length);
  const sources = sampleCount === ordered.length
    ? ordered
    : Array.from({ length: sampleCount }, (_, index) => ordered[Math.floor(index * ordered.length / sampleCount)]);
  for (const sourceNode of sources) {
    const stack = [];
    const predecessors = new Map(nodes.map((node) => [node.id, []]));
    const paths = new Map(nodes.map((node) => [node.id, 0]));
    const distance = new Map(nodes.map((node) => [node.id, -1]));
    paths.set(sourceNode.id, 1);
    distance.set(sourceNode.id, 0);
    const queue = [sourceNode.id];
    for (let cursor = 0; cursor < queue.length; cursor += 1) {
      const current = queue[cursor];
      stack.push(current);
      for (const neighbor of adjacency.get(current).keys()) {
        if (distance.get(neighbor) < 0) {
          queue.push(neighbor);
          distance.set(neighbor, distance.get(current) + 1);
        }
        if (distance.get(neighbor) === distance.get(current) + 1) {
          paths.set(neighbor, paths.get(neighbor) + paths.get(current));
          predecessors.get(neighbor).push(current);
        }
      }
    }
    const dependency = new Map(nodes.map((node) => [node.id, 0]));
    while (stack.length) {
      const nodeId = stack.pop();
      for (const predecessor of predecessors.get(nodeId)) {
        const pathCount = paths.get(nodeId) || 1;
        const contribution = (paths.get(predecessor) / pathCount) * (1 + dependency.get(nodeId));
        dependency.set(predecessor, dependency.get(predecessor) + contribution);
      }
      if (nodeId !== sourceNode.id) centrality.set(nodeId, centrality.get(nodeId) + dependency.get(nodeId));
    }
  }
  return centrality;
}

function summarizeTopics(nodes) {
  const byCluster = new Map();
  for (const node of nodes) {
    if (!byCluster.has(node.cluster)) byCluster.set(node.cluster, []);
    byCluster.get(node.cluster).push(node);
  }
  return [...byCluster.entries()]
    .map(([cluster, clusterNodes]) => ({
      id: cluster,
      color: CLUSTER_PALETTE[cluster % CLUSTER_PALETTE.length],
      label: clusterNodes.find((node) => node.clusterLabel)?.clusterLabel
        || clusterNodes.find((node) => node.kind === 'topic')?.label
        || clusterNodes.sort((left, right) => right.importance - left.importance)[0]?.label
        || `Themenfeld ${cluster + 1}`,
      nodeId: clusterNodes[0]?.id || '',
      nodeCount: clusterNodes.length,
      importance: clusterNodes.reduce((sum, node) => sum + node.importance, 0),
    }))
    .sort((left, right) => right.importance - left.importance || left.id - right.id);
}

function tokenize(value) {
  const matches = String(value || '').match(/[\p{L}\p{N}][\p{L}\p{N}+#.-]{2,}/gu) || [];
  return matches
    .map((label) => ({ label: normalizeDisplayLabel(label), normalized: normalizeToken(label) }))
    .filter((token) => token.normalized.length >= 3 && !STOP_WORDS.has(token.normalized) && !TECHNICAL_TOKENS.has(token.normalized) && !/^\d+$/.test(token.normalized));
}

function collectText(value, depth = 0) {
  if (depth > 4 || value === null || value === undefined) return '';
  if (typeof value === 'string' || typeof value === 'number') return String(value);
  if (Array.isArray(value)) return value.slice(0, 80).map((item) => collectText(item, depth + 1)).join(' ');
  if (typeof value === 'object') {
    return Object.values(value).slice(0, 80).map((item) => collectText(item, depth + 1)).join(' ');
  }
  return '';
}

function collectSemanticText(value, depth = 0, field = '') {
  if (depth > 5 || value === null || value === undefined) return '';
  if (typeof value === 'string' || typeof value === 'number') {
    return !field || SEMANTIC_TEXT_FIELDS.has(field) ? String(value) : '';
  }
  if (Array.isArray(value)) {
    return value.slice(0, 80).map((item) => collectSemanticText(item, depth + 1, field)).join(' ');
  }
  if (typeof value === 'object') {
    return Object.entries(value)
      .filter(([key]) => SEMANTIC_TEXT_FIELDS.has(key) || SEMANTIC_PHRASE_FIELDS.has(key))
      .map(([key, item]) => collectSemanticText(item, depth + 1, key))
      .join(' ');
  }
  return '';
}

function collectSemanticPhrases(value, depth = 0, field = '') {
  if (depth > 5 || value === null || value === undefined) return [];
  if (typeof value === 'string') {
    if (!SEMANTIC_PHRASE_FIELDS.has(field)) return [];
    const raw = value.trim();
    if (raw.startsWith('[')) {
      try {
        const parsed = JSON.parse(raw);
        if (Array.isArray(parsed)) return parsed.flatMap((item) => collectSemanticPhrases(String(item), depth + 1, 'tags'));
      } catch {}
    }
    return raw.split(/[,;|]/).map((item) => item.trim()).filter(Boolean).filter((item) => !isTechnicalPhrase(item));
  }
  if (Array.isArray(value)) {
    return value.slice(0, 80).flatMap((item) => collectSemanticPhrases(item, depth + 1, field));
  }
  if (typeof value === 'object') {
    return Object.entries(value)
      .filter(([key]) => SEMANTIC_PHRASE_FIELDS.has(key))
      .flatMap(([key, item]) => collectSemanticPhrases(item, depth + 1, key));
  }
  return [];
}

function normalizeDisplayLabel(value) {
  const text = String(value || '').replace(/[._-]+/g, ' ').trim();
  if (/^[A-Z0-9+#]{2,8}$/.test(text)) return text;
  return text.replace(/^\p{L}/u, (letter) => letter.toUpperCase());
}

function normalizeToken(value) {
  return String(value || '')
    .toLocaleLowerCase('de-DE')
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9+#äöüß]+/g, '')
    .trim();
}

function formatConceptLabel(value) {
  return String(value || '')
    .replace(/^(concept|topic|source|evidence):/, '')
    .split(/[\s_-]+/)
    .filter(Boolean)
    .map(normalizeDisplayLabel)
    .join(' ');
}

function parseStringList(value) {
  if (Array.isArray(value)) return value.map(String).filter(Boolean);
  if (typeof value !== 'string') return [];
  const trimmed = value.trim();
  if (!trimmed) return [];
  try {
    const parsed = JSON.parse(trimmed);
    if (Array.isArray(parsed)) return parsed.map(String).filter(Boolean);
  } catch {}
  return trimmed.split(/[,;|]/).map((item) => item.trim()).filter(Boolean);
}

function firstString(value, keys) {
  if (!value || typeof value !== 'object') return '';
  for (const key of keys) {
    const candidate = value[key];
    if (candidate !== null && candidate !== undefined && String(candidate).trim()) return String(candidate).trim();
  }
  return '';
}

function positiveNumber(value, fallback = 0) {
  const number = Number(value);
  return Number.isFinite(number) && number > 0 ? number : fallback;
}

function normalizedUnit(value) {
  const number = Number(value);
  if (!Number.isFinite(number) || number <= 0) return 0;
  return clamp(number > 1 ? number / 100 : number, 0, 1);
}

function stableHash(value) {
  let hash = 2166136261;
  for (const char of String(value || '')) {
    hash ^= char.codePointAt(0);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

function shortLabel(value, max) {
  const text = String(value || '').trim();
  return text.length > max ? `${text.slice(0, Math.max(1, max - 1)).trim()}…` : text;
}

function clamp(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function normalizeDetailLevel(value) {
  return Object.hasOwn(GRAPH_DETAIL_LEVELS, value) ? value : 'standard';
}

function isConfidence(value) {
  const number = Number(value);
  return Number.isFinite(number) && number >= 0 && number <= 100;
}

function parseProvenance(value) {
  if (value && typeof value === 'object' && !Array.isArray(value) && Object.keys(value).length) return value;
  if (typeof value !== 'string' || !value.trim()) return null;
  try {
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) && Object.keys(parsed).length ? parsed : null;
  } catch {
    return null;
  }
}

function isSemanticLabel(value) {
  const label = String(value || '').trim();
  if (label.length < 2 || /^https?:\/\//i.test(label) || /^sha256:/i.test(label)) return false;
  return !isTechnicalPhrase(label) && !/^(?:node|edge|concept|topic)[:_][a-z0-9_-]+$/i.test(label);
}

function isTechnicalPhrase(value) {
  return /https?:\/\/|sha256:|snapshot|canonical|source[_ -]?id|evidence[_ -]?id|claim[_ -]?id/i.test(String(value || ''));
}

function relationLabel(relation) {
  return String(relation || 'co_occurs').replace(/_/g, ' ');
}

export const __researchGraphDataTestHooks = {
  approximateBetweenness,
  collectText,
  collectSemanticPhrases,
  collectSemanticText,
  louvainFirstPhase,
  projectionFromPersisted,
  projectionFromResearchRows,
  validatePersistedGraph,
  tokenize,
};
