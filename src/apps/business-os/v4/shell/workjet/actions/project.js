// Shell v4 – Workjet-Brücke, Aktionsgruppe A (Projekt, KPI, Exit, Präsentation). Eigentum Strang A.
// WJB-0 hat hier project.list (Lesepfad) und project.worker.add (Befehlspfad) als Beleg portiert; der Strang ergänzt
// die übrigen Aktionen der Gruppe nach demselben Muster (siehe ../PORT-PLAN.md).
//
// Muster: jede Aktion ist `async (env, request, diagnostics) => response`. `env` liefert dieselben Abhängigkeiten wie
// die V2-Shell (env.state, env.actorContext, env.newId), in Produktion aus v2-compat.js, im Orakel-Test aus dem
// V2-Testaufbau. Der Rumpf bleibt wörtlich V2 (ref-Anker), nur `state.` → `env.state.`.
import {
  WORKJET_PROJECT_CONTROL_MAX_RESULTS, WORKJET_PROJECT_CONTROL_MAX_WORKING_COPIES, WORKJET_PROJECT_CONTROL_TIMEOUT_MS,
  boundedWorkjetProjectText, assertWorkjetProjectPayloadKeys, boundedWorkjetProjectIsoDate,
  awaitWorkjetProjectListStep, readWorkjetProjectListRows,
  listProjectedWorkjetProjects, listProjectedWorkjetWorkingCopies,
  // Strang A
  boundedWorkjetProjectMetadata, boundedWorkjetProjectResult, boundedWorkjetWorkingCopyResult, publicWorkjetWorkingCopyResult,
} from '../v2-helpers.js';
import { unsupportedError } from '../v2-compat.js'; // Strang A
// Strang A: V2-Verträge unverändert aus shared/ (byte-gleich zum V2-Pin e35a116ae, im Orakel geprüft).
import { PROJECT_KPIS_SCHEMA, validateProjectKpiValue } from '../../../../shared/workjet-project-kpis-contract.generated.mjs';
import { PRESENTATION_SCHEMA } from '../../../../shared/workjet-presentation-contract.generated.mjs';
import {
  PRESENTATION_ACTIONS, PRESENTATION_READ_ACTION, PRESENTATION_CANVAS_SAVE_ACTION, presentationReadPayload,
  presentationCanvasSavePayload, presentationFromReadResult, presentationMutationFromResult, readJourFixePresentationContent,
} from '../../../../shared/jour-fixe-presentation.mjs';

// ref: app.js:15010-15023
async function requireWorkjetProjectDataPlane(env) {
  const { state } = env;
  if (!state.commandBus?.dispatch || !state.db?.collection?.('business_commands')) { // v4-allow: debt L-42
    throw new Error('Workjet project control is not ready.'); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  }
  const collection = state.db?.collection?.('workjet_projects');
  if (!collection) throw new Error('workjet_projects collection is not registered.');
  // Command submission owns authenticated readiness, not a historical pull.
  const [, projectBridge, workingCopyBridge] = await Promise.all([
    state.sync?.startCollection?.('business_commands'), // v4-allow: debt L-42
    state.sync?.startCollection?.('workjet_projects', { pin: false, forceDirect: true }), // v4-allow: debt L-42
    state.sync?.startCollection?.('workjet_working_copies', { pin: false, forceDirect: true }), // v4-allow: debt L-42
  ]);
  return { projectBridge, workingCopyBridge };
}

// ref: app.js:13851-13904. V2 misst zusätzlich Befehlslaufzeiten über das Command-Bus-Modul (diagnostics.onCommandTiming);
// der v4-Kernel hat keine Laufzeitmarken. Die Diagnose bleibt ohne Wirkung (Workjet sendet dann kein commandTiming).
async function dispatchWorkjetProjectList(env, command, options, deadline) {
  return awaitWorkjetProjectListStep(env.state.commandBus.dispatch(command, options), deadline, 'command', clock(env)); // v4-allow: debt L-42
}

// Uhr der Umgebung (V2-Tests setzen Date im vm-Kontext; Produktion: Date.now).
const clock = (env) => (typeof env.now === 'function' ? env.now : Date.now);

// Gemeinsamer Vorspann aus workjetProjectControl (ref: app.js:13906-13915).
function scope(env) {
  const { state } = env;
  return {
    ownerUserId: boundedWorkjetProjectText(env.actorContext(state.session).id, 'owner_user_id', 256),
    requestSession: state.session,
    requestDb: state.db,
  };
}

// ref: app.js:14047-14060 (Datenebene) + 14713-14846 (project.list)
async function projectList(env, request, diagnostics) { // eslint-disable-line no-unused-vars
  const { state, actorContext } = env;
  const newId = env.newId;
  const { ownerUserId, requestSession, requestDb } = scope(env);
  // Reserve a delivery margin inside Workjet's 30-second desktop call.
  const now = clock(env);
  const listDeadline = now() + WORKJET_PROJECT_CONTROL_TIMEOUT_MS - 1_000;
  const { projectBridge, workingCopyBridge } = await awaitWorkjetProjectListStep(
    requireWorkjetProjectDataPlane(env), listDeadline, 'collections', now,
  );
  assertWorkjetProjectPayloadKeys(request, new Set(['action', 'includeConfiguration', 'includeSupervisorLuma', 'includeExecutionPolicy']));
  for (const flag of ['includeConfiguration', 'includeSupervisorLuma', 'includeExecutionPolicy']) {
    if (Object.hasOwn(request, flag) && typeof request[flag] !== 'boolean') {
      throw new Error(`Invalid Workjet project ${flag}.`);
    }
  }
  const includeConfiguration = request.includeConfiguration === true;
  const includeSupervisorLuma = request.includeSupervisorLuma === true;
  const includeExecutionPolicy = request.includeExecutionPolicy === true;
  const commandId = `cmd_workjet_project_list_${newId()}`;
  const assertCurrentIdentity = () => {
    if (state.session !== requestSession || state.db !== requestDb
      || actorContext(state.session).id !== ownerUserId) {
      throw new Error('Workjet project session changed before the list was delivered.');
    }
  };
  assertCurrentIdentity();
  const receipt = await dispatchWorkjetProjectList(env, {
    id: commandId,
    command_id: commandId,
    module: 'ctox',
    command_type: 'ctox.workjet.project.list',
    record_id: ownerUserId,
    payload: { limit: WORKJET_PROJECT_CONTROL_MAX_RESULTS },
    client_context: {
      source: 'workjet-project-control',
      actor: actorContext(state.session),
    },
  }, {
    until: 'terminal', // v4-allow: debt L-42
    sync_queue_tasks: false,
    timeoutMs: Math.max(1, listDeadline - now()),
  }, listDeadline);
  assertCurrentIdentity();
  if (receipt?.command_id !== commandId || receipt?.status !== 'completed'
    || receipt?.ok !== true || receipt?.result?.ok !== true
    || receipt?.result?.collection !== 'workjet_projects') {
    throw new Error('Workjet project list returned an uncorrelated or unsuccessful receipt.');
  }
  const count = receipt.result.count;
  if (!Number.isSafeInteger(count) || count < 0 || count > WORKJET_PROJECT_CONTROL_MAX_RESULTS
    || typeof receipt.result.truncated !== 'boolean') {
    throw Object.assign(new Error('Workjet could not confirm the complete project list.'), {
      code: 'WORKJET_PROJECT_LIST_UNCONFIRMED', retryable: false,
    });
  }
  if (receipt.result.truncated) {
    throw Object.assign(new Error('Workjet project list exceeds the supported complete window.'), {
      code: 'WORKJET_PROJECT_LIST_INCOMPLETE', retryable: false,
    });
  }
  // The confirmed owner can differ from the authenticated actor for a verified alias.
  // Keep the original actor/session fence; only the native receipt selects the read scope.
  let projectOwnerUserId = ownerUserId;
  if (Object.hasOwn(receipt.result, 'owner_user_id')) {
    const confirmedOwner = receipt.result.owner_user_id;
    if (typeof confirmedOwner !== 'string' || !confirmedOwner || confirmedOwner.length > 256
      || confirmedOwner.trim() !== confirmedOwner || /[\u0000-\u001f\u007f]/.test(confirmedOwner)) {
      throw Object.assign(new Error('Workjet could not confirm the project owner identity.'), {
        code: 'WORKJET_PROJECT_LIST_UNCONFIRMED', retryable: false,
      });
    }
    projectOwnerUserId = confirmedOwner;
  }
  let confirmedProjectIds = null;
  if (Object.hasOwn(receipt.result, 'project_ids')) {
    const ids = receipt.result.project_ids;
    if (!Array.isArray(ids) || ids.length !== count || new Set(ids).size !== count
      || ids.some((id) => typeof id !== 'string' || !id || id.length > 128
        || id.trim() !== id || /[\u0000-\u001f\u007f]/.test(id))) {
      throw Object.assign(new Error('Workjet could not confirm the active project identities.'), {
        code: 'WORKJET_PROJECT_LIST_UNCONFIRMED', retryable: false,
      });
    }
    confirmedProjectIds = new Set(ids);
  }
  const selector = { owner_user_id: { $eq: projectOwnerUserId } };
  const projectSelector = { ...selector, status: { $eq: 'active' } };
  const controller = new AbortController();
  const projectAuthority = {};
  try {
    let [projectDocs, copyDocs] = await Promise.all([
      readWorkjetProjectListRows(projectBridge, {
        selector: projectSelector, limit: WORKJET_PROJECT_CONTROL_MAX_RESULTS,
      }, commandId, listDeadline, controller.signal, projectAuthority, now),
      readWorkjetProjectListRows(workingCopyBridge, {
        selector, limit: WORKJET_PROJECT_CONTROL_MAX_WORKING_COPIES,
      }, commandId, listDeadline, controller.signal, {}, now),
    ]);
    assertCurrentIdentity();
    const workingCopies = await listProjectedWorkjetWorkingCopies(env, projectOwnerUserId, copyDocs);
    let projects = await listProjectedWorkjetProjects(
      env, projectOwnerUserId, WORKJET_PROJECT_CONTROL_MAX_RESULTS, workingCopies, projectDocs,
      { includeConfiguration, includeSupervisorLuma, includeExecutionPolicy },
    );
    assertCurrentIdentity();
    if (confirmedProjectIds) {
      const projectedIds = new Set(projects.map((project) => project.id));
      const missingIds = [...confirmedProjectIds].filter((id) => !projectedIds.has(id));
      if (missingIds.length) {
        const missingDocs = await readWorkjetProjectListRows(projectBridge, {
          selector: { ...projectSelector, id: { $in: missingIds } }, limit: missingIds.length,
        }, `${commandId}:missing`, listDeadline, controller.signal, projectAuthority, now);
        assertCurrentIdentity();
        if (missingDocs.some((doc) => !missingIds.includes((doc?.toJSON?.() || doc)?.id))) {
          throw new Error('Workjet project repair query returned an unrequested identity.');
        }
        projectDocs = projectDocs.concat(missingDocs);
        projects = await listProjectedWorkjetProjects(
          env, projectOwnerUserId, WORKJET_PROJECT_CONTROL_MAX_RESULTS, workingCopies, projectDocs,
          { includeConfiguration, includeSupervisorLuma, includeExecutionPolicy },
        );
      }
    }
    assertCurrentIdentity();
    if (projects.length !== count
      || (confirmedProjectIds && projects.some((project) => !confirmedProjectIds.has(project.id)))) {
      throw Object.assign(new Error('Workjet project projection does not match the confirmed active projects.'), {
        code: 'WORKJET_PROJECT_LIST_INCOMPLETE', retryable: true,
      });
    }
    if (includeConfiguration && receipt.result.exit_models) {
      projects = projects.map((project) => {
        const assessment = receipt.result.exit_models[project.id];
        if (!assessment || assessment.contract !== 'ctox.workjet.exit_model.v1' || assessment.project_id !== project.id) throw new Error('Exit metadata conflicts with its project.');
        return Object.freeze({ ...project, exitModel: assessment });
      });
    }
    return { action: 'project.list', projects, count, truncated: false };
  } finally {
    controller.abort();
  }
}

// ref: app.js:14056-14060 (Datenebene) + 14340-14382 (project.worker.add / project.chat.create).
// project.chat.create teilt den Rumpf; freigeschaltet wird es erst, wenn Strang A seinen Orakel-Test aufnimmt.
function projectChatMutation(creatingChat) {
  return async function projectChat(env, request) {
    const { state, actorContext } = env;
    const action = creatingChat ? 'project.chat.create' : 'project.worker.add';
    const { ownerUserId, requestSession, requestDb } = scope(env);
    await requireWorkjetProjectDataPlane(env);
    const allowedKeys = new Set(['action', 'commandId', 'projectId', 'workerProfileId', 'createdAt']);
    if (creatingChat) allowedKeys.add('title');
    assertWorkjetProjectPayloadKeys(request, allowedKeys);
    const commandId = boundedWorkjetProjectText(request.commandId, 'commandId', 128);
    const projectId = boundedWorkjetProjectText(request.projectId, 'projectId', 128);
    const workerProfileId = boundedWorkjetProjectText(request.workerProfileId, 'workerProfileId', 256);
    boundedWorkjetProjectIsoDate(request.createdAt, 'createdAt');
    const payload = { project_id: projectId, worker_profile_id: workerProfileId };
    if (creatingChat) payload.title = boundedWorkjetProjectText(request.title, 'title', 256);
    const assertCurrentSession = () => {
      if (state.session !== requestSession || state.db !== requestDb
        || actorContext(state.session).id !== ownerUserId) {
        throw new Error('Workjet project session changed before the command result was delivered.');
      }
    };
    assertCurrentSession();
    const receipt = await state.commandBus.dispatch({ // v4-allow: debt L-42
      id: commandId,
      command_id: commandId,
      module: 'ctox',
      command_type: creatingChat ? 'ctox.workjet.project.chat.create' : 'ctox.workjet.project.worker.add',
      record_id: projectId,
      payload,
      client_context: {
        source: 'workjet-project-control',
        actor: actorContext(requestSession),
      },
    }, { until: 'terminal', timeoutMs: WORKJET_PROJECT_CONTROL_TIMEOUT_MS }); // v4-allow: debt L-42
    assertCurrentSession();
    if (receipt?.command_id !== commandId || receipt.ok !== true || receipt.status !== 'completed'
      || receipt.target_record_id !== projectId
      || receipt.payload?.project_id !== projectId || receipt.payload?.worker_profile_id !== workerProfileId
      || (creatingChat && receipt.payload?.title !== payload.title)
      || receipt.result?.ok !== true || receipt.result?.contract !== 'workjet-project-chats.v1') {
      throw new Error('Workjet project command returned an uncorrelated or unsuccessful receipt.');
    }
    const chatId = creatingChat ? receipt.result.chat_id : receipt.result.first_chat_id;
    if (typeof chatId !== 'string' || !/^workjet_private_[A-Za-z0-9_-]+$/.test(chatId) || chatId.length > 256) {
      throw new Error('Workjet project command did not return a private chat id.');
    }
    return { action, commandId, projectId, workerProfileId, chatId };
  };
}

// ===================================================================================================================
// Strang A (WJB-A): create, init, configure, chat.create, KPI, Exit, Präsentation.
// Rumpf wörtlich V2 (Pin e35a116ae, /Volumes/tmp/app-main.js); Ersetzungen nur nach PORT-PLAN Regel 2
// (state → env.state bzw. destrukturiert wie im Vorbild oben, actorContext → env.actorContext, Date.now() → env.now(),
// window.setTimeout → setTimeout). Lokale V2-Hilfen unten gehören nach v2-helpers (Bericht WJB-A.md).
// ===================================================================================================================

// ---- lokale V2-Hilfen (gehört nach v2-helpers) ----------------------------------------------------------------

// ref: app.js:15002-15008
async function requireWorkjetSupervisorDataPlane(env) {
  const { state } = env;
  if (!state.commandBus?.dispatch || !state.db?.collection?.('business_commands')) { // v4-allow: debt L-42
    throw new Error('Workjet supervisor control is not ready.'); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  }
  await state.sync?.startCollection?.('business_commands'); // v4-allow: debt L-42
  return {};
}

// ref: app.js:15458-15476. Keine env-Abhängigkeit; die Orakel-Hülle (tests/oracle.test.mjs, Block Strang A) schneidet
// diese Funktion und die zwei folgenden als Quelltext aus dieser Datei (Signatur je auf EINER Zeile, schließende
// Klammer in Spalte 0 – nicht umformatieren).
async function waitForSyncBridgeReady(bridge, timeoutMs = 15000) {
  const state = bridge?.state;
  if (!state) return;
  let timer = null;
  try {
    await Promise.race([
      Promise.resolve()
        .then(() => state.awaitInSync?.() || state.awaitInitialReplication?.())
        .catch(() => {}),
      new Promise((resolve) => {
        timer = setTimeout(resolve, timeoutMs);
        timer?.unref?.();
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

// ref: app.js:15120-15154. Wartet auf die Projektion in workjet_projects (Kern-Export, K-WJ-1).
async function waitForProjectedWorkjetProject(env, projectId, expectedTitle, ownerUserId, bridge, timeoutMs, workingCopies = []) {
  const collection = env.state.db?.collection?.('workjet_projects');
  const deadline = env.now() + timeoutMs;
  let lastError = null;
  while (env.now() < deadline) {
    try {
      await waitForSyncBridgeReady(bridge, Math.max(1, deadline - env.now()));
      const doc = await collection.findOne(projectId).exec();
      const rawProject = doc?.toJSON?.() || doc;
      if (rawProject?.owner_user_id === ownerUserId
        && rawProject?.name === expectedTitle
        && rawProject?.status === 'active') {
        const project = boundedWorkjetProjectResult(rawProject);
        if (project) return Object.freeze({
          ...project,
          workingCopies: Object.freeze(workingCopies.map(publicWorkjetWorkingCopyResult)),
        });
      }
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  const error = new Error(`Workjet project projection did not arrive for ${projectId}.`);
  error.code = 'workjet_project_projection_timeout';
  if (lastError) error.cause = lastError;
  throw error;
}

// ref: app.js:15171-15204. Wartet auf die Projektion in workjet_working_copies (Kern-Export K-WJ-1, freigegeben).
async function waitForProjectedWorkjetWorkingCopy(env, projectId, expected, ownerUserId, bridge, timeoutMs) {
  const collection = env.state.db?.collection?.('workjet_working_copies');
  const deadline = env.now() + timeoutMs;
  let lastError = null;
  while (env.now() < deadline) {
    try {
      await waitForSyncBridgeReady(bridge, Math.max(1, deadline - env.now()));
      const docs = await collection.find({
        selector: {
          project_id: { $eq: projectId },
          computer_id: { $eq: expected.computerId },
          status: { $eq: 'active' },
        },
        limit: 2,
      }).exec();
      const matches = docs
        .map((doc) => boundedWorkjetWorkingCopyResult(doc?.toJSON?.() || doc))
        .filter((copy) => copy?.ownerUserId === ownerUserId && copy.path === expected.path);
      if (matches.length === 1) return matches[0];
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  const error = new Error(`Workjet working-copy projection did not arrive for ${projectId}.`);
  error.code = 'workjet_working_copy_projection_timeout';
  if (lastError) error.cause = lastError;
  throw error;
}
// Für die Orakel-Hülle (Readiness-Orakel): Quelltextnamen der drei oben stehenden Funktionen.
export const PROJECTION_WAIT_SOURCES = Object.freeze(['waitForSyncBridgeReady', 'waitForProjectedWorkjetProject', 'waitForProjectedWorkjetWorkingCopy']);

// ref: app.js:15206-15216
function boundedWorkjetWorkingCopyRequest(value) {
  if (value === undefined) return null;
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('Invalid Workjet project workingCopy.');
  }
  assertWorkjetProjectPayloadKeys(value, new Set(['computerId', 'path']));
  return Object.freeze({
    computerId: boundedWorkjetProjectText(value.computerId, 'workingCopy.computerId', 256),
    path: boundedWorkjetProjectText(value.path, 'workingCopy.path', 4096),
  });
}

// ref: app.js:15242-15251
async function workjetProjectChildCommandId(parentCommandId, kind) {
  if (!crypto?.subtle) throw new Error('Secure Workjet command IDs are unavailable.'); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  const bytes = new TextEncoder().encode(`${kind}\0${parentCommandId}`);
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  const hex = Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
  return `cmd_workjet_${kind.replaceAll('-', '_')}_${hex}`;
}

// ---- project.create ---------------------------------------------------------------------------------------------
// ref: app.js:13906-13915 (Vorspann) + 14056-14061 (Datenebene) + 14917-14999
async function projectCreate(env, request) {
  const { state, actorContext } = env;
  const { ownerUserId } = scope(env);
  const { projectBridge, workingCopyBridge } = await requireWorkjetProjectDataPlane(env);
  assertWorkjetProjectPayloadKeys(request, new Set([
    'action',
    'commandId',
    'projectId',
    'title',
    'createdAt',
    'workingCopy',
  ]));
  const commandId = boundedWorkjetProjectText(request.commandId, 'commandId', 128);
  const projectId = boundedWorkjetProjectText(request.projectId, 'projectId', 128);
  const title = boundedWorkjetProjectText(request.title, 'title', 256);
  const requestedCreatedAt = boundedWorkjetProjectIsoDate(request.createdAt, 'createdAt');
  const requestedWorkingCopy = boundedWorkjetWorkingCopyRequest(request.workingCopy);
  await state.commandBus.dispatch({ // v4-allow: debt L-42
    id: commandId,
    command_id: commandId,
    module: 'ctox',
    command_type: 'ctox.workjet.project.upsert',
    record_id: projectId,
    payload: {
      project_id: projectId,
      name: title,
    },
    client_context: {
      source: 'workjet-project-control',
      actor: actorContext(state.session),
    },
  }, { until: 'terminal', timeoutMs: WORKJET_PROJECT_CONTROL_TIMEOUT_MS }); // v4-allow: debt L-42
  await waitForProjectedWorkjetProject(
    env,
    projectId,
    title,
    ownerUserId,
    projectBridge,
    WORKJET_PROJECT_CONTROL_TIMEOUT_MS,
  );
  if (requestedWorkingCopy) {
    const workingCopyCommandId = await workjetProjectChildCommandId(commandId, 'working-copy');
    await state.commandBus.dispatch({ // v4-allow: debt L-42
      id: workingCopyCommandId,
      command_id: workingCopyCommandId,
      module: 'ctox',
      command_type: 'ctox.workjet.working_copy.upsert',
      record_id: projectId,
      payload: {
        project_id: projectId,
        computer_id: requestedWorkingCopy.computerId,
        path: requestedWorkingCopy.path,
        active: true,
      },
      client_context: {
        source: 'workjet-project-control',
        actor: actorContext(state.session),
      },
    }, { until: 'terminal', timeoutMs: WORKJET_PROJECT_CONTROL_TIMEOUT_MS }); // v4-allow: debt L-42
    await waitForProjectedWorkjetWorkingCopy(
      env,
      projectId,
      requestedWorkingCopy,
      ownerUserId,
      workingCopyBridge,
      WORKJET_PROJECT_CONTROL_TIMEOUT_MS,
    );
  }
  const workingCopies = (await listProjectedWorkjetWorkingCopies(env, ownerUserId))
    .filter((copy) => copy.projectId === projectId);
  const project = await waitForProjectedWorkjetProject(
    env,
    projectId,
    title,
    ownerUserId,
    projectBridge,
    WORKJET_PROJECT_CONTROL_TIMEOUT_MS,
    workingCopies,
  );
  return {
    action: 'project.create',
    project: {
      ...project,
      createdAt: project.createdAt || requestedCreatedAt,
    },
  };
}

// ---- project.configure ------------------------------------------------------------------------------------------
// ref: app.js:13906-13915 + 14056-14061 + 14848-14915
async function projectConfigure(env, request) {
  const { state, actorContext } = env;
  const action = 'project.configure';
  const { ownerUserId, requestSession, requestDb } = scope(env);
  await requireWorkjetProjectDataPlane(env);
  assertWorkjetProjectPayloadKeys(request, new Set([
    'action', 'commandId', 'projectId', 'title',
    'description', 'repoUrl', 'publicUrl', 'info', 'jourFixe', 'feedSettings', 'supervisorLumaId', 'executionPolicy',
  ]));
  const commandId = boundedWorkjetProjectText(request.commandId, 'commandId', 128);
  const projectId = boundedWorkjetProjectText(request.projectId, 'projectId', 128);
  const title = boundedWorkjetProjectText(request.title, 'title', 256);
  const payload = { project_id: projectId, name: title, ...boundedWorkjetProjectMetadata(request) };
  const assertCurrentIdentity = () => {
    if (state.session !== requestSession || state.db !== requestDb
      || actorContext(state.session).id !== ownerUserId) {
      throw new Error('Workjet project session changed before the configuration was delivered.');
    }
  };
  assertCurrentIdentity();
  const receipt = await state.commandBus.dispatch({ // v4-allow: debt L-42
    id: commandId,
    command_id: commandId,
    module: 'ctox',
    command_type: 'ctox.workjet.project.upsert',
    record_id: projectId,
    payload,
    client_context: { source: 'workjet-project-control', actor: actorContext(requestSession) },
  }, { until: 'terminal', timeoutMs: WORKJET_PROJECT_CONTROL_TIMEOUT_MS }); // v4-allow: debt L-42
  assertCurrentIdentity();
  const nativeProject = receipt?.result?.project;
  // An authenticated alias can mutate its verified Owner's project.
  // The native writer binds this scope; caller payloads cannot select it.
  let projectOwnerUserId = ownerUserId;
  if (Object.hasOwn(receipt?.result || {}, 'owner_user_id')) {
    const confirmedOwner = receipt.result.owner_user_id;
    if (typeof confirmedOwner !== 'string' || !confirmedOwner || confirmedOwner.length > 256
      || confirmedOwner.trim() !== confirmedOwner || /[\u0000-\u001f\u007f]/.test(confirmedOwner)) {
      throw new Error('Workjet project configuration returned an uncorrelated owner identity.');
    }
    projectOwnerUserId = confirmedOwner;
  }
  if (receipt?.command_id !== commandId || receipt.status !== 'completed' || receipt.ok !== true
    || receipt.target_record_id !== projectId || receipt.result?.ok !== true
    || receipt.result?.collection !== 'workjet_projects'
    || nativeProject?.id !== projectId || nativeProject?.name !== title
    || nativeProject?.owner_user_id !== projectOwnerUserId) {
    throw new Error('Workjet project configuration returned an uncorrelated or unsuccessful receipt.');
  }
  const project = boundedWorkjetProjectResult(nativeProject, {
    includeConfiguration: true, includeSupervisorLuma: Object.hasOwn(payload, 'supervisor_luma_id'),
    includeExecutionPolicy: Object.hasOwn(payload, 'execution_policy'),
  });
  if (!project) throw new Error('Workjet project configuration returned no project.');
  if (Object.hasOwn(payload, 'execution_policy')) {
    const requested = payload.execution_policy;
    const returned = project.executionPolicy;
    if (!returned || returned.schema !== requested.schema || returned.mode !== requested.mode
      || ![requested.expected_revision, requested.expected_revision + 1].includes(returned.revision)
      || (requested.mode === 'autonomous_worktree' && returned.revision === 0)) {
      throw new Error('Workjet project configuration returned an unmatched execution policy.');
    }
  }
  if (Object.hasOwn(payload, 'supervisor_luma_id')) {
    const selected = project.supervisorLumaId ?? null;
    if (selected !== payload.supervisor_luma_id) {
      throw new Error('Workjet project configuration returned an unmatched supervisor Luma.');
    }
    return { action, commandId, project: Object.freeze({ ...project, supervisorLumaId: selected }) };
  }
  return { action, commandId, project };
}

// ---- project.init (Orchestrator-Entscheid: freigeschaltet; V2 hat nur eine Quelltext-Regex-Probe, Testdatei Z. 2039,
// daher eigene v4-Probe aus diesem Rumpf in tests/oracle.test.mjs, Block „Strang A (v4-Proben)“) -----------------
// ref: app.js:13906-13915 + 14056-14061 (Supervisor-Datenebene) + 14647-14669
async function projectInit(env, request) {
  const { state, actorContext } = env;
  const action = 'project.init';
  const { ownerUserId, requestSession, requestDb } = scope(env);
  await requireWorkjetSupervisorDataPlane(env);
  // One-click onboarding; a stable operationId makes polling idempotent.
  assertWorkjetProjectPayloadKeys(request, new Set(['action', 'commandId', 'projectId', 'operationId']));
  const commandId = boundedWorkjetProjectText(request.commandId, 'commandId', 128);
  const projectId = boundedWorkjetProjectText(request.projectId, 'projectId', 128);
  const operationId = boundedWorkjetProjectText(request.operationId, 'operationId', 128);
  const receipt = await state.commandBus.dispatch({ // v4-allow: debt L-42
    id: commandId, command_id: commandId, module: 'ctox',
    command_type: 'ctox.workjet.project.init', record_id: projectId,
    payload: { project_id: projectId, operation_id: operationId },
    client_context: { source: 'workjet-project-control', actor: actorContext(state.session) },
  }, { until: 'terminal', sync_queue_tasks: false, timeoutMs: WORKJET_PROJECT_CONTROL_TIMEOUT_MS }); // v4-allow: debt L-42
  if (state.session !== requestSession || state.db !== requestDb || actorContext(state.session).id !== ownerUserId
    || receipt?.command_id !== commandId || receipt.ok !== true || receipt.status !== 'completed'
    || receipt.result?.ok !== true || receipt.result?.project_id !== projectId
    || !Array.isArray(receipt.result?.steps)) {
    throw new Error('Project init returned an uncorrelated or unsuccessful response.');
  }
  const steps = receipt.result.steps.slice(0, 12).map((step) => ({
    id: String(step.id), state: String(step.state), detail: String(step.detail ?? '').slice(0, 256),
  }));
  return { action, commandId, projectId, operationId, steps };
}

// ---- project.kpis.read|configure|refresh ------------------------------------------------------------------------
// ref: app.js:13906-13915 + 14056-14061 (Supervisor-Datenebene) + 14241-14338. V2 teilt den Block mit
// project.jour_fixe.meeting.read (Strang C); hier steht nur der KPI-Zweig (meetingRead = false entfernt).
function projectKpis(action) {
  return async function projectKpisAction(env, request) {
    const { state, actorContext } = env;
    const { ownerUserId, requestSession, requestDb } = scope(env);
    await requireWorkjetSupervisorDataPlane(env);
    const configuring = action === 'project.kpis.configure';
    const allowedKeys = new Set(['action', 'commandId', 'projectId']);
    if (configuring) for (const key of ['operationId', 'expectedRevision', 'prompts']) allowedKeys.add(key);
    assertWorkjetProjectPayloadKeys(request, allowedKeys);
    const commandId = boundedWorkjetProjectText(request.commandId, 'commandId', 128);
    const projectId = boundedWorkjetProjectText(request.projectId, 'projectId', 128);
    const payload = { project_id: projectId };
    if (configuring) {
      payload.operation_id = boundedWorkjetProjectText(request.operationId, 'operationId', 128);
      if (!Number.isSafeInteger(request.expectedRevision) || request.expectedRevision < 0) {
        throw new TypeError('Workjet KPI expectedRevision must be a nonnegative safe integer.');
      }
      payload.expected_revision = request.expectedRevision;
      payload.prompts = request.prompts;
    }
    const requestType = configuring ? 'ConfigureKpisRequest' : 'ReadKpisRequest';
    const requestValidation = validateProjectKpiValue(requestType, payload);
    if (requestValidation.ok !== true) throw new TypeError(requestValidation.error);
    if (configuring) payload.prompts = payload.prompts.map(prompt => {
      assertWorkjetProjectPayloadKeys(prompt, new Set(['kpi_id', 'prompt']));
      return { kpi_id: prompt.kpi_id, prompt: prompt.prompt };
    });
    const assertCurrentIdentity = () => {
      if (state.session !== requestSession || state.db !== requestDb
        || actorContext(state.session).id !== ownerUserId) {
        throw new Error('Die Projektsitzung hat sich vor der Antwort geändert.');
      }
    };
    assertCurrentIdentity();
    const receipt = await state.commandBus.dispatch({ // v4-allow: debt L-42
      id: commandId, command_id: commandId, module: 'ctox', record_id: projectId,
      command_type: `ctox.workjet.${action}`,
      payload, client_context: { source: 'workjet-project-control', actor: actorContext(requestSession) },
    }, { until: 'terminal', sync_queue_tasks: false, timeoutMs: WORKJET_PROJECT_CONTROL_TIMEOUT_MS }); // v4-allow: debt L-42
    assertCurrentIdentity();
    if (receipt?.command_id !== commandId || receipt.ok !== true || receipt.status !== 'completed'
      || receipt.target_record_id !== projectId || receipt.result?.ok !== true
      || Object.entries(payload).some(([key, value]) => key === 'prompts'
        ? !Array.isArray(receipt.payload?.prompts) || receipt.payload.prompts.length !== value.length
          || value.some((prompt, index) => receipt.payload.prompts[index]?.kpi_id !== prompt.kpi_id
            || receipt.payload.prompts[index]?.prompt !== prompt.prompt)
        : receipt.payload?.[key] !== value)) {
      throw new Error('Für den Projektauftrag liegt keine passende Erfolgsbestätigung vor.');
    }
    const kpis = receipt.result.kpis;
    const kpiValidation = validateProjectKpiValue('ProjectKpis', kpis);
    if (kpiValidation.ok !== true) throw new TypeError(kpiValidation.error);
    if (kpis.project_id !== projectId || !Number.isSafeInteger(kpis.revision)) {
      throw new Error('Workjet KPI result belongs to another project or revision.');
    }
    const result = { action, commandId, projectId, contract: PROJECT_KPIS_SCHEMA, kpis: JSON.parse(JSON.stringify(kpis)) };
    if (action === 'project.kpis.refresh') {
      const refresh = receipt.result.refresh;
      const statuses = new Set(['fresh', 'stale', 'blocked', 'missing']);
      if (!refresh || typeof refresh.completed !== 'boolean' || !Array.isArray(refresh.items)
        || refresh.items.some(item => typeof item?.kpi_id !== 'string' || !statuses.has(item.status))) {
        throw new TypeError('Workjet KPI refresh result is malformed.');
      }
      result.refresh = {
        completed: refresh.completed,
        ...(typeof refresh.reason_code === 'string' ? { reasonCode: refresh.reason_code } : {}),
        items: refresh.items.map(item => ({ kpiId: item.kpi_id, status: item.status,
          ...(typeof item.reason_code === 'string' ? { reasonCode: item.reason_code } : {}) })),
      };
    }
    return result;
  };
}

// ---- project.exit_model.read|refresh|submit ---------------------------------------------------------------------
// ref: app.js:13906-13915 + 14056-14061 (Supervisor-Datenebene) + 14671-14711
function projectExitModel(action) {
  return async function projectExitModelAction(env, request) {
    const { state, actorContext } = env;
    const { ownerUserId, requestSession, requestDb } = scope(env);
    await requireWorkjetSupervisorDataPlane(env);
    assertWorkjetProjectPayloadKeys(request, new Set(['action', 'commandId', 'projectId', 'asOf', 'resources', 'inputs', 'confirmRunId']));
    const commandId = boundedWorkjetProjectText(request.commandId, 'commandId', 128);
    const projectId = boundedWorkjetProjectText(request.projectId, 'projectId', 128);
    const verb = action.split('.').at(-1);
    const confirming = Object.hasOwn(request, 'confirmRunId');
    if (verb === 'read' && ['asOf', 'resources', 'inputs', 'confirmRunId'].some((key) => Object.hasOwn(request, key))) throw new Error('Exit read accepts project identity only.');
    if (verb === 'refresh' && (Object.hasOwn(request, 'inputs') || confirming)) throw new Error('Exit refresh cannot submit researched inputs.');
    if (confirming && (verb !== 'submit' || ['asOf', 'resources', 'inputs'].some((key) => Object.hasOwn(request, key)))) throw new Error('Exit confirmation cannot override the stored proposal.');
    if (verb === 'submit' && !confirming && (!request.inputs || Object.hasOwn(request, 'resources'))) throw new Error('Exit submission requires typed inputs.');
    const payload = { project_id: projectId };
    if (confirming) payload.confirm_run_id = boundedWorkjetProjectText(request.confirmRunId, 'confirmRunId', 256);
    if (Object.hasOwn(request, 'asOf')) {
      if (typeof request.asOf !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(request.asOf)) throw new Error('Invalid exit assessment date.');
      payload.as_of = request.asOf;
    }
    if (Object.hasOwn(request, 'resources')) {
      const resource = request.resources;
      if (!resource || typeof resource !== 'object' || Array.isArray(resource)) throw new Error('Invalid exit resource proposal.');
      assertWorkjetProjectPayloadKeys(resource, new Set(['hoursPerWeek', 'monthlyBudgetEur', 'comparisonMode']));
      if (!Number.isFinite(resource.hoursPerWeek) || resource.hoursPerWeek < 0 || resource.hoursPerWeek > 168
        || !Number.isFinite(resource.monthlyBudgetEur) || resource.monthlyBudgetEur < 0
        || !['equal_resources', 'project_specific'].includes(resource.comparisonMode)) throw new Error('Invalid exit resource proposal.');
      payload.resources = { hours_per_week: resource.hoursPerWeek, monthly_budget_eur: resource.monthlyBudgetEur, comparison_mode: resource.comparisonMode };
    }
    if (Object.hasOwn(request, 'inputs')) payload.inputs = request.inputs;
    if (JSON.stringify(payload).length > 512 * 1024) throw new Error('Exit inputs exceed the payload budget.');
    const receipt = await state.commandBus.dispatch({ // v4-allow: debt L-42
      id: commandId, command_id: commandId, module: 'ctox',
      command_type: 'ctox.workjet.exit_model.' + verb, record_id: projectId, payload,
      client_context: { source: 'workjet-project-control', actor: actorContext(state.session) },
    }, { until: 'terminal', sync_queue_tasks: false, timeoutMs: WORKJET_PROJECT_CONTROL_TIMEOUT_MS }); // v4-allow: debt L-42
    if (state.session !== requestSession || state.db !== requestDb || actorContext(state.session).id !== ownerUserId
      || receipt?.command_id !== commandId || receipt.ok !== true || receipt.status !== 'completed'
      || receipt.target_record_id !== projectId || receipt.result?.ok !== true
      || receipt.result?.assessment?.contract !== 'ctox.workjet.exit_model.v1'
      || receipt.result?.assessment?.project_id !== projectId) {
      throw new Error('Exit assessment returned an uncorrelated or unsuccessful response.');
    }
    return { action, commandId, projectId, assessment: receipt.result.assessment };
  };
}

// ---- project.presentation.read|content.read|canvas.save (freigeschaltet WJB-R, E26 / K-WJ-2) --------------------
// ref: app.js:13906-13915 + 14005-14007 + 15478-15563. Wörtlich V2 inkl. content.read. Das Bereichslesen läuft wie in
// V2 über `bridge.state.demandFileLoader.fetchFile(fileId, { range })` mit range = {offset, length}; v2-compat.js
// startCollection bildet das auf api.files.get({ id, range: { start: offset, end: offset + length } }) ab und liefert
// die V2-Chunkliste [{ sequence: 0, bytesBase64, hash }] (WJB-R). Einzige v4-Abweichung im Rumpf: fehlt der Loader
// (Kernel ohne files.get), meldet readRange „nicht unterstützt“ statt V2 „Presentation file transport is unavailable.“
function projectPresentation(action) {
  return async function projectPresentationAction(env, request) {
    const { state, actorContext } = env;
    const { ownerUserId, requestSession, requestDb } = scope(env);
    const deadline = env.now() + WORKJET_PROJECT_CONTROL_TIMEOUT_MS - 1000;
    const requestSync = state.sync;
    const now = clock(env);
    const assertSession = () => {
      if (env.now() >= deadline || state.session !== requestSession || state.db !== requestDb
        || state.sync !== requestSync || actorContext(state.session).id !== ownerUserId) {
        throw Object.assign(new Error('The Workjet project session changed during the presentation request.'),
          { code: 'PRESENTATION_SCOPE_CHANGED' });
      }
    };
    const dispatch = async (commandId, projectId, commandType, payload, compareKeys) => {
      assertSession();
      const receipt = await state.commandBus.dispatch({ // v4-allow: debt L-42
        id: commandId, command_id: commandId, module: 'ctox', record_id: projectId,
        command_type: commandType, inbound_channel: 'ctox', payload: JSON.parse(JSON.stringify(payload)),
        client_context: { source: 'workjet-project-control', actor: actorContext(requestSession) },
      }, { until: 'terminal', sync_queue_tasks: false, timeoutMs: Math.max(1, deadline - env.now()) }); // v4-allow: debt L-42
      assertSession();
      if (receipt?.command_id !== commandId || receipt.ok !== true || receipt.status !== 'completed'
        || receipt.target_record_id !== projectId || receipt.result?.ok !== true
        || receipt.result.contract !== PRESENTATION_SCHEMA
        || compareKeys.some(key => receipt.payload?.[key] !== payload[key])) {
        const reason = receipt?.result?.error || receipt?.error;
        throw new Error(typeof reason === 'string' && reason ? reason : 'The presentation command was not confirmed.');
      }
      return receipt.result;
    };
    if (!PRESENTATION_ACTIONS.includes(action)) {
      throw new TypeError('Unsupported Workjet presentation action.');
    }
    await awaitWorkjetProjectListStep(requireWorkjetSupervisorDataPlane(env), deadline, 'presentation collections', now);
    const readManifest = async scopeRequest => {
      const commandId = scopeRequest.commandId ?? env.newId();
      const payload = presentationReadPayload({ action: PRESENTATION_READ_ACTION, commandId,
        projectId: scopeRequest.projectId, meetingId: scopeRequest.meetingId });
      const result = await dispatch(commandId, scopeRequest.projectId, 'ctox.workjet.presentation.read', payload,
        Object.keys(payload));
      return presentationFromReadResult(result, scopeRequest);
    };
    if (action === PRESENTATION_READ_ACTION) {
      presentationReadPayload(request);
      const presentation = await readManifest(request);
      return { action, commandId: request.commandId, projectId: request.projectId, meetingId: request.meetingId,
        contract: PRESENTATION_SCHEMA, presentation };
    }
    if (action === PRESENTATION_CANVAS_SAVE_ACTION) {
      const payload = presentationCanvasSavePayload(request);
      const result = await dispatch(request.commandId, request.projectId, 'ctox.workjet.presentation.canvas.save',
        payload, ['operation_id', 'presentation_id', 'expected_revision', 'slide_id']);
      const { mutation, presentation } = presentationMutationFromResult(result, payload, request);
      return { action, commandId: request.commandId, projectId: request.projectId, meetingId: request.meetingId,
        contract: PRESENTATION_SCHEMA, mutation, presentation };
    }
    const instance = boundedWorkjetProjectText(
      state.syncConfig?.instance_id || requestSync?.config?.instance_id, 'native instanceId', 256,
    );
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), Math.max(0, deadline - env.now()));
    const authority = {};
    let bridge;
    const assertCurrent = () => {
      assertSession();
      if (controller.signal.aborted
        || (state.syncConfig?.instance_id || state.sync?.config?.instance_id) !== instance
        || (authority.peer && (authority.peer.cancelled
          || authority.peer.collection.demandLoader !== authority.loader
          || authority.peer.collectionQueryGenerationToken?.(authority.peer.activeRemotePeerId) !== authority.generation))) {
        throw Object.assign(new Error('Presentation scope or connection changed.'), { code: 'PRESENTATION_SCOPE_CHANGED' });
      }
    };
    try {
      return await readJourFixePresentationContent(request, {
        assertCurrent,
        readManifest: scopeRequest => readManifest({ projectId: scopeRequest.projectId, meetingId: scopeRequest.meetingId }),
        readMetadata: async fileId => {
          bridge ??= await awaitWorkjetProjectListStep(
            requestSync?.startCollection?.('desktop_files', { pin: false, forceDirect: true }), deadline, 'presentation file bridge', now, // v4-allow: debt L-42
          );
          if (!bridge?.state && bridge?.ready) {
            bridge = await awaitWorkjetProjectListStep(bridge.ready, deadline, 'presentation file bridge readiness', now);
          }
          const rows = await readWorkjetProjectListRows(bridge, {
            selector: { id: { $eq: fileId } }, limit: 1,
          }, env.newId(), deadline, controller.signal, authority, now);
          return rows[0]?.toJSON?.() || rows[0] || null;
        },
        // K-WJ-2 / E26: Bereichslesen über v2-compat (siehe Kopfkommentar). Ohne demandFileLoader im v4-Peer (Kernel
        // ohne files.get) meldet die Aktion „nicht unterstützt“ statt „fehlgeschlagen“.
        readRange: (fileId, range) => {
          const loader = bridge?.state?.demandFileLoader;
          if (!loader?.fetchFile) throw unsupportedError('project', action, 'K-WJ-2: files.get ohne range');
          return awaitWorkjetProjectListStep(loader.fetchFile(fileId, { range }), deadline, 'presentation bytes', now);
        },
      });
    } finally { clearTimeout(timer); controller.abort(); }
  };
}

// Kern-Lücken je nicht freigeschalteter Aktion dieser Gruppe (Bericht WJB-A.md). Der Router meldet sie mit dem
// Workjet-Merkmal „Unsupported Workjet project control action: <a>“ (v2-compat unsupportedError).
// WJB-R: presentation.* sind mit E26 (files.get range, Stub 637caf6) freigeschaltet; keine offene Lücke mehr.
export const PROJECT_GAPS = Object.freeze({});

// Freigeschaltete Aktionen dieser Gruppe. Nur was hier steht, beantwortet die Brücke; alles andere ist unsupported.
export const PROJECT_ACTIONS = Object.freeze({
  'project.list': projectList,
  'project.worker.add': projectChatMutation(false),
  // Strang A
  'project.create': projectCreate,
  'project.configure': projectConfigure,
  'project.chat.create': projectChatMutation(true),
  'project.kpis.read': projectKpis('project.kpis.read'),
  'project.kpis.configure': projectKpis('project.kpis.configure'),
  'project.kpis.refresh': projectKpis('project.kpis.refresh'),
  'project.exit_model.read': projectExitModel('project.exit_model.read'),
  'project.exit_model.refresh': projectExitModel('project.exit_model.refresh'),
  'project.exit_model.submit': projectExitModel('project.exit_model.submit'),
  'project.init': projectInit,
  // Strang R (E26 / K-WJ-2)
  'project.presentation.read': projectPresentation('project.presentation.read'),
  'project.presentation.content.read': projectPresentation('project.presentation.content.read'),
  'project.presentation.canvas.save': projectPresentation('project.presentation.canvas.save'),
});

// Vorbereitet, nicht freigeschaltet (Gründe: PROJECT_GAPS). Seit WJB-R leer.
export const PREPARED = Object.freeze({});
