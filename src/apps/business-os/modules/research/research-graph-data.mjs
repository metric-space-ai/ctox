const MAX_GRAPH_NODES = 500;
const MAX_GRAPH_LINKS = 6000;
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
  const persisted = projectionFromPersisted(input);
  const base = persisted || projectionFromResearchRows(input);
  const enriched = enrichGraph(base.nodes, base.links);
  const visibleLimit = clamp(Number(input.visibleLimit) || 120, 12, MAX_GRAPH_NODES);
  const visibleNodeIds = new Set(enriched.nodes.slice(0, visibleLimit).map((node) => node.id));
  const nodes = enriched.nodes.filter((node) => visibleNodeIds.has(node.id));
  const links = enriched.links
    .filter((link) => visibleNodeIds.has(link.source) && visibleNodeIds.has(link.target))
    .slice(0, MAX_GRAPH_LINKS);
  const topics = summarizeTopics(nodes);
  return {
    nodes,
    links,
    topics,
    origin: persisted ? 'persisted' : 'derived',
    availableNodeCount: enriched.nodes.length,
    availableLinkCount: enriched.links.length,
    metrics: {
      nodeCount: nodes.length,
      linkCount: links.length,
      clusterCount: new Set(nodes.map((node) => node.cluster)).size,
      sourceCount: new Set(nodes.flatMap((node) => node.sourceIds || [])).size,
    },
  };
}

function projectionFromPersisted(input) {
  const rawNodes = Array.isArray(input.graphNodeRows) ? input.graphNodeRows : [];
  const rawLinks = Array.isArray(input.graphEdgeRows) ? input.graphEdgeRows : [];
  if (!rawNodes.length || !rawLinks.length) return null;
  const nodes = rawNodes.map((row, index) => {
    const id = firstString(row, ['node_id', 'id', 'concept_id', 'key']) || `concept_${index + 1}`;
    return {
      id,
      label: firstString(row, ['label', 'title', 'concept', 'term']) || formatConceptLabel(id),
      kind: firstString(row, ['kind', 'node_type', 'type']) || 'concept',
      clusterHint: firstString(row, ['cluster_id', 'cluster', 'community']) || '',
      occurrences: positiveNumber(row.occurrences ?? row.frequency ?? row.count, 1),
      centralityHint: normalizedUnit(row.betweenness_centrality ?? row.centrality ?? row.importance),
      sourceIds: parseStringList(row.source_ids_json ?? row.source_ids ?? row.sources),
      provenance: row.provenance_json ?? row.provenance ?? null,
    };
  });
  const nodeIds = new Set(nodes.map((node) => node.id));
  const links = rawLinks.map((row, index) => ({
    id: firstString(row, ['edge_id', 'id']) || `edge_${index + 1}`,
    source: firstString(row, ['source_id', 'source', 'from']),
    target: firstString(row, ['target_id', 'target', 'to']),
    weight: positiveNumber(row.weight ?? row.occurrences ?? row.count, 1),
    sourceIds: parseStringList(row.source_ids_json ?? row.source_ids ?? row.sources),
    provenance: row.provenance_json ?? row.provenance ?? null,
  })).filter((link) => link.source !== link.target && nodeIds.has(link.source) && nodeIds.has(link.target));
  return links.length ? { nodes, links } : null;
}

function projectionFromResearchRows(input) {
  const documents = buildDocuments(input);
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
    }))
    .sort((left, right) => right.rawScore - left.rawScore || left.id.localeCompare(right.id))
    .slice(0, MAX_GRAPH_NODES);

  const nodeIds = new Set(nodes.map((node) => node.id));
  let links = [...edgeStats.values()]
    .filter((link) => nodeIds.has(link.source) && nodeIds.has(link.target))
    .map((link) => ({ ...link, sourceIds: [...link.sourceIds] }))
    .sort((left, right) => right.weight - left.weight || left.id.localeCompare(right.id))
    .slice(0, MAX_GRAPH_LINKS);

  ({ nodes, links } = addRequestedLayers(nodes, links, documents, input.graphLayer));
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
  const documents = sourceModels.map((source, index) => ({
    id: `document:${source.id || index}`,
    sourceId: String(source.id || `source_${index + 1}`),
    title: String(source.title || source.row?.title || `Source ${index + 1}`),
    score: Number(source.score || 0),
    source,
    evidence: evidenceBySource.get(String(source.id || '')) || [],
    text: collectText([
      source.title,
      source.subtitle,
      source.note,
      source.row,
      source.curated,
      evidenceBySource.get(String(source.id || '')) || [],
    ]),
  }));
  if (input.task) {
    documents.unshift({
      id: `task:${input.task.id || 'research'}`,
      sourceId: '',
      title: String(input.task.title || input.task.prompt || 'Research'),
      score: 100,
      source: null,
      evidence: [],
      text: collectText([input.task.title, input.task.prompt, input.task.criteria, input.task.knowledge_domain]),
    });
  }
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

function addRequestedLayers(nodes, links, documents, layer = 'concepts') {
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
  for (const document of documents.filter((item) => item.sourceId)) {
    const sourceNodeId = `source:${document.sourceId}`;
    extraNodes.push({
      id: sourceNodeId,
      label: shortLabel(document.title, 42),
      kind: 'source',
      occurrences: 1,
      documentCount: 1,
      rawScore: 3 + document.score / 20,
      sourceIds: [document.sourceId],
    });
    for (const concept of (bySource.get(document.sourceId) || []).slice(0, 6)) {
      extraLinks.push({
        id: `edge:${stableHash(`${sourceNodeId}\u0000${concept.id}`).toString(36)}`,
        source: sourceNodeId,
        target: concept.id,
        weight: 2,
        sourceIds: [document.sourceId],
      });
    }
    if (layer === 'evidence') {
      for (const [index, evidence] of document.evidence.slice(0, 4).entries()) {
        const evidenceId = `evidence:${document.sourceId}:${index}`;
        extraNodes.push({
          id: evidenceId,
          label: shortLabel(firstString(evidence, ['fact_label', 'criterion_id', 'title', 'name']) || `Evidence ${index + 1}`, 36),
          kind: 'evidence',
          occurrences: 1,
          documentCount: 1,
          rawScore: 2,
          sourceIds: [document.sourceId],
        });
        extraLinks.push({
          id: `edge:${stableHash(`${sourceNodeId}\u0000${evidenceId}`).toString(36)}`,
          source: sourceNodeId,
          target: evidenceId,
          weight: 1.5,
          sourceIds: [document.sourceId],
        });
      }
    }
  }
  return {
    nodes: [...nodes, ...extraNodes].slice(0, MAX_GRAPH_NODES),
    links: [...links, ...extraLinks].slice(0, MAX_GRAPH_LINKS),
  };
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
    node.visualSize = 2.2 + Math.pow(Math.max(0.015, node.importance), 0.72) * 8.4;
    node.labelSize = node.primary
      ? 4.6 + Math.pow(Math.max(0.02, node.importance), 0.7) * 4.4
      : 3.2 + Math.pow(Math.max(0.02, node.importance), 0.8) * 2.4;
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
  for (let pass = 0; pass < 20; pass += 1) {
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
  const sampleCount = Math.min(32, ordered.length);
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
      label: clusterNodes.sort((left, right) => right.importance - left.importance)[0]?.label || `Cluster ${cluster + 1}`,
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
    .filter((token) => token.normalized.length >= 3 && !STOP_WORDS.has(token.normalized) && !/^\d+$/.test(token.normalized));
}

function collectText(value, depth = 0) {
  if (depth > 4 || value === null || value === undefined) return '';
  if (typeof value === 'string' || typeof value === 'number') return String(value);
  if (Array.isArray(value)) return value.slice(0, 80).map((item) => collectText(item, depth + 1)).join(' ');
  if (typeof value === 'object') {
    return Object.entries(value).slice(0, 80).flatMap(([key, item]) => [key, collectText(item, depth + 1)]).join(' ');
  }
  return '';
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

export const __researchGraphDataTestHooks = {
  approximateBetweenness,
  collectText,
  louvainFirstPhase,
  projectionFromPersisted,
  projectionFromResearchRows,
  tokenize,
};
