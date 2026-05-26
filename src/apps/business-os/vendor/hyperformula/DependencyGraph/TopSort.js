/**
 * CTOX Business OS - HyperFormula ESM Port
 * DependencyGraph/TopSort.js — Kahn's cycle-detecting topological sort algorithm.
 *
 * ref: hyperformula/src/DependencyGraph/TopSort.ts:1-100
 */

export function topologicalSort(graph) {
  // ref: hyperformula/src/DependencyGraph/TopSort.ts:105-125
  const inDegree = new Map();
  const order = [];
  const queue = [];

  // Initialize in-degrees for all registered nodes
  for (const nodeKey of graph.keys()) {
    const node = graph.getNode(nodeKey);
    inDegree.set(nodeKey, node.incoming.size);
    if (node.incoming.size === 0) {
      queue.push(nodeKey);
    }
  }

  // ref: hyperformula/src/DependencyGraph/TopSort.ts:130-155
  while (queue.length > 0) {
    const key = queue.shift();
    order.push(key);

    const node = graph.getNode(key);
    if (node) {
      for (const neighbor of node.outgoing) {
        const currentInDegree = inDegree.get(neighbor) - 1;
        inDegree.set(neighbor, currentInDegree);
        if (currentInDegree === 0) {
          queue.push(neighbor);
        }
      }
    }
  }

  // ref: hyperformula/src/DependencyGraph/TopSort.ts:160-180
  const cycles = [];
  for (const [key, degree] of inDegree.entries()) {
    if (degree > 0) {
      cycles.push(key);
    }
  }

  return {
    order,
    hasCycles: cycles.length > 0,
    cycles
  };
}
