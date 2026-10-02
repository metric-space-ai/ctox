// Test-only shell host. Production still receives its db and sync from the shell.
import { mount } from "../modules/ctox/index.js";
const params = new URLSearchParams(location.search);
let mode = params.get("state") || "idle";
let hasCachedTask = mode === "working";
const listeners = new Set(),
  readinessListeners = new Set();
const task = {
  id: "fixture-task",
  title: "Bestehende Crew Darstellung prüfen",
  prompt: "Den vorhandenen Ablauf prüfen.",
  source_module: "ctox",
  command_type: "ctox.task.create",
  status: "running",
  route_status: "running",
  leased_at: new Date(Date.now() - 180000).toISOString(),
  updated_at_ms: Date.now(),
  execution_progress: {
    version: 1,
    revision: 1,
    phase: "work",
    percent: 50,
    current_step: 2,
    completed_steps: 1,
    total_steps: 2,
    steps: [
      {
        position: 1,
        label: "Bestand ansehen",
        status: "completed",
        activity_turns: 1,
      },
      {
        position: 2,
        label: "Zustände prüfen",
        status: "in_progress",
        activity_turns: 1,
      },
    ],
    review: { status: "pending" },
    activity_turns: { total: 2, thinking: 1, tools: 1, last_kind: "tool" },
    updated_at_ms: Date.now(),
  },
};
function collection(name) {
  return {
    $: {
      subscribe(fn) {
        listeners.add(fn);
        return {
          unsubscribe() {
            listeners.delete(fn);
          },
        };
      },
    },
    find() {
      return {
        limit() {
          return this;
        },
        async exec() {
          if (mode === "loading") return new Promise(() => {});
          if (mode === "error" && name === "ctox_queue_tasks")
            throw new Error("Fixture: IndexedDB read failed");
          return name === "ctox_queue_tasks" && hasCachedTask
            ? [{ toJSON: () => task }]
            : [];
        },
      };
    },
    findOne() {
      return {
        async exec() {
          return {
            toJSON: () => ({
              id: "runtime-settings",
              harness_flow: { ok: true, mode: "rxdb-webrtc", events: [] },
            }),
          };
        },
      };
    },
  };
}
const ctx = {
  host: document.querySelector("[data-module-content]"),
  locale: params.get("lang") || "de",
  args: {},
  session: { user: { id: "fixture-admin", role: "admin" } },
  // The shell's drawer API: the task detail opens in the left module pane.
  openLeftDrawer(content) {
    const pane = document.querySelector(".shell-window-module-pane--left");
    pane.replaceChildren(content);
    pane.dataset.fixtureDrawer = "open";
  },
  closeDrawers() {
    const pane = document.querySelector(".shell-window-module-pane--left");
    pane.replaceChildren();
    delete pane.dataset.fixtureDrawer;
  },
  db: { collection: (name) => (mode === "missing" ? null : collection(name)) },
  sync: {
    mode: "webrtc",
    collectionReadiness: () => ({
      ready: !["offline", "loading", "missing"].includes(mode),
      state: ["offline", "missing"].includes(mode)
        ? "offline-pending"
        : mode === "loading"
          ? "catching-up"
          : "live",
    }),
    subscribeCollectionReadiness(name, fn) {
      readinessListeners.add(fn);
      return () => readinessListeners.delete(fn);
    },
  },
};
window.fixtureSetState = (next) => {
  mode = next;
  if (next === "working" || next === "idle") hasCachedTask = next === "working";
  for (const fn of readinessListeners) fn();
  for (const fn of listeners) fn();
};
window.fixtureDispose = await mount(ctx);
window.fixtureReady = true;
