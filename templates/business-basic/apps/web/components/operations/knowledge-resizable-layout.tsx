"use client";

import { Children, useMemo, useState } from "react";
import type { ReactNode } from "react";

const minColumns = [280, 320, 560];

export function KnowledgeResizableLayout({ children }: { children: ReactNode }) {
  const panes = Children.toArray(children);
  const [columns, setColumns] = useState([0.23, 0.29, 0.48]);
  const template = useMemo(
    () => `minmax(${minColumns[0]}px, ${columns[0]}fr) 8px minmax(${minColumns[1]}px, ${columns[1]}fr) 8px minmax(${minColumns[2]}px, ${columns[2]}fr)`,
    [columns]
  );

  const startResize = (divider: 0 | 1, event: React.PointerEvent<HTMLButtonElement>) => {
    const container = event.currentTarget.parentElement;
    if (!container) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    const startX = event.clientX;
    const startColumns = [...columns];
    const rect = container.getBoundingClientRect();
    const total = rect.width;

    const move = (moveEvent: PointerEvent) => {
      const delta = (moveEvent.clientX - startX) / total;
      setColumns(() => {
        const next = [...startColumns];
        const left = divider;
        const right = divider + 1;
        const leftNext = Math.max(0.18, startColumns[left] + delta);
        const rightNext = Math.max(0.22, startColumns[right] - delta);
        const spill = leftNext + rightNext - (startColumns[left] + startColumns[right]);
        next[left] = leftNext - Math.max(0, spill);
        next[right] = rightNext;
        return next;
      });
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
  };

  return (
    <div className="ops-workspace ops-knowledge-workspace ctox-knowledge-os" style={{ gridTemplateColumns: template }}>
      {panes[0]}
      <button aria-label="Resize skills and skillbooks" className="ctox-pane-resizer" onPointerDown={(event) => startResize(0, event)} type="button" />
      {panes[1]}
      <button aria-label="Resize skillbooks and runbooks" className="ctox-pane-resizer" onPointerDown={(event) => startResize(1, event)} type="button" />
      {panes[2]}
    </div>
  );
}
