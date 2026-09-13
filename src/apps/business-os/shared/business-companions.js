export function createBusinessCompanionScheduler({
  loadBusinessReporterModule,
  loadBusinessChatModule,
  getSession,
  findCtoxModule = () => null,
  registerModuleSchemas = async () => {},
  createReporterContext = () => ({}),
  createChatContext = () => ({}),
  onError = () => {},
  onSchemaError = () => {},
}) {
  if (typeof loadBusinessReporterModule !== 'function') throw new TypeError('loadBusinessReporterModule must be a function');
  if (typeof loadBusinessChatModule !== 'function') throw new TypeError('loadBusinessChatModule must be a function');
  if (typeof getSession !== 'function') throw new TypeError('getSession must be a function');

  let activeRun = null;

  return Object.freeze({
    cancel() {
      activeRun = null;
    },
    schedule() {
      const session = getSession();
      if (!session?.authenticated) return null;

      // Scheduling replaces any previous run. The session is captured because
      // state.session is mutable and these imports/schema work is deferred.
      const run = { session };
      activeRun = run;
      const isActive = () => (
        activeRun === run
        && getSession() === session
        && session.authenticated === true
      );

      loadBusinessReporterModule()
        .then((module) => {
          if (!isActive()) return;
          module.initBusinessReporter({
            session,
            ...createReporterContext(session, module),
          });
        })
        .catch(onError);

      loadBusinessChatModule()
        .then(async (module) => {
          if (!isActive()) return;

          // The crew bar needs CTOX schemas before its first pool load, but the
          // awaited registration must not authorize a replaced logged-out run.
          const ctoxModule = findCtoxModule();
          if (ctoxModule) {
            try {
              await registerModuleSchemas(ctoxModule);
            } catch (error) {
              onSchemaError(error);
            }
          }

          if (!isActive()) return;
          module.initBusinessChat({
            session,
            ...createChatContext(session, module),
          });
        })
        .catch(onError);

      return run;
    },
  });
}
