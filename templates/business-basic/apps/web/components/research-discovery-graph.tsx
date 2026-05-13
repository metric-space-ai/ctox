"use client";

import { useMemo } from "react";
import type { ResearchGraphEdge, ResearchGraphNode } from "../lib/marketing-seed";

type PositionedNode = ResearchGraphNode & {
  x: number;
  y: number;
};

const WIDTH = 760;
const HEIGHT = 360;

export function ResearchDiscoveryGraph({
  edges,
  nodes
}: {
  edges: ResearchGraphEdge[];
  nodes: ResearchGraphNode[];
}) {
  const positioned = useMemo(() => layoutNodes(nodes), [nodes]);
  const byId = new Map(positioned.map((node) => [node.id, node]));

  return (
    <svg aria-label="Research discovery graph" className="research-graph-svg" role="img" viewBox={`0 0 ${WIDTH} ${HEIGHT}`}>
      <g className="research-graph-edges">
        {edges.map((edge, index) => {
          const source = byId.get(edge.source);
          const target = byId.get(edge.target);
          if (!source || !target) return null;
          return (
            <line key={`${edge.source}-${edge.target}-${index}`} x1={source.x} x2={target.x} y1={source.y} y2={target.y}>
              <title>{edge.relation}</title>
            </line>
          );
        })}
      </g>
      <g className="research-graph-nodes">
        {positioned.map((node) => (
          <g className={`research-graph-node research-graph-node-${node.kind} score-${node.score?.toLowerCase() ?? "none"}`} key={node.id} transform={`translate(${node.x} ${node.y})`}>
            <circle r={node.kind === "group" ? 19 : 15} />
            <text dy={node.kind === "group" ? 36 : 31}>{node.label}</text>
            <title>{node.label}</title>
          </g>
        ))}
      </g>
    </svg>
  );
}

function layoutNodes(nodes: ResearchGraphNode[]): PositionedNode[] {
  const groups = {
    query: nodes.filter((node) => node.kind === "query"),
    group: nodes.filter((node) => node.kind === "group"),
    source: nodes.filter((node) => node.kind === "source")
  };

  return [
    ...placeColumn(groups.query, 92),
    ...placeColumn(groups.group, 380),
    ...placeColumn(groups.source, 650)
  ];
}

function placeColumn(nodes: ResearchGraphNode[], x: number): PositionedNode[] {
  const step = HEIGHT / (nodes.length + 1);
  return nodes.map((node, index) => ({ ...node, x, y: Math.round(step * (index + 1)) }));
}
