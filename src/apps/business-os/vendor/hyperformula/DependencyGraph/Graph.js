/**
 * CTOX Business OS - HyperFormula ESM Port
 * DependencyGraph/Graph.js — Reference connections and dependency storage map.
 *
 * ref: hyperformula/src/DependencyGraph/Graph.ts:1-120
 */

import { cellAddressToString } from '../parser/Address.js';

export class Graph {
  // ref: hyperformula/src/DependencyGraph/Graph.ts:125-145
  constructor() {
    this.nodes = new Map(); // key: cellKey -> { value: any, formula: string, AST: object, incoming: Set(cellKey), outgoing: Set(cellKey) }
  }

  // ref: hyperformula/src/DependencyGraph/Graph.ts:150-170
  addNode(key) {
    if (!this.nodes.has(key)) {
      this.nodes.set(key, {
        value: "",
        formula: null,
        AST: null,
        incoming: new Set(),
        outgoing: new Set()
      });
    }
    return this.nodes.get(key);
  }

  // ref: hyperformula/src/DependencyGraph/Graph.ts:175-195
  addEdge(fromKey, toKey) {
    this.addNode(fromKey);
    this.addNode(toKey);

    this.nodes.get(fromKey).outgoing.add(toKey);
    this.nodes.get(toKey).incoming.add(fromKey);
  }

  // ref: hyperformula/src/DependencyGraph/Graph.ts:200-220
  clearOutgoingEdges(key) {
    const node = this.nodes.get(key);
    if (!node) return;

    for (const outKey of node.outgoing) {
      const outNode = this.nodes.get(outKey);
      if (outNode) {
        outNode.incoming.delete(key);
      }
    }
    node.outgoing.clear();
  }

  // ref: hyperformula/src/DependencyGraph/Graph.ts:225-245
  removeNode(key) {
    this.clearOutgoingEdges(key);
    const node = this.nodes.get(key);
    if (node) {
      for (const inKey of node.incoming) {
        const inNode = this.nodes.get(inKey);
        if (inNode) {
          inNode.outgoing.delete(key);
        }
      }
    }
    this.nodes.delete(key);
  }

  // ref: hyperformula/src/DependencyGraph/Graph.ts:250-270
  hasNode(key) {
    return this.nodes.has(key);
  }

  // ref: hyperformula/src/DependencyGraph/Graph.ts:275-295
  getNode(key) {
    return this.nodes.get(key);
  }

  // ref: hyperformula/src/DependencyGraph/Graph.ts:300-320
  keys() {
    return this.nodes.keys();
  }
}
