// ui/candidateBoard.js — pure candidate-stage Kanban renderer for the matching
// pipeline (PIPELINE-1). Renders the structured stage columns from the tested
// pipeline core and supports drag-to-move-stage. No RxDB, no native command —
// persistence is the caller's job (matching writes RxDB directly via
// updateMatchState; another host can wire onStageChange to a command).
//
// Baukasten: a generic stage-board over groupByCandidateStage; the stage set is
// the recruiting profile from core/pipeline.js.

import { CANDIDATE_STAGES, groupByCandidateStage, normalizeCandidateStage } from '../core/pipeline.js';

function escapeHtml(value) {
  return String(value == null ? '' : value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/**
 * Render the candidate Kanban into `host` and wire drag-to-move-stage.
 * @param {HTMLElement} host
 * @param {Array<object>} matches each carries an id + label + data.pipeline.stage
 * @param {{ onStageChange?: (matchId: string, toStage: string) => void, label?: (m: object) => string }} [opts]
 * @returns {() => void} teardown
 */
export function renderCandidateBoard(host, matches, opts = {}) {
  if (!host) return () => {};
  const label = typeof opts.label === 'function' ? opts.label : (m) => m.title || m.objectId || m.id || '';
  const byId = new Map((matches || []).map((m) => [String(m.id ?? m.objectId ?? ''), m]));
  const groups = groupByCandidateStage(matches || []);

  host.innerHTML = `<div class="candidate-board" data-candidate-board>${groups
    .map(
      (g) => `<section class="cb-col${g.terminal ? ' cb-terminal' : ''}" data-stage="${g.key}">
        <header class="cb-col-head">${escapeHtml(g.label)} <span class="cb-count">${g.items.length}</span></header>
        <div class="cb-col-body" data-drop-stage="${g.key}">
          ${g.items
            .map((m) => {
              const id = String(m.id ?? m.objectId ?? '');
              return `<article class="cb-card" draggable="true" data-card-id="${escapeHtml(id)}">${escapeHtml(label(m))}</article>`;
            })
            .join('')}
        </div>
      </section>`,
    )
    .join('')}</div>`;

  let draggingId = null;
  const onDragStart = (e) => {
    const card = e.target.closest('[data-card-id]');
    if (!card) return;
    draggingId = card.dataset.cardId;
    try {
      e.dataTransfer.setData('text/plain', draggingId);
      e.dataTransfer.effectAllowed = 'move';
    } catch {
      /* setData may be unavailable in some environments */
    }
  };
  const onDragOver = (e) => {
    if (e.target.closest('[data-drop-stage]')) e.preventDefault();
  };
  const onDrop = (e) => {
    const zone = e.target.closest('[data-drop-stage]');
    if (!zone) return;
    e.preventDefault();
    const id = draggingId || (() => {
      try {
        return e.dataTransfer.getData('text/plain');
      } catch {
        return null;
      }
    })();
    draggingId = null;
    if (!id) return;
    const toStage = zone.dataset.dropStage;
    const match = byId.get(String(id));
    const fromStage = match ? normalizeCandidateStage(match) : null;
    if (toStage && toStage !== fromStage) {
      opts.onStageChange?.(String(id), toStage);
    }
  };

  const board = host.querySelector('[data-candidate-board]');
  board?.addEventListener('dragstart', onDragStart);
  board?.addEventListener('dragover', onDragOver);
  board?.addEventListener('drop', onDrop);

  return () => {
    board?.removeEventListener('dragstart', onDragStart);
    board?.removeEventListener('dragover', onDragOver);
    board?.removeEventListener('drop', onDrop);
  };
}

export { CANDIDATE_STAGES };
