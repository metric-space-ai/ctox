/** Pure existing crew appearance. Callers own identity, task state and telemetry.
 * animationKey controls motion only: it is not a worker identity or an authority token.
 * No DOM access, chat imports, timer, random identity or persistence is created here.
 */

const CREW_SHAPES = Object.freeze(['round', 'blob', 'square', 'triangle']);

const NEUTRAL_CREW_IDENTITY = Object.freeze({ name: 'Crew', color: '#7d7f84', shape: 'round' });

function crewHash(value) {
  let hash = 2166136261;
  const input = String(value || 'ctox-crew');
  for (let index = 0; index < input.length; index += 1) {
    hash ^= input.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

export function normalizeCrewAppearance(explicit) {
  if (explicit && typeof explicit === 'object' && String(explicit.name || '').trim()) {
    return {
      name: String(explicit.name).trim(),
      color: /^#[0-9a-f]{6}$/i.test(String(explicit.color || '')) ? String(explicit.color) : NEUTRAL_CREW_IDENTITY.color,
      shape: CREW_SHAPES.includes(explicit.shape) ? explicit.shape : NEUTRAL_CREW_IDENTITY.shape,
    };
  }
  return { ...NEUTRAL_CREW_IDENTITY };
}

function crewBodyMarkup(shape) {
  if (shape === 'blob') {
    return '<path d="M13 34c0-8 5-13 12-14 2-8 8-12 16-10 7 1 11 6 12 13 6 2 10 7 9 14-1 8-7 12-15 12-4 6-12 7-18 2-9 1-16-7-16-17Z" />';
  }
  if (shape === 'square') {
    return '<path d="M12 14c5-5 35-6 40 0 5 6 5 33 0 38-6 5-34 5-40 0-5-5-5-32 0-38Z" />';
  }
  if (shape === 'triangle') {
    return '<path d="M27 9c3-5 8-5 11 0l22 38c3 6 0 10-7 10H11c-7 0-10-5-7-11L27 9Z" />';
  }
  return '<path d="M32 7c15 0 26 10 26 25S48 58 32 58 7 48 7 32 17 7 32 7Z" />';
}

function crewEyesMarkupForMode(shape, mode = 'working') {
  const y = shape === 'triangle' ? 36 : 30;
  if (mode === 'failed') {
    return `
      <g class="ctox-crew-eyes-x">
        <path d="M21 ${y - 5}l10 10M31 ${y - 5}L21 ${y + 5}" />
        <path d="M36 ${y - 5}l10 10M46 ${y - 5}L36 ${y + 5}" />
      </g>
    `;
  }
  if (mode === 'sleeping') {
    return `
      <g class="ctox-crew-eyes-sleeping">
        <path d="M21 ${y + 2}q5 5 10 0" />
        <path d="M36 ${y + 2}q5 5 10 0" />
      </g>
    `;
  }
  if (mode === 'review') {
    // Scrutinising: the eyes narrow into peering arcs, unlike the open
    // working eyes and the closed sleeping ones (Review-Befund B5).
    return `
      <g class="ctox-crew-eyes-review">
        <path d="M21 ${y + 2}q5 -7 10 0" />
        <path d="M36 ${y + 2}q5 -7 10 0" />
      </g>
    `;
  }
  if (mode === 'reading') {
    // Lowered gaze: the eyes sit on a page and scan it (CSS moves them).
    return `
      <g class="ctox-crew-eyes-reading">
        <path d="M22 ${y + 2}h8" />
        <path d="M37 ${y + 2}h8" />
      </g>
    `;
  }
  if (mode === 'learning') {
    // Wide, lifted eyes: something just clicked.
    return `
      <g class="ctox-crew-eyes-learning">
        <circle cx="26" cy="${y - 1}" r="3.2" />
        <circle cx="41" cy="${y - 2}" r="3.2" />
      </g>
    `;
  }
  return `
    <path d="M25 ${y - 3}l3 8" />
    <path d="M40 ${y - 4}l3 8" />
  `;
}

function crewMotionStyle(animationKey) {
  const hash = crewHash(animationKey);
  const tenths = (offset, min, span) => (min + ((hash >>> offset) % span) / 10).toFixed(2);
  const delay = (offset, duration) => (-((hash >>> offset) % 1000) / 1000 * duration).toFixed(2);
  const workDrift = tenths(3, 3.7, 21);
  const workBody = tenths(11, 2.3, 27);
  const workEyes = tenths(19, 4.1, 31);
  const reviewDrift = tenths(7, 4.4, 25);
  const reviewBody = tenths(15, 2.7, 29);
  const reviewEyes = tenths(23, 5.2, 37);
  return [
    `--crew-work-drift:${workDrift}s`,
    `--crew-work-body:${workBody}s`,
    `--crew-work-eyes:${workEyes}s`,
    `--crew-review-drift:${reviewDrift}s`,
    `--crew-review-body:${reviewBody}s`,
    `--crew-review-eyes:${reviewEyes}s`,
    `--crew-work-delay:${delay(5, Number(workDrift))}s`,
    `--crew-work-body-delay:${delay(13, Number(workBody))}s`,
    `--crew-work-eyes-delay:${delay(21, Number(workEyes))}s`,
    `--crew-review-delay:${delay(9, Number(reviewDrift))}s`,
    `--crew-review-body-delay:${delay(17, Number(reviewBody))}s`,
    `--crew-review-eyes-delay:${delay(25, Number(reviewEyes))}s`,
  ].join(';');
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (char) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;',
  }[char]));
}

function escapeAttr(value) {
  return escapeHtml(value).replace(/`/g, '&#96;');
}

export function renderCrewCreature({
  appearance, animationKey, taskState, mode, placement = 'dock',
  progressPercent = 0, activity = { total: 0, lastKind: '', updatedAt: 0 },
}) {
  const crew = normalizeCrewAppearance(appearance);
  const progressAngle = Math.max(0, Math.min(360, Number(progressPercent || 0) * 3.6));
  const telemetry = activity;
  const motionSeed = crewHash(`${animationKey}:${placement}`);
  return `
    <span class="ctox-crew-creature is-${escapeAttr(taskState)} is-${escapeAttr(mode)} is-${escapeAttr(crew.shape)} is-${escapeAttr(placement)}" data-crew-mode="${escapeAttr(mode)}" data-crew-identity="${escapeAttr(JSON.stringify(crew))}" data-crew-seed="${motionSeed}" data-crew-key="${escapeAttr(`${animationKey}:${placement}`)}" data-activity-turns="${escapeAttr(telemetry.total)}" data-activity-kind="${escapeAttr(telemetry.lastKind)}" data-activity-updated-at="${escapeAttr(telemetry.updatedAt)}" style="--crew-color:${escapeAttr(crew.color)};--ctox-progress-angle:${progressAngle}deg;${crewMotionStyle(animationKey)}" aria-hidden="true">
      <svg viewBox="0 0 64 64" focusable="false">
        <g class="ctox-crew-body">${crewBodyMarkup(crew.shape)}</g>
        <g class="ctox-crew-eyes is-${escapeAttr(mode)}">${crewEyesMarkupForMode(crew.shape, mode)}</g>
      </svg>
    </span>
  `;
}

/** Exact base rules from the existing chat; its adapter preserves their cascade position. */
export const CREW_CREATURE_BASE_CSS = `
    .ctox-crew-creature {
      display: inline-grid;
      place-items: center;
      width: 100%;
      height: 100%;
      transform-origin: 50% 78%;
      will-change: transform;
      contain: layout style;
    }
    .ctox-crew-creature svg {
      display: block;
      width: 100%;
      height: 100%;
      overflow: visible;
    }
    .ctox-crew-body {
      fill: var(--crew-color);
      filter: drop-shadow(0 3px 5px color-mix(in srgb, var(--crew-color) 32%, transparent));
      transform-box: fill-box;
      transform-origin: center;
    }
    .ctox-crew-eyes {
      fill: none;
      stroke: #090a0c;
      stroke-width: 5;
      stroke-linecap: round;
      transform-box: fill-box;
      transform-origin: center;
    }
    .ctox-crew-eyes-x,
    .ctox-crew-eyes-sleeping {
      fill: none;
      stroke: #090a0c;
      stroke-width: 5;
      stroke-linecap: round;
    }
    .ctox-crew-creature.is-window {
      width: 38px;
      height: 38px;
      flex: 0 0 38px;
    }
    /* Resting, queued and scheduled crew members deliberately stay still. */
    .ctox-crew-creature.is-working,
    .ctox-chat-window.is-task-running:not(.is-task-review) .ctox-crew-creature {
      animation: none;
    }
    .ctox-crew-creature.is-working .ctox-crew-body,
    .ctox-chat-window.is-task-running:not(.is-task-review) .ctox-crew-body {
      animation: none;
    }
    .ctox-crew-creature.is-working .ctox-crew-eyes,
    .ctox-chat-window.is-task-running:not(.is-task-review) .ctox-crew-eyes {
      animation: none;
    }
    .ctox-crew-creature.is-review,
    .ctox-chat-window.is-task-review .ctox-crew-creature {
      animation: none;
    }
    .ctox-crew-creature.is-review .ctox-crew-body,
    .ctox-chat-window.is-task-review .ctox-crew-body {
      animation: none;
    }
    .ctox-crew-creature.is-review .ctox-crew-eyes,
    .ctox-chat-window.is-task-review .ctox-crew-eyes {
      animation: none;
    }
    .ctox-crew-creature.is-failed,
    .ctox-chat-window.is-task-failed .ctox-crew-creature {
      animation: ctoxCrewOops 860ms cubic-bezier(.22,.75,.35,1) 1 both;
    }
    /* Reading: the body leans in a little, the lowered eyes scan the page. */
    .ctox-chat-crew-slot { touch-action: none; }
    .ctox-crew-eyes-review,
    .ctox-crew-eyes-reading,
    .ctox-crew-eyes-learning {
      fill: none;
      stroke: #090a0c;
      stroke-width: 5;
      stroke-linecap: round;
    }
    .ctox-crew-eyes-learning circle {
      fill: #090a0c;
      stroke: none;
    }
    .ctox-crew-creature.is-reading .ctox-crew-body {
      animation: ctoxCrewReadLean 3.2s ease-in-out infinite;
    }
    .ctox-crew-creature.is-reading .ctox-crew-eyes {
      animation: ctoxCrewReadScan 2.4s ease-in-out infinite;
    }
    /* Learning: a slow, content nod with a soft glow of the body. */
    .ctox-crew-creature.is-learning {
      animation: ctoxCrewLearnNod 2.6s ease-in-out infinite;
    }
    .ctox-crew-creature.is-learning .ctox-crew-body {
      animation: ctoxCrewLearnGlow 2.6s ease-in-out infinite;
    }
    @keyframes ctoxCrewReadLean {
      0%, 100% { transform: scale(1, 1) rotate(0); }
      50% { transform: scale(1.02, .985) rotate(2deg); }
    }
    @keyframes ctoxCrewReadScan {
      0%, 100% { transform: translateX(-2px); }
      45% { transform: translateX(2.5px); }
      55% { transform: translateX(2.5px); }
    }
    @keyframes ctoxCrewLearnNod {
      0%, 100% { transform: translateY(0) rotate(0); }
      35% { transform: translateY(-1.5px) rotate(-2deg); }
      70% { transform: translateY(1px) rotate(1.5deg); }
    }
    @keyframes ctoxCrewLearnGlow {
      0%, 100% { filter: drop-shadow(0 3px 5px color-mix(in srgb, var(--crew-color) 32%, transparent)); }
      50% { filter: drop-shadow(0 0 9px color-mix(in srgb, var(--crew-color) 78%, transparent)); }
    }
    @keyframes ctoxCrewWorkDrift {
      0%, 19%, 100% { transform: translate3d(0, 0, 0) rotate(-1.5deg); }
      33% { transform: translate3d(0, -2px, 0) rotate(1deg); }
      57% { transform: translate3d(1px, 1px, 0) rotate(2.5deg); }
      78% { transform: translate3d(-1px, -1px, 0) rotate(-2deg); }
    }
    @keyframes ctoxCrewWorkBody {
      0%, 100% { transform: scale(1, 1); }
      27% { transform: scale(1.035, .97) skewX(-1deg); }
      52% { transform: scale(.975, 1.025) skewX(1.5deg); }
      81% { transform: scale(1.018, .988) skewX(-.5deg); }
    }
    @keyframes ctoxCrewWorkEyes {
      0%, 23%, 100% { transform: translateX(0) rotate(0); }
      41% { transform: translateX(2px) rotate(-2deg); }
      63% { transform: translateX(-1px) rotate(1deg); }
      86% { transform: translateX(1px) rotate(-1deg); }
    }
    @keyframes ctoxCrewReviewDrift {
      0%, 100% { transform: translate3d(0, 0, 0) rotate(2deg); }
      24% { transform: translate3d(-1px, -1px, 0) rotate(-2deg); }
      49% { transform: translate3d(1px, 1px, 0) rotate(3deg); }
      73% { transform: translate3d(-1px, 0, 0) rotate(-1deg); }
    }
    @keyframes ctoxCrewReviewBody {
      0%, 100% { transform: scale(1) rotate(0); }
      18% { transform: scale(.965, 1.03) rotate(-2deg); }
      46% { transform: scale(1.025, .975) rotate(1.5deg); }
      68% { transform: scale(.985, 1.015) rotate(3deg); }
      88% { transform: scale(1.012, .99) rotate(-1deg); }
    }
    @keyframes ctoxCrewReviewEyes {
      0%, 100% { transform: translateX(-1px) rotate(2deg); }
      31% { transform: translateX(3px) rotate(-3deg); }
      59% { transform: translateX(-3px) rotate(2deg); }
      82% { transform: translateX(1px) rotate(-1deg); }
    }
    @keyframes ctoxCrewOops {
      0%, 68%, 100% { transform: translate3d(0, 0, 0) rotate(0); }
      76% { transform: translate3d(-2px, 0, 0) rotate(-3deg); }
      84% { transform: translate3d(2px, 0, 0) rotate(3deg); }
      92% { transform: translate3d(-1px, 0, 0) rotate(-1deg); }
    }`;

/** Standalone hosts also get the existing reduced-motion behavior. */
export const CREW_CREATURE_CSS = CREW_CREATURE_BASE_CSS + `
@media (prefers-reduced-motion: reduce) {
  .ctox-crew-creature, .ctox-crew-creature * {
    animation: none !important;
    transition: none !important;
  }
}
`;
