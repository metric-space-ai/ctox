'use strict';

const fs = require('fs');
const path = require('path');

async function withDeadline(work, timeoutMs, message) {
  let timer;
  try {
    return await Promise.race([
      work,
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(message())), timeoutMs); }),
    ]);
  } finally { clearTimeout(timer); }
}

// Each role receives its own browser profile and server-authenticated bootstrap.
// Signed capabilities go only to this fixture's local document/control origin;
// records and commands still travel through the real WebRTC data plane.
async function runThreadsRightClickPeers({
  chromium, launchOptions, runtimeRoot, smokeUrl, capabilities,
  smokeMode, threadsScaleSeed, browserDiagnostics, evidenceDir, readNativeAuthorizationState,
  readNativeSyncState,
  workflowTimeoutMs = 300000, closeTimeoutMs = 5000, diagnosticTimeoutMs = 3000,
}) {
  const origin = new URL(smokeUrl).origin;
  const contexts = [];
  const identities = [];
  const authorizationEvidence = {
    schema: 'ctox.threads.authorization_probe.v1',
    // Decoded payloads are diagnostic claims, not signature verification.
    issuedClaims: {
      requester: summarizeCapability(capabilities.requester?.token),
      reviewer: summarizeCapability(capabilities.reviewer?.token),
    },
    snapshots: [], deliveredCapabilities: [], deliveredCapabilityCount: 0,
  };
  const pendingCapabilityReads = new Set();
  let workflowError = null;
  let workflowFinished = false;
  let workflowPhase = 'opening-profiles';
  const persistDiagnostic = (name, value) => {
    if (!evidenceDir) return;
    fs.mkdirSync(evidenceDir, { recursive: true });
    const serialized = JSON.stringify(value ?? { available: false, reason: 'not-available' }, null, 2);
    // Never turn an oversized/truncated value into a purported valid snapshot.
    const output = Buffer.byteLength(serialized) <= 4 * 1024 * 1024 ? serialized
      : JSON.stringify({ available: false, reason: 'diagnostic-size-limit', bytes: Buffer.byteLength(serialized) });
    fs.writeFileSync(path.join(evidenceDir, name + '.json'), output + '\n');
  };
  const captureFailureDiagnostic = async (name, capture) => {
    let evidence;
    try {
      evidence = await withDeadline(Promise.resolve().then(capture), diagnosticTimeoutMs,
        () => name + ' capture deadline exceeded');
    } catch (error) {
      evidence = { available: false, reason: 'capture-failed', error: String(error?.message || error).slice(0, 240) };
    }
    try { persistDiagnostic(name, { atMs: Date.now(), workflowPhase, evidence }); }
    catch { console.error('threads_diagnostic_write_failed=' + name); }
  };
  const persistAuthorizationEvidence = () => {
    if (!evidenceDir) return;
    fs.mkdirSync(evidenceDir, { recursive: true });
    fs.writeFileSync(path.join(evidenceDir, 'threads-authorization.json'),
      JSON.stringify(authorizationEvidence, null, 2) + '\n');
  };
  const snapshotAuthorization = (phase) => {
    try {
      authorizationEvidence.snapshots.push({
        phase, atMs: Date.now(), identities: identities.map(identity => ({ ...identity })),
        native: readNativeAuthorizationState?.() ?? null,
      });
    } catch {
      // Preserve the workflow failure and make unavailable evidence explicit.
      authorizationEvidence.snapshots.push({ phase, atMs: Date.now(), nativeReadFailed: true });
    }
    persistAuthorizationEvidence();
  };
  snapshotAuthorization('before-browser-profiles');
  async function openActor(actor, role, capability) {
    if (!capability?.token) throw new Error('missing native capability for ' + actor);
    const profile = fs.mkdtempSync(path.join(runtimeRoot, actor + '-profile-'));
    const context = await chromium.launchPersistentContext(profile, launchOptions);
    contexts.push(context);
    await context.route((url) => url.origin === origin, (route) => route.continue({
      headers: { ...route.request().headers(), authorization: 'Bearer ' + capability.token },
    }));
    const page = await context.newPage();
    page.on('console', (message) => {
      const type = message.type();
      if (type === 'warning') browserDiagnostics.warnings += 1;
      if (type === 'error') browserDiagnostics.errors += 1;
      console.log('[browser:' + actor + ':' + type + '] ' + message.text());
    });
    page.on('pageerror', (error) => {
      browserDiagnostics.errors += 1;
      console.error('[browser:' + actor + ':error] ' + error.message);
    });
    page.on('requestfailed', (request) => {
      browserDiagnostics.requestFailures += 1;
      console.error('[browser:' + actor + ':requestfailed] ' + request.url());
    });
    page.on('response', (response) => {
      if (response.status() >= 400) {
        browserDiagnostics.assetResponseErrors += 1;
        console.error('[browser:' + actor + ':response] ' + response.status() + ' ' + response.url());
      }
      const url = new URL(response.url());
      if (url.origin === origin && url.pathname === '/api/business-os/auth/capability') {
        authorizationEvidence.deliveredCapabilityCount++;
        if (authorizationEvidence.deliveredCapabilities.length + pendingCapabilityReads.size < 40) {
          const work = (async () => {
            let claims = null;
            let bodyReadFailed = false;
            let timer;
            try {
              const body = await Promise.race([
                response.json(),
                new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('response timeout')), 3000); }),
              ]);
              claims = summarizeCapability(body.capability_token);
            } catch { bodyReadFailed = true; }
            finally { clearTimeout(timer); }
            authorizationEvidence.deliveredCapabilities.push({
              actor, atMs: Date.now(), httpStatus: response.status(), claims, bodyReadFailed,
            });
            persistAuthorizationEvidence();
          })();
          pendingCapabilityReads.add(work);
          work.catch(() => {}).finally(() => pendingCapabilityReads.delete(work));
        }
      }
    });
    await page.goto(smokeUrl, { waitUntil: 'commit', timeout: 60000 });
    await page.waitForFunction(({ actor, role }) => {
      const state = globalThis.CTOX_BUSINESS_OS_APP || globalThis.ctoxBusinessOsSmoke?.state;
      return state?.session?.authenticated
        && state.session.user?.id === actor && state.session.user?.role === role
        && state.db?.raw?.business_commands && state.sync
        && globalThis.CTOX_BUSINESS_OS_STATUS;
    }, { actor, role }, { timeout: 60000 });
    const identity = await page.evaluate(() => {
      const state = globalThis.CTOX_BUSINESS_OS_APP || globalThis.ctoxBusinessOsSmoke?.state;
      return {
        actorId: state.session.user.id, role: state.session.user.role,
        instanceId: state.syncConfig?.instance_id || state.sync?.config?.instance_id || '',
      };
    });
    identities.push(identity);
    snapshotAuthorization(actor + '-bootstrapped');
    return page;
  }

  try {
    const requester = await openActor('threads-requester', 'user', capabilities.requester);
    const reviewer = await openActor('threads-reviewer', 'admin', capabilities.reviewer);
    if (!identities[0].instanceId || identities[0].instanceId !== identities[1].instanceId) {
      throw new Error('requester and reviewer must authenticate to the same instance');
    }
    await requester.exposeFunction('__ctoxReviewThreadsApproval', async (request) => (
      reviewer.evaluate(runReviewerInBrowser, request)
    ));
    await requester.exposeFunction('__ctoxOpenContextTarget', async (request) => (
      requester.evaluate(openContextTargetInBrowser, request)
    ));
    await requester.exposeFunction('__ctoxReportThreadsPhase', (phase) => {
      if (workflowFinished) return;
      // Only fixture-owned phase labels are persisted, never browser records.
      if (!['open-threads-module', 'start-thread-collections', 'direct-denial',
        'context-data', 'context-ask', 'context-app', 'reviewer-approval'].includes(phase)) return;
      workflowPhase = phase;
      snapshotAuthorization('workflow:' + phase);
    });
    await requester.exposeFunction('__ctoxRecordThreadsStatus', (status) => {
      if (!workflowFinished) persistDiagnostic('threads-requester-command-status', status);
    });
    await reviewer.exposeFunction('__ctoxRecordThreadsStatus', (status) => {
      if (!workflowFinished) persistDiagnostic('threads-reviewer-result-status', status);
    });
    workflowPhase = 'requester-start';
    snapshotAuthorization('workflow:' + workflowPhase);
    // Playwright evaluate has no timeout. A blocked module/dispatch promise must
    // produce failure evidence and enter teardown before CI kills the process.
    const result = await withDeadline(
      requester.evaluate(runRequesterInBrowser, { smokeMode, threadsScaleSeed }),
      workflowTimeoutMs, () => `threads workflow exceeded ${workflowTimeoutMs} ms in ${workflowPhase}`,
    );
    workflowFinished = true;
    console.log('threads_authenticated_peers=' + JSON.stringify(identities));
    return { ...result, authenticatedPeers: identities, isolatedBrowserProfiles: true };
  } catch (error) {
    workflowError = error;
    workflowFinished = true;
    await Promise.allSettled([...pendingCapabilityReads]);
    snapshotAuthorization('workflow-failed');
    if (evidenceDir) {
      await captureFailureDiagnostic('threads-native-failure', () => readNativeSyncState
        ? readNativeSyncState() : { available: false, reason: 'reader-not-configured' });
    }
    for (let index = 0; index < contexts.length; index += 1) {
      const pages = contexts[index].pages();
      const page = pages[pages.length - 1];
      if (!page || page.isClosed()) continue;
      const actor = index === 0 ? 'threads-requester' : 'threads-reviewer';
      if (evidenceDir) {
        await captureFailureDiagnostic(actor + '-failure-state',
          () => page.evaluate(readCachedThreadFailureState));
      }
      let screenshot = '';
      if (evidenceDir) {
        fs.mkdirSync(evidenceDir, { recursive: true });
        screenshot = path.join(evidenceDir, actor + '-failure.png');
        await page.screenshot({ path: screenshot, timeout: 5000 }).catch(() => { screenshot = ''; });
      }
      console.error('threads_actor_failure=' + JSON.stringify({
        actor, screenshot,
        visibleText: (await page.locator('body').innerText({ timeout: 3000 }).catch(() => '')).slice(0, 4000),
      }));
    }
    throw error;
  } finally {
    workflowFinished = true;
    const closed = await Promise.allSettled(contexts.map((context, index) => withDeadline(
      context.close(), closeTimeoutMs, () => `threads browser profile ${index} did not close`,
    )));
    const closeErrors = closed.filter(result => result.status === 'rejected').map(result => result.reason);
    await Promise.allSettled([...pendingCapabilityReads]);
    snapshotAuthorization(closeErrors.length ? 'profile-close-failed' : 'profiles-closed');
    if (closeErrors.length) {
      console.error('threads_profile_close_failed=' + closeErrors.length);
      if (!workflowError) throw new AggregateError(closeErrors, 'threads browser profile cleanup failed');
    }
  }
}

function readCachedThreadFailureState() {
  const state = globalThis.CTOX_BUSINESS_OS_APP || globalThis.ctoxBusinessOsSmoke?.state;
  // Read already-published diagnostics only. No query, repair or new status
  // snapshot may perturb the failed transport while collecting its evidence.
  return {
    capturedAtMs: Date.now(),
    actorId: state?.session?.user?.id || null,
    role: state?.session?.user?.role || null,
    sync: globalThis.ctoxBusinessOsSyncDiagnostics || null,
    available: Boolean(globalThis.ctoxBusinessOsSyncDiagnostics),
  };
}

function summarizeCapability(token) {
  if (typeof token !== 'string' || !token) return { present: false };
  try {
    const payload = JSON.parse(Buffer.from(token.split('.')[0], 'base64url').toString('utf8'));
    return {
      present: true, unverifiedPayload: true,
      userId: typeof payload.uid === 'string' ? payload.uid.slice(0, 160) : null,
      role: typeof payload.role === 'string' ? payload.role.slice(0, 32) : null,
      epoch: Number.isSafeInteger(payload.epoch) ? payload.epoch : null,
      issuedAtMs: Number.isSafeInteger(payload.iat) ? payload.iat : null,
      expiresAtMs: Number.isSafeInteger(payload.exp) ? payload.exp : null,
      deviceBound: Boolean(payload.cnf || payload.device_pairing_id || payload.device_id),
    };
  } catch {
    return { present: true, payloadDecodeFailed: true };
  }
}

async function openContextTargetInBrowser({ moduleId, recordId, timeoutMs = 30000 }) {
  const state = globalThis.CTOX_BUSINESS_OS_APP || globalThis.ctoxBusinessOsSmoke?.state;
  await state.openModule(moduleId, { force: true, asModule: true });
  const ownerId = 'desktop-app:' + moduleId;
  const selector = `[data-shell-window="true"][data-owner-id="${CSS.escape(ownerId)}"] [data-module-root="${CSS.escape(moduleId)}"]`;
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    const win = state.windowManager?.listWindows?.().find(entry => entry.ownerId === ownerId);
    const root = document.querySelector(selector);
    const bounds = root?.getBoundingClientRect();
    const host = root?.querySelector('[data-module-content]');
    last = {
      ok: Boolean(win?.isFocused && win.state !== 'minimized'
        && root?.dataset.moduleReady === 'true' && root?.dataset.moduleLoadFailed !== 'true'
        && bounds?.width > 0 && bounds?.height > 0 && host),
      windowId: win?.id || null, ownerId: win?.ownerId || null,
      activeModule: state.activeModule?.id || '', moduleReady: root?.dataset.moduleReady || null,
      moduleLoadFailed: root?.dataset.moduleLoadFailed || null,
      focused: win?.isFocused === true,
    };
    if (last.ok) {
      let marker = host.querySelector('[data-threads-rightclick-fixture]');
      for (const other of document.querySelectorAll('[data-threads-rightclick-fixture]')) {
        if (other !== marker) other.remove();
      }
      if (!marker) {
        marker = document.createElement('section');
        marker.dataset.threadsRightclickFixture = 'true';
        marker.dataset.moduleRoot = moduleId;
        marker.dataset.contextRecordId = recordId;
        marker.dataset.contextRecordType = 'smoke-record';
        marker.dataset.contextLabel = 'Threads Right-Click Smoke Record';
        marker.style.padding = '8px';
        marker.style.margin = '8px';
        marker.textContent = 'Threads Right-Click Smoke Record';
        host.prepend(marker);
      }
      marker.scrollIntoView({ block: 'nearest' });
      console.log('threads_context_target=' + JSON.stringify(last));
      return last;
    }
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error('threads right-click target window open timed out: ' + JSON.stringify(last));
}

async function runRequesterInBrowser({ smokeMode, threadsScaleSeed }) {
  const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const waitFor = async (predicate, ms, label) => {
    const deadline = Date.now() + ms;
    let last = null;
    while (Date.now() < deadline) {
      last = await predicate();
      if (last?.ok) return last;
      await delay(100);
    }
    throw new Error(`${label} timed out: ${JSON.stringify(last)}`);
  };
  const css = (value) => {
    if (globalThis.CSS?.escape) return globalThis.CSS.escape(String(value));
    return String(value).replace(/["\\]/g, '\\$&');
  };
  const docsToJson = (docs) => (Array.isArray(docs) ? docs : [])
    .map((doc) => doc?.toJSON?.() || doc)
    .filter((doc) => doc && doc._deleted !== true && doc.is_deleted !== true);
  const smoke = globalThis.ctoxBusinessOsSmoke;
  const state = globalThis.CTOX_BUSINESS_OS_APP || smoke?.state;
  if (!state) throw new Error('Business OS app state is unavailable for threads right-click UI smoke');
  if (typeof smoke?.renderTabs !== 'function') throw new Error('Business OS smoke renderTabs hook is unavailable');
  if (typeof state.openModule !== 'function') throw new Error('Business OS state.openModule is unavailable for threads right-click UI smoke');
  if (typeof state.commandBus?.dispatch !== 'function') throw new Error('Business OS command bus is unavailable for threads right-click UI smoke');

  const targetModule = {
    id: 'tickets',
    title: 'Tickets',
    glyph: 'T',
  };
  const requesterSession = state.session;
  if (requesterSession?.user?.id !== 'threads-requester' || requesterSession.user.role !== 'user') {
    throw new Error('requester shell must use its server-authenticated actor');
  }
  const reviewerId = 'threads-reviewer';
  const targetRecordId = 'tickets_seed_ops_review';
  const appTargetRecordId = 'tickets';
  const threadsCollections = [
    'user_threads',
    'user_thread_messages',
    'user_thread_links',
    'user_notifications',
    'ctox_task_approval_requests',
  ];
  const dataPrompt = `Threads right-click data change ${Date.now()}`;
  const askPrompt = `Threads right-click question ${Date.now()}`;
  const appPrompt = `Threads right-click app change ${Date.now()}`;
  const reviewerPickerEvidence = [];
  let scaleFirstRenderEvidence = null;
  const ensureThreadsModuleCollections = async () => {
    const renderStartedAt = performance.now();
    await state.openModule('threads', { force: true, asModule: true });
    await waitFor(() => {
      const raw = state.db?.raw || {};
      return {
        ok: threadsCollections.every((name) => Boolean(raw[name])),
        activeModule: state.activeModule?.id || '',
        missing: threadsCollections.filter((name) => !raw[name]),
      };
    }, 30000, 'threads module collections registered');
    await Promise.all(threadsCollections.map((name) => (
      state.sync?.startCollection?.(name).catch(() => null)
    )));
    if (threadsScaleSeed) {
      scaleFirstRenderEvidence = await waitFor(() => {
        const root = document.querySelector('[data-threads-root]');
        const visibleThreadRows = root?.querySelectorAll?.('[data-thread-id]')?.length || 0;
        return {
          ok: Boolean(root && visibleThreadRows > 0 && visibleThreadRows <= 200),
          visibleThreadRows,
          renderMs: Math.round(performance.now() - renderStartedAt),
        };
      }, 30000, 'threads scale first bounded render');
    }
  };
  const openTargetModule = async () => {
    return globalThis.__ctoxOpenContextTarget({ moduleId: targetModule.id, recordId: targetRecordId });
  };
  const openGlobalContextMenu = async () => {
    const target = document.querySelector('[data-threads-rightclick-fixture]');
    if (!target) throw new Error('threads right-click fixture DOM target is missing');
    const rect = target.getBoundingClientRect();
    target.dispatchEvent(new MouseEvent('contextmenu', {
      bubbles: true,
      cancelable: true,
      button: 2,
      clientX: Math.max(24, Math.round(rect.left + 12)),
      clientY: Math.max(24, Math.round(rect.top + 12)),
    }));
    return waitFor(() => {
      const menu = document.querySelector('.ctox-global-context-menu:not([hidden])');
      const form = menu?.querySelector('form') || null;
      const modes = menu ? [...menu.querySelectorAll('input[name="contextMode"]')].map((input) => input.value) : [];
      return {
        ok: Boolean(menu && form && ['data', 'ask', 'app'].every((mode) => modes.includes(mode))),
        modes,
        text: menu?.textContent?.trim().slice(0, 1000) || '',
      };
    }, 5000, 'threads right-click global context menu');
  };
  const waitForReviewerOption = async () => waitFor(() => {
    const menu = document.querySelector('.ctox-global-context-menu:not([hidden])');
    const options = menu
      ? [...menu.querySelectorAll('[data-ctox-context-user-options] option')].map((option) => ({
        value: option.getAttribute('value') || '',
        label: option.getAttribute('label') || '',
      }))
      : [];
    return {
      ok: options.some((option) => option.value === reviewerId && /Threads Reviewer/.test(option.label)),
      options,
    };
  }, 5000, 'threads right-click reviewer option');
  const submitContextMode = async ({ mode, message, userId, contextRecordId = targetRecordId }) => {
    await openTargetModule();
    const contextTarget = document.querySelector('[data-threads-rightclick-fixture]');
    contextTarget.dataset.contextRecordId = contextRecordId;
    contextTarget.dataset.contextLabel = contextRecordId === appTargetRecordId
      ? 'Threads Right-Click App Smoke Record'
      : 'Threads Right-Click Smoke Record';
    await openGlobalContextMenu();
    const reviewerOption = await waitForReviewerOption().catch((error) => ({
      ok: false,
      error: String(error?.message || error),
    }));
    reviewerPickerEvidence.push({
      mode,
      visible: reviewerOption.ok === true,
      optionCount: Array.isArray(reviewerOption.options) ? reviewerOption.options.length : 0,
      error: reviewerOption.error || '',
    });
    const menu = document.querySelector('.ctox-global-context-menu:not([hidden])');
    const form = menu?.querySelector('form');
    const input = menu?.querySelector(`input[name="contextMode"][value="${css(mode)}"]`);
    const label = input?.closest('label') || null;
    const textarea = menu?.querySelector('.ctox-context-textarea');
    const userInput = menu?.querySelector('.ctox-context-user-input');
    if (!form || !input || !label || !textarea || !userInput) {
      throw new Error(`threads right-click context form missing controls for ${mode}`);
    }
    label.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
    const needsApproval = mode === 'data' || mode === 'app';
    await waitFor(() => {
      const row = userInput.closest('.ctox-context-user-row');
      const visible = row?.hidden === false && getComputedStyle(row).display !== 'none';
      return {
        ok: visible === needsApproval,
        hidden: row?.hidden ?? null,
        display: getComputedStyle(row).display,
      };
    }, 5000, `threads right-click ${mode} delegation state`);
    if (needsApproval) {
      userInput.value = userId;
      userInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    textarea.value = message;
    textarea.dispatchEvent(new Event('input', { bubbles: true }));
    const submittedAt = performance.now();
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    if (!needsApproval) {
      await waitFor(() => {
        const promptWindow = [...document.querySelectorAll('.ctox-chat-window.is-active')]
          .find((element) => element.textContent.includes(message));
        const rect = promptWindow?.getBoundingClientRect();
        const style = promptWindow ? getComputedStyle(promptWindow) : null;
        return {
          ok: Boolean(rect && rect.width > 0 && rect.height > 0
            && rect.bottom > 0 && rect.top < innerHeight
            && rect.right > 0 && rect.left < innerWidth
            && style.display !== 'none' && style.visibility !== 'hidden'
            && Number(style.opacity) > 0
            && !document.querySelector('.ctox-global-context-menu:not([hidden])')),
        };
      }, 5000, 'context prompt visible before native receipt');
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const firstPaintMs = performance.now() - submittedAt;
      console.log('context_prompt_first_paint=' + JSON.stringify({ mode, firstPaintMs }));
      if (firstPaintMs >= 150) {
        throw new Error(`Context prompt first paint exceeded 150ms: ${firstPaintMs.toFixed(1)}ms`);
      }
    }
    await waitFor(() => ({
      ok: !document.querySelector('.ctox-global-context-menu:not([hidden])'),
      status: document.querySelector('.ctox-global-context-menu .ctox-context-status')?.textContent?.trim() || '',
    }), 70000, `threads right-click ${mode} submit accepted`);
    const commandCollection = state.db?.raw?.business_commands || state.db?.collection?.('business_commands');
    return waitFor(async () => {
      const expectedCommandType = needsApproval
        ? 'threads.ctox_approval.request'
        : 'business_os.context.ask';
      const docs = docsToJson(await commandCollection.find({
        selector: { command_type: expectedCommandType },
        sort: [{ updated_at_ms: 'desc' }],
        limit: 50,
      }).exec());
      const command = docs.find((doc) => {
          if (needsApproval) {
            return doc.command_type === 'threads.ctox_approval.request'
              && doc.payload?.prompt === message
              && doc.payload?.reviewer_user_id === userId
              && doc.payload?.target_command_type === (mode === 'app'
                ? 'ctox.business_os.app.modify'
                : 'business_os.data.modify')
              && doc.payload?.source_context?.record_id === contextRecordId;
          }
          return doc.command_type === 'business_os.context.ask'
            && doc.record_id === contextRecordId;
        });
      return {
        ok: Boolean(command),
        command: command || null,
        commandCount: docs.length,
        sample: command ? null : (docs[0] || null),
      };
    }, 30000, `threads right-click ${mode} command persisted`);
  };

  await globalThis.__ctoxReportThreadsPhase('open-threads-module');
  await ensureThreadsModuleCollections();
  await waitFor(() => {
    const raw = state.db?.raw || {};
    const names = [
      'business_commands',
      'business_users',
      ...threadsCollections,
    ];
    return {
      ok: names.every((name) => Boolean(raw[name])),
      missing: names.filter((name) => !raw[name]),
    };
  }, 30000, 'threads right-click collections available');
  await globalThis.__ctoxReportThreadsPhase('start-thread-collections');
  await Promise.all([
    'business_commands',
    'business_users',
    ...threadsCollections,
    'ctox_queue_tasks',
  ].map((name) => state.sync?.startCollection?.(name).catch(() => null)));

  const deniedCommandId = `cmd_${crypto.randomUUID()}`;
  let deniedDispatchError = '';
  await globalThis.__ctoxReportThreadsPhase('direct-denial');
  try {
    await state.commandBus.dispatch({
      id: deniedCommandId,
      module: targetModule.id,
      command_type: 'business_os.data.modify',
      record_id: targetRecordId,
      inbound_channel: targetModule.id,
      payload: {
        prompt: `Denied direct data change ${Date.now()}`,
        instruction: 'This direct mutation must be denied before delegation.',
        context: {
          module: targetModule.id,
          record_type: 'smoke-record',
          record_id: targetRecordId,
          label: 'Threads Right-Click Smoke Record',
        },
      },
      client_context: {
        action: 'context-data-modify-direct-denial-smoke',
        module: targetModule.id,
        module_id: targetModule.id,
        app_id: targetModule.id,
        actor: requesterSession.user,
        record_id: targetRecordId,
      },
    }, { until: 'local' });
  } catch (error) {
    deniedDispatchError = String(error?.message || error);
  }
  const deniedDirectCommand = await waitFor(async () => {
    const docs = docsToJson(await state.db.raw.business_commands.find({
      selector: { command_id: deniedCommandId },
      limit: 5,
    }).exec());
    const command = docs.find((item) => (
      item.command_id === deniedCommandId || item.id === deniedCommandId
    )) || null;
    const serialized = JSON.stringify(command || {});
    return {
      ok: Boolean(command && command.status === 'failed' && serialized.includes('role_or_scope_denied')),
      // waitFor serializes its last result on timeout. Keep only the fields
      // needed to diagnose admission; client_context contains a signed bearer.
      command: command ? {
        id: command.id,
        command_id: command.command_id,
        status: command.status,
        command_type: command.command_type,
        result: {
          reason_code: command.result?.reason_code,
          decision: { reason_code: command.result?.decision?.reason_code },
        },
      } : null,
      deniedDispatchError,
    };
  }, 30000, 'threads right-click direct native denial');

  await globalThis.__ctoxReportThreadsPhase('context-data');
  const dataCommand = await submitContextMode({ mode: 'data', message: dataPrompt, userId: reviewerId });
  await globalThis.__ctoxReportThreadsPhase('context-ask');
  const askCommand = await submitContextMode({ mode: 'ask', message: askPrompt, userId: reviewerId });
  await globalThis.__ctoxReportThreadsPhase('context-app');
  const appCommand = await submitContextMode({
    mode: 'app',
    message: appPrompt,
    userId: reviewerId,
    contextRecordId: appTargetRecordId,
  });
  const contextCaptured = [
    [dataCommand, targetRecordId],
    [askCommand, targetRecordId],
    [appCommand, appTargetRecordId],
  ].every(([entry, expectedRecordId]) => {
    const command = entry.command || {};
    const context = command.payload?.source_context || {};
    const contextV2 = context.context_v2 || command.client_context?.context || {};
    const pointer = contextV2.pointer || {};
    return command.record_id === expectedRecordId
      && (context.record_id === expectedRecordId || contextV2.entity?.id === expectedRecordId)
      && Number.isFinite(pointer.x)
      && Number.isFinite(pointer.y);
  });
  if (!contextCaptured) {
    throw new Error('threads right-click commands lost their exact record or pointer context');
  }
  const commandStatus = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
    includeCounts: false,
    requiredCollections: ['business_commands', 'business_users'],
  });
  await globalThis.__ctoxRecordThreadsStatus(commandStatus);
  if (commandStatus?.version !== 'business-os-advanced-status-v1' || commandStatus.ok !== true) {
    throw new Error('threads right-click command status unhealthy; see threads-requester-command-status.json');
  }
  await globalThis.__ctoxReportThreadsPhase('reviewer-approval');
  const { projections, rendered, approvalDecision, status, authenticatedReviewer } =
    await globalThis.__ctoxReviewThreadsApproval({
      targetModule, targetRecordId, appTargetRecordId, threadsCollections,
      dataPrompt, appPrompt, reviewerId,
    });

  const scale = threadsScaleSeed || {};
  const scaleBudgetPassed = !threadsScaleSeed || (Number(scale.commands || 0) >= 10000
    && Number(scale.threads || 0) >= 10000
    && Number(scale.messages || 0) >= 10000
    && Number(scale.notifications || 0) >= 10000
    && Number(scaleFirstRenderEvidence?.visibleThreadRows || 0) >= 1
    && Number(scaleFirstRenderEvidence?.visibleThreadRows || 0) <= 200
    && Number(scaleFirstRenderEvidence?.renderMs || 0) <= 30000);
  if (threadsScaleSeed && !scaleBudgetPassed) {
    throw new Error(`threads right-click scale budget failed: ${JSON.stringify({
      scale,
      scaleFirstRenderEvidence,
    }, null, 2)}`);
  }

  return {
    mode: smokeMode,
    targetModuleId: targetModule.id,
    reviewerId,
    threadId: projections.thread.id,
    dataCommandId: dataCommand.command?.command_id || dataCommand.command?.id || '',
    askCommandId: askCommand.command?.command_id || askCommand.command?.id || '',
    appCommandId: appCommand.command?.command_id || appCommand.command?.id || '',
    dataApprovalCommandPersisted: dataCommand.command?.command_type === 'threads.ctox_approval.request',
    directDenialCommandId: deniedCommandId,
    directDenialReason: deniedDirectCommand.command?.result?.reason_code
      || deniedDirectCommand.command?.result?.decision?.reason_code
      || 'role_or_scope_denied',
    dataApprovalId: projections.dataApproval.id || '',
    appApprovalId: projections.appApproval?.id || '',
    dataApprovalProjected: Boolean(projections.dataApproval),
    askCommandPersisted: Boolean(askCommand.command),
    appApprovalCommandPersisted: appCommand.command?.command_type === 'threads.ctox_approval.request',
    appApprovalProjected: Boolean(projections.appApproval),
    sourceContextCaptured: [
      [dataCommand, targetRecordId],
      [askCommand, targetRecordId],
      [appCommand, appTargetRecordId],
    ].every(([entry, expectedRecordId]) => {
      const command = entry.command || {};
      const context = command.payload?.source_context || {};
      const contextV2 = context.context_v2 || command.client_context?.context || {};
      const pointer = contextV2.pointer || {};
      return command.record_id === expectedRecordId
        && (context.record_id === expectedRecordId || contextV2.entity?.id === expectedRecordId)
        && Number.isFinite(pointer.x)
        && Number.isFinite(pointer.y);
    }),
    reviewerNotificationProjected: Boolean(projections.reviewerNotification),
    hubRendered: rendered.ok === true,
    approvalActionRendered: rendered.hasApproveButton === true,
    approvalDecision: approvalDecision.approval?.status || '',
    approvedCommandId: approvalDecision.approval?.approved_command_id || '',
    approvedCommandStatus: approvalDecision.approvedCommand?.status || '',
    reauthorizationLinked: approvalDecision.approvalLink === projections.dataApproval.id,
    authState: 'authenticated',
    browserContext: 'clean',
    tenantScope: 'local-workspace',
    actorRole: requesterSession.user.role,
    reviewerRole: authenticatedReviewer.role,
    reviewerPickerVisible: reviewerPickerEvidence.some((item) => item.visible),
    reviewerPickerEvidence,
    scaleCommands: Number(scale.commands || 0),
    scaleThreads: Number(scale.threads || 0),
    scaleMessages: Number(scale.messages || 0),
    scaleNotifications: Number(scale.notifications || 0),
    scaleVisibleThreadRows: Number(scaleFirstRenderEvidence?.visibleThreadRows || 0),
    scaleFirstRenderMs: Number(scaleFirstRenderEvidence?.renderMs || 0),
    scaleBudgetPassed,
    advancedStatusVersion: status.version || '',
    advancedStatusRuntime: status.rxdbRuntime || null,
  };

}

async function runReviewerInBrowser({
  targetModule, targetRecordId, appTargetRecordId, threadsCollections,
  dataPrompt, appPrompt, reviewerId,
}) {
  const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const waitFor = async (predicate, ms, label) => {
    const deadline = Date.now() + ms;
    let last = null;
    while (Date.now() < deadline) {
      last = await predicate();
      if (last?.ok) return last;
      await delay(100);
    }
    throw new Error(label + ' timed out: ' + JSON.stringify(last));
  };
  const css = (value) => CSS.escape(String(value));
  const docsToJson = (docs) => (Array.isArray(docs) ? docs : [])
    .map((doc) => doc?.toJSON?.() || doc)
    .filter((doc) => doc && doc._deleted !== true && doc.is_deleted !== true);
  const state = globalThis.CTOX_BUSINESS_OS_APP || globalThis.ctoxBusinessOsSmoke?.state;
  if (state?.session?.user?.id !== reviewerId || state.session.user.role !== 'admin') {
    throw new Error('reviewer shell must use its server-authenticated actor');
  }
  await state.openModule('threads', { force: true, asModule: true });
  await waitFor(() => ({ ok: threadsCollections.every((name) => Boolean(state.db?.raw?.[name])) }),
    30000, 'reviewer thread collections registered');
  await Promise.all(threadsCollections.map((name) => state.sync.startCollection(name)));
  const rawDb = state.db.raw;
  const expectedThreadId = `thread_${targetModule.id}_smoke-record_${targetRecordId}`;
  const projectionUpdatedAfterMs = Date.now() - 5 * 60 * 1000;
  let projectionPollAttempt = 0;
  let projectedThread = null;
  let projectedMessages = [];
  let projectedNotifications = [];
  let projectedApprovals = [];
  let projectedAppApprovals = [];
  const projections = await waitFor(async () => {
    const currentProjectionAttempt = projectionPollAttempt++;
    const threadDocs = projectedThread ? [projectedThread] : docsToJson(await rawDb.user_threads.find({
      // Demand-query windows are intentionally stale-while-revalidate.
      // Vary the lower bound so a previously completed empty window
      // cannot mask a projection created just after the first poll.
      selector: {
        id: expectedThreadId,
        updated_at_ms: { $gte: projectionUpdatedAfterMs + currentProjectionAttempt },
      },
      limit: 1,
    }).exec());
    const thread = threadDocs.find((item) => item.id === expectedThreadId) || null;
    if (thread) projectedThread = thread;
    const relatedUpdatedAfterMs = projectionUpdatedAfterMs + currentProjectionAttempt;
    const relatedQuery = thread ? {
      selector: {
        thread_id: thread.id,
        updated_at_ms: { $gte: relatedUpdatedAfterMs },
      },
      sort: [{ updated_at_ms: 'desc' }],
      limit: 50,
    } : null;
    const [messages, notifications, approvals, appApprovals] = relatedQuery
      ? await Promise.all([
        projectedMessages.length
          ? projectedMessages
          : rawDb.user_thread_messages.find(relatedQuery).exec().then(docsToJson),
        projectedNotifications.length
          ? projectedNotifications
          : rawDb.user_notifications.find(relatedQuery).exec().then(docsToJson),
        projectedApprovals.length
          ? projectedApprovals
          : rawDb.ctox_task_approval_requests.find(relatedQuery).exec().then(docsToJson),
        projectedAppApprovals.length
          ? projectedAppApprovals
          : rawDb.ctox_task_approval_requests.find({
          selector: {
            source_record_id: appTargetRecordId,
            reviewer_user_id: reviewerId,
            status: 'pending',
            updated_at_ms: { $gte: relatedUpdatedAfterMs },
          },
          sort: [{ updated_at_ms: 'desc' }],
          limit: 20,
        }).exec().then(docsToJson),
      ])
      : [[], [], [], []];
    if (messages.length) projectedMessages = messages;
    if (notifications.length) projectedNotifications = notifications;
    if (approvals.length) projectedApprovals = approvals;
    if (appApprovals.length) projectedAppApprovals = appApprovals;
    const threadMessages = thread
      ? messages.filter((item) => item.thread_id === thread.id)
      : [];
    const threadNotifications = thread
      ? notifications.filter((item) => item.thread_id === thread.id)
      : [];
    const threadApprovals = thread
      ? approvals.filter((item) => item.thread_id === thread.id)
      : [];
    const dataApproval = threadApprovals.find((item) => (
      item.prompt === dataPrompt
      && item.reviewer_user_id === reviewerId
      && item.status === 'pending'
    )) || null;
    const appApproval = appApprovals.find((item) => (
      item.prompt === appPrompt
      && item.reviewer_user_id === reviewerId
      && item.status === 'pending'
    )) || null;
    const reviewerNotification = threadNotifications.find((item) => item.user_id === reviewerId) || null;
    return {
      ok: Boolean(thread && dataApproval && reviewerNotification),
      thread,
      dataApproval,
      appApproval,
      reviewerNotification,
      counts: {
        threads: thread ? 1 : 0,
        messages: messages.length,
        notifications: notifications.length,
        approvals: approvals.length,
      },
    };
  }, 150000, 'threads right-click native projections');

  await state.openModule('threads', { force: true, asModule: true });
  const rendered = await waitFor(() => {
    const root = document.querySelector('[data-threads-root]');
    const row = root?.querySelector(`[data-thread-id="${css(projections.thread.id)}"]`) || null;
    row?.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
    const timeline = root?.querySelector('[data-thread-timeline]') || null;
    const dataApprovalCard = root?.querySelector(`[data-approval-id="${css(projections.dataApproval.id)}"]`) || null;
    const timelineText = timeline?.innerText || timeline?.textContent || '';
    const contextText = root?.querySelector('[data-thread-context]')?.innerText || '';
    return {
      ok: Boolean(
        root
          && row
          && timelineText.includes(dataPrompt)
          && dataApprovalCard
          && dataApprovalCard.querySelector('[data-approve-approval]')
          && /Threads Reviewer|threads-reviewer/.test(timelineText)
      ),
      activeModule: state.activeModule?.id || '',
      hasRoot: Boolean(root),
      hasRow: Boolean(row),
      hasApprovalCard: Boolean(dataApprovalCard),
      hasApproveButton: Boolean(dataApprovalCard?.querySelector('[data-approve-approval]')),
      timelineText: timelineText.slice(0, 1200),
      contextText: contextText.slice(0, 600),
    };
  }, 30000, 'threads right-click hub render');

  const approvalButton = document.querySelector(
    `[data-approval-id="${css(projections.dataApproval.id)}"] [data-approve-approval]`,
  );
  if (!approvalButton) throw new Error('threads right-click approval button disappeared before reviewer decision');
  approvalButton.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
  const decisionUpdatedAfterMs = Date.now() - 5 * 60 * 1000;
  let approvalDecisionAttempt = 0;
  const approvalDecision = await waitFor(async () => {
    const approvalDocs = docsToJson(await rawDb.ctox_task_approval_requests.find({
      selector: {
        id: projections.dataApproval.id,
        updated_at_ms: { $gte: decisionUpdatedAfterMs + approvalDecisionAttempt++ },
      },
      limit: 1,
    }).exec());
    const approval = approvalDocs.find((item) => item.id === projections.dataApproval.id) || null;
    const approvedCommandId = approval?.approved_command_id || '';
    const commandDocs = approvedCommandId
      ? docsToJson(await rawDb.business_commands.find({
        selector: {
          id: approvedCommandId,
          updated_at_ms: { $gte: decisionUpdatedAfterMs + approvalDecisionAttempt },
        },
        limit: 1,
      }).exec())
      : [];
    const approvedCommand = commandDocs.find((item) => (
      item.command_id === approvedCommandId || item.id === approvedCommandId
    )) || null;
    const approvalLink = approvedCommand?.payload?.approval?.approval_request_id
      || approvedCommand?.client_context?.approval_request_id
      || '';
    return {
      ok: Boolean(
        approval?.status === 'approved'
        && approvedCommandId
        && approvedCommand
        && approvedCommand.status !== 'failed'
        && approvalLink === projections.dataApproval.id
      ),
      approval,
      approvedCommand,
      approvalLink,
    };
  }, 60000, 'threads reviewer decision and approved target reauthorization');

  const status = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
    includeCounts: false,
    requiredCollections: [
      'business_commands',
      'business_users',
      'user_threads',
      'user_thread_messages',
      'user_notifications',
      'ctox_task_approval_requests',
    ],
  });
  await globalThis.__ctoxRecordThreadsStatus(status);
  if (status?.version !== 'business-os-advanced-status-v1') {
    throw new Error('threads right-click UI smoke lost advanced status evidence; see threads-reviewer-result-status.json');
  }
  const requiredInitialSyncEntries = Array.isArray(status?.sync?.initialSync?.entries)
    ? status.sync.initialSync.entries
    : [];
  const incompleteInitialSync = requiredInitialSyncEntries.filter((entry) => (
    entry?.state !== 'complete'
    || !entry?.initialReplicationAt
    || entry?.checkpointEpochAdvertised !== true
    || !entry?.checkpointEpoch
  ));
  const missingRequiredCollections = Array.isArray(status?.sync?.missingRequiredCollections)
    ? status.sync.missingRequiredCollections
    : [];
  const unhealthyFrameCollections = Array.isArray(status?.sync?.frameTransport?.unhealthyCollections)
    ? status.sync.frameTransport.unhealthyCollections
    : [];
  if (
    status.ok !== true
    || Number(status.health?.errorTotal || 0) !== 0
    || missingRequiredCollections.length
    || incompleteInitialSync.length
    || unhealthyFrameCollections.length
  ) {
    throw new Error(`threads right-click UI smoke advanced status target collections unhealthy: ${JSON.stringify({
      ok: status.ok,
      health: status.health || null,
      missingRequiredCollections,
      incompleteInitialSync,
      unhealthyFrameCollections,
      requiredCollections: status.sync?.requiredCollections || null,
    }, null, 2)}`);
  }


  return { projections, rendered, approvalDecision, status, authenticatedReviewer: state.session.user };
}

module.exports = { runThreadsRightClickPeers, runRequesterInBrowser, runReviewerInBrowser, openContextTargetInBrowser };
