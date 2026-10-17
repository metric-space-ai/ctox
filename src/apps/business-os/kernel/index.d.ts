// CTOX App-API 4.0 – Typpaket (Quelle für ctox-app-api.d.ts, §16.10).
// Vertrag: ctox-sync-v4-architektur.md §10.2, §10.2a, §16.7, §16.10, §16.15, §16.19.
// Teil 1: normative App-API. Teil 2: Shell-Kernel-API (nur Shell). Teil 3: Stub-Kernel + Sim-Hooks (nur Stub).

// ============================================================================================================
// Teil 1 – App-API (an jede App übergeben, app_api_ver = 4.0)
// ============================================================================================================

/** E47/E47.1: read-only view metadata. put/patch discard only _rev/_pending; CAS uses expected_revision explicitly. */
export type ReadDocument<T> = T & {readonly _rev: string | null; readonly _pending: boolean};

/** Live-Ergebnis einer Abfrage. Ergebnisse sortiert, Gleichstände nach `id`; ohne Zeilendeckel. */
export interface LiveQuery<T> {
  get(): T[];
  /** Ruft `cb` sofort mit dem aktuellen Stand und danach bei jeder Änderung; liefert die Abmeldung. */
  subscribe(cb: (rows: T[]) => void): () => void;
}

/** Live-Einzelwert. */
export interface LiveDoc<T> {
  get(): T;
  subscribe(cb: (value: T) => void): () => void;
}

/** Mango-Teilmenge (§10.2a Nr. 5). Fehlendes Feld ≠ null; `$exists:false` trifft fehlende Felder; `$eq:null` nur null. */
export type Query = {
  selector?: Selector;
  /** Mehrere Felder, z. B. `[{prio: 'desc'}, {title: 'asc'}]`. Gleichstand → `id` aufsteigend. */
  sort?: Array<Record<string, 'asc' | 'desc'>>;
  limit?: number;
  skip?: number;
};
export type Selector = {
  $and?: Selector[];
  $or?: Selector[];
  $not?: Selector;
  [field: string]: FieldCondition | Selector[] | Selector | undefined;
};
export type Scalar = string | number | boolean | null;
export type FieldCondition =
  | Scalar
  | RegExp
  | unknown[]
  | { [key: string]: unknown } // Gleichheit mit einem Objekt
  | FieldOperators;
export interface FieldOperators {
  $eq?: unknown;
  $ne?: unknown;
  $gt?: number | string;
  $gte?: number | string;
  $lt?: number | string;
  $lte?: number | string;
  $in?: unknown[];
  $nin?: unknown[];
  $exists?: boolean;
  /** JS-Regex; einziges erlaubtes Flag: `i` (über `$options: 'i'` oder `/x/i`). */
  $regex?: string | RegExp;
  $options?: 'i' | '';
  $not?: FieldOperators | RegExp;
  /** Eine Ebene: Selektor für Objekt-Elemente oder Operatoren für skalare Elemente. */
  $elemMatch?: Selector | FieldOperators;
}

export type WriteResult = {
  /** Full locally acknowledged version and canonical replacement retained atomically in archive_local. */
  local_archived_ref?: string;
  status: 'accepted' | 'merged' | 'rejected' | 'migrated';
  code?: CtoxErrorCode;
  gen?: number;
  client_seq?: string;
  /** Receipt alone cannot clear the outbox; wait for the canonical page. */
  requires_page?: boolean;
  rev?: string;
  reason?: string;
  archived_ref?: string;
  /** Stub/Draht: globale Folgenummer der bestätigten Änderung (§2.2). */
  seq?: string;
};

export interface WriteHandle<Extra = {}> {
  /** Stub: Vorgangs-ID (UUIDv7) des Outbox-Eintrags. */
  op_id?: string;
  /** Lokal in IDB committet. Lehnt mit `CtoxError{code:'invalid', path}` ab, wenn das Dokument ungültig ist – dann wird nichts geschrieben. */
  local: Promise<void>;
  /** Core-Ergebnis. Lehnt nie ab; bleibt offen, solange offline. */
  result: Promise<WriteResult & Extra>;
  /** Nur vor dem Senden möglich; `true` = zurückgenommen (result → rejected/cancelled). */
  cancel(): boolean;
}

export type CommandState = {
  status: 'queued' | 'accepted' | 'running' | 'done' | 'failed' | 'cancelled' | 'rejected' | 'submit_failed';
  progress?: number;
  result?: unknown;
  error?: string | { code: string; reason: string };
  local_inserted?: false;
};
/** submit_failed means local persistence failed; no command was submitted. */
export interface CommandHandle {
  /** ID des Befehlsdokuments in `business_commands`. */
  id: string;
  /** Gepushter Status, kein Warten auf Annahme. */
  status: LiveDoc<CommandState>;
  local: Promise<void>;
  result: Promise<WriteResult>;
  cancel(): boolean;
}

export type CtoxErrorCode = 'revision_conflict' | 'invalid' | 'forbidden' | 'not_found' | 'conflict' | 'revoked' | 'expired' | 'quota' | 'unavailable' | 'stale_base' | 'migration_required' | 'locally_modified' | 'token_expired' | 'token_used';
export declare class CtoxError extends Error {
  readonly name: 'CtoxError';
  readonly code: CtoxErrorCode;
  readonly path?: string;
  readonly reason?: string;
  readonly detail?: string;
  constructor(code: CtoxErrorCode, message?: string, extra?: { path?: string; reason?: string; detail?: string });
}

export type StatusItem = { level: 'info' | 'warn' | 'error'; code: string; text: string; ref?: string };

/** Unix epoch milliseconds, first observation of each stage; null = not observed.
 * queued = local durable queue, sent = first transport dispatch, acked = validated
 * Core receipt (including rejection), completed = canonical terminal state or rejection.
 * Observational per-tab ring, creation order, at most 500; not durable or a completion authority.
 */
export type CommandTiming = Readonly<{command_id:string; queued:number; sent:number|null; acked:number|null; completed:number|null}>;
/** Bridge adapter hook: forward timing snapshots here, never command payloads.
 * Exceptions are isolated from the data path. Cross-tab updates can repeat; key by command_id.
 */
export interface CommandDiagnostics { onCommandTiming?(timing:CommandTiming):void | Promise<void>; }
export interface CommandMetrics extends LiveDoc<Metrics> { readonly commands:ReadonlyArray<CommandTiming>; }
export type Metrics = {
  commands: ReadonlyArray<CommandTiming>;
  timing?: {push_latency_ms:number;clock_offset_ms:number;clock_uncertainty_ms:number;samples:number};
  backlog: number;
  outbox: { count: number; oldest_ms: number };
  /** §16.19 Nr. 7: state ∈ online | offline. */
  link: { state: 'online' | 'offline'; rtt_ms: number; relay: boolean };
  /**
   * §16.45 Collection-Liveness je abonnierter Collection: live = Verbindung oben und Backlog 0 nach vollständiger Seite;
   * catching_up = Backlog > 0 oder Seite unvollständig; offline = Verbindung weg (lokale Daten lesbar); error mit Code (§16.42).
   * Die Shell zeigt grün nur, wenn alle angeforderten Collections live sind.
   */
  collections: Record<string, CollectionLiveness>;
};
export type CollectionLiveness = {
  state: 'live' | 'catching_up' | 'offline' | 'error';
  backlog: number;
  gen: number;
  /** Zeitpunkt des letzten Zustandswechsels (ms seit Epoche). */
  since_ms: number;
  error?: { code: string; message: string };
};

export interface RouteEvent { from: string; to: string; params: Record<string, unknown> }

/** §10.2a Nr. 2 – gehört der Shell, versioniert mit app_api_ver. */
export interface ShellApi {
  setTitle(title: string): void;
  /** §16.19 Nr. 5: App rendert ihr Kopfzeilen-Element, die Shell setzt es in den Kopf-Slot. null entfernt es. */
  registerHeader(el: HTMLElement | null): void;
  /** Unbekanntes Ziel → Fehler in der Zustandszone, `false`, nie stiller Fallback. */
  /** Read-only: no navigation or status entry is emitted. */
  canNavigate(appId: string, params?: Record<string, unknown>): boolean;
  /** E48: ref grants one files.get to target, bound to this user/session for at most one hour; audited before routing. */
  navigate(appId: string, params: Record<string, unknown> & {ref: string | {id?: string; ref?: string}}): Promise<boolean>;
  navigate(appId: string, params?: Record<string, unknown>): boolean;
  onRoute(cb: (ev: RouteEvent) => void): () => void;
  locale: LiveDoc<string>;
  t(key: string): string;
  theme: LiveDoc<Record<string, string>>;
  /** Nur innerhalb des App-Fensters. */
  dialog(spec: unknown): Promise<unknown>;
  menu(spec: unknown): Promise<unknown>;
  window: {
    size: LiveDoc<{ width: number; height: number }>;
    mode: LiveDoc<string>;
    close(): void;
  };
  openFile(ref: string): void;
}

/** §10.2a Nr. 3 – nur Spiegel, Entscheidung im Core. */
export interface Actor {
  readonly id: string;
  readonly display_name: string;
  readonly role: string;
  permissions: LiveDoc<string[]>;
}

export type FileMeta = { id: string; name: string; mime: string; size: number; ref: string; owner_app: string; state?: 'rejected' };
export interface FileUploadHandle extends WriteHandle<{ ref: string }> {
  /** sha256 des Inhalts (hex). */
  ref: Promise<string>;
  /** Resolves on Core acceptance + upload; rejects with CtoxError {code,reason}; rejected file metadata remains locally. */
  done: Promise<WriteResult & { ref: string }>;
  upload: { progress: LiveDoc<{ sent: number; total: number }> };
}
/** §10.2a Nr. 4 + §16.10. Metadaten = Dokumente der Collection `files`. */
export type FileInfo = { id: string; ref: string; name: string | null; mime: string; size: number };
/** CAP-3b (#844): Ergebnis von files.get – Text nur für UTF-8 ≤ 512 KiB, sonst Bytes; nie Base64. */
export type FileGetResult = ({ found: true } & FileInfo & ({ text: string } | { bytes: Uint8Array })) | { found: false; ref: string | null; id?: string };
export interface FilesApi {
  /** E30: own file, readable desktop_files row with the same id (including read export), or BlobRef in a readable canonical document; otherwise forbidden. list/delete remain own-app only. */
  /** E26: byte range, end exclusive. Invalid bounds reject with CtoxError invalid/path range. */
  get(sel: ({ ref: string } | { id: string }) & { range: { start: number; end: number } }): Promise<({ found: true } & FileInfo & { bytes: Uint8Array }) | { found: false; ref: string | null; id?: string }>;
  get(sel: ({ ref: string } | { id: string }) & { range?: { start: number; end: number } }): Promise<FileGetResult>;
  /** @deprecated Altform (K-1): Blob aus dem Cache bzw. über Spur 4. */
  get(ref: string): Promise<Blob>;
  /** CAP-3b: put(bytes | {text}, {name, mime}) → {id, ref} nach lokalem Commit; Upload über Spur 4. */
  put(content: Uint8Array | ArrayBuffer | { text: string }, meta?: { id?: string; name?: string; mime?: string }): Promise<{ id: string; ref: string; done: FileUploadHandle['done'] }>;
  /** @deprecated Altform (K-1): Blob mit Upload-Handle. */
  put(blob: Blob, meta?: { id?: string; name?: string; mime?: string }): FileUploadHandle;
  list(): Promise<{ files: FileInfo[] }>;
  delete(sel: { ref: string } | { id: string }): Promise<{ deleted: boolean; id?: string }>;
}
/** #830 / CATALOG §6: Responses-förmige Antwort von model.run. */
export type ModelResponse = {
  id: string; object: 'response'; model: string; status: 'completed' | 'failed';
  output: Array<{ type: 'message'; role: 'assistant'; content: Array<{ type: 'output_text'; text: string }> }>;
  output_text: string; usage?: { input_tokens: number; output_tokens: number };
};
export interface ModelApi {
  /** Nur mit `model` in capabilities_required; Fake-Core deterministisch, echter Weg über die App-Laufzeit (#830). */
  run(req: { model?: string; input?: string | Array<{ role: string; content: string | Array<{ type: string; text?: string }> }>; prompt?: string; [k: string]: unknown }): Promise<ModelResponse>;
}

/** E51: identity is bound to the caller. Limits: 64 keys, 16 KiB JSON, key [a-z0-9._-]{1,64}. */
export interface PreferencesApi {
  get<T = unknown>(key: string): T | null;
  set(key: string, value: unknown): WriteHandle;
  delete(key: string): WriteHandle;
}
export interface CtoxAppApi {
  version: '4.0';
  app: { id: string; ver: string };
  // Lesen – nur lokal, nie Netzwerk, keine Lesesperre
  query<T = any>(collection: string, q: Query): LiveQuery<ReadDocument<T>>;
  get<T = any>(collection: string, id: string): LiveDoc<ReadDocument<T> | null>;
  // Schreiben – lokal sofort, Outbox
  put<T>(collection: string, doc: T & { id: string }): WriteHandle;
  /** E35: Core writer transaction compares expected_revision; mismatch resolves result as rejected/code revision_conflict. */
  patch(collection: string, id: string, patch: object, options?: { expected_revision?: string }): WriteHandle;
  remove(collection: string, id: string): WriteHandle;
  // Befehle – Dokument mit gepushtem Status
  /** opts.record_id (§16.19 Nr. 8): optionaler Bezug des Befehls auf einen Datensatz. */
  /** E26: idempotency_key ≤128 characters per app + actor; SHA-256(app|user|key) → UUIDv8; repeat reuses stored intent/state. */
  /** opts.on_complete/on_failed: Rückruf-Befehlstypen der eigenen App (§16.39 Nr. 3), als Felder am Befehlsdokument.
   *  §16.44: ctox.coding.turn {app_id, draft_id, base_draft_rev, prompt, model?, turn_id?}, ctox.coding.cancel {turn_id};
   *  Ablehnung im write_result {status:'rejected', code, reason}. */
  command(type: string, payload: object, opts?: { record_id?: string; on_complete?: string; on_failed?: string; idempotency_key?: string; target_app?: string }): CommandHandle;
  // Priorität (E1) – reiner Hinweis
  setVisible(collections: string[]): void;
  // Zustand – nur an die Zustandszone der Shell (Anf. 27)
  status: {
    report(item: StatusItem): void;
    clear(code: string): void;
  };
  // Messwerte (lesend, für die Zustandszone; Apps zeigen sie nicht selbst). Leser-Tabs: link = Zustand des Leiters.
  metrics: CommandMetrics;
  shell: ShellApi;
  actor: Actor;
  prefs: PreferencesApi;
  files: FilesApi;
  model: ModelApi;
}

/** §10.2a Nr. 1 – jede App exportiert mount(). */
export interface AppMountHandle {
  unmount(): void | Promise<void>;
  /** structured-clone-fähig, ≤ 1 MiB. */
  snapshot?(): unknown;
}
export type AppMount = (root: HTMLElement, api: CtoxAppApi, ctx: { restored?: unknown }) => AppMountHandle | Promise<AppMountHandle>;
export interface AppModule { mount: AppMount }

/** §16.15 – die Shell exportiert startShell/stopShell; der Loader tauscht damit live. */
export interface ShellModule {
  startShell(kernel: StubKernel, opts: { restored?: unknown }): void | Promise<void>;
  stopShell(): unknown | Promise<unknown>;
}

// ============================================================================================================
// Teil 2 – Shell-Kernel-API (§10.2a Nr. 6–8, §16.15) – nur für die Shell
// ============================================================================================================

export type ActivationState = 'received' | 'fetched' | 'data-ready' | 'activated' | 'failed';
/** §16.19 Nr. 6 – fehlende Felder sind null. */
export type AppLayout = {
  icon_asset: string | null;
  icon_svg: string | null;
  category: string | null;
  frame_palette: string | null;
  default_width: number | null;
  default_height: number | null;
  min_width: number | null;
  min_height: number | null;
  presentation: string | null;
};
export type AppEntry = {
  id: string;
  title: string;
  /** vom Core veröffentlicht */
  active_ver: string;
  /** lokal aktiviert (läuft) */
  activated_ver: string | null;
  /** Zustand der Aktivierung von target_ver */
  state: ActivationState;
  target_ver: string | null;
  error: string | null;
  layout: AppLayout;
  styles: string[];
};
export type Activation = {
  app_id: string;
  activated_ver: string | null;
  target_ver: string | null;
  previous_ver?: string | null;
  /** zuletzt erfolgreich aktivierte Versionen (für reportFailed). */
  good_vers?: string[];
  state: ActivationState | null;
  error: string | null;
  at_seq?: string;
  updated_at: number;
};
export type ActivationEvent = { app_id: string; ver: string | null; state: ActivationState; activated_ver: string | null; error: string | null };

export type AppManifest = {
  app_id: string;
  app_ver: string;
  files: Record<string, string>; // Pfad → sha256
  entry: string;
  collections: Record<string, { schema?: object; class?: string; client_limit?: number; byte_budget?: number; rights?: unknown }>;
  /** §16.4: Generation je Collection (fehlt = 1); Namensraum `name@g<k>`. */
  generations: Record<string, number>;
  /** Stub: durch diese Version abgelöste Namensräume (werden nach generationGraceMs lokal verworfen, §16.16). */
  retires: Record<string, string>;
  /** §16.19 Nr. 6 */
  layout: AppLayout;
  styles: string[];
  handlers: Record<string, string>;
  min_app_api: string;
  capabilities_required: string[];
  provenance: { base_release: string | null; base_hash: string | null; locally_modified: boolean };
  author: string;
  created_at: string;
};

/** K-1b: Artefaktinhalt; Text bleibt UTF-8, Binärinhalt wird als Bytes gehasht und gespeichert. */
export type ArtifactContent = string | Blob | ArrayBuffer | ArrayBufferView;
/** §16.22 A9: Inhalt von collections.schema.json. */
export type CollectionsFile = {
  collections?: Record<string, object | { schema: object; indexes?: unknown[]; class?: string; rights?: unknown; client_limit?: number; byte_budget?: number }>;
  migrations?: Record<string, { from_ver: string; migrate_fn: string } | { from_ver: string; steps: MigrationStep[] }>;
};
/** Stub-Teilmenge deklarativer Migrationsschritte. */
export type MigrationStep =
  | { op: 'rename'; from: string; to: string }
  | { op: 'add'; field: string; value?: unknown }
  | { op: 'remove'; field: string }
  | { op: 'cast'; field: string; to: 'string' | 'number' | 'boolean' };
export type CollectionDiff = {
  name: string;
  kind: 'unchanged' | 'compatible' | 'new' | 'generation' | 'removed';
  reasons: string[];
  needsMigration: boolean;
  migration: { from_ver: string; migrate_fn?: string; steps?: MigrationStep[] } | null;
};
/** §16.22 A4. files: Pfad → Text, null = im Entwurf gelöscht. */
export interface DraftWriteHandle extends Promise<void> {
  local: Promise<void>;
  result: Promise<{ status: 'accepted'; rev: string } | { status: 'rejected'; code: string; reason?: string }>;
}
export type DraftInfo = { base_app_ver: string | null; files: Record<string, string | Uint8Array | null>; manifest: Record<string, unknown>; updated_at: number ; /** Bestätigte Revision der app_drafts-Zeile (= base_draft_rev für ctox.coding.turn, §16.44); null, solange Änderungen ausstehen oder der Entwurf neu und noch unbestätigt ist. */ rev: string | null };
/** §16.44: Plattform-Collection coding_turns/<turn_id> (schreibt nur der Core; messages/tool_events erst am Turn-Ende). */
export type CodingTurn = {
  id: string; app_id: string; draft_id: string; user_id: string; command_id: string; origin_app: string; base_draft_rev: string;
  prompt: string; model: string | null;
  state: 'queued' | 'running' | 'done' | 'failed' | 'cancelled' | 'conflict';
  messages: Array<{ role: 'user' | 'assistant'; text: string; at_ms: number }>;
  tool_events: Array<{ kind: string; path?: string; at_ms: number }>;
  draft_rev_after: string | null; changed_files: string[];
  patch?: { ref: string; file_id: string };
  error?: { status: 'failed'; code: string; reason: string; detail?: string };
  callback: { on_complete: string | null; on_failed: string | null };
  created_at_ms: number; updated_at_ms: number; finished_at_ms?: number;
};

/** §10.2a Nr. 7 + §16.22. Entwürfe je (App, Nutzer); im Kernel lokal in IDB meta `draft:<app>` (IDB je Instanz+Nutzer, §16.7). */
export interface SourceApi {
  /** Recht apps.source.view. Ohne `ver`: Basis des eigenen Entwurfs (sonst aktive Version) überlagert mit dem Entwurf. */
  list(ver?: string): Promise<string[]>;
  /** Recht apps.source.view. Ohne `ver`: Entwurfsfassung, sonst Basis. Gelöschte Datei → not_found. */
  read(path: string, ver?: string): Promise<string | Uint8Array>;
  draft: {
    /** A4 – null, wenn kein eigener Entwurf. */
    info(): Promise<DraftInfo | null>;
    /** Recht apps.source.edit (gilt für alle schreibenden Entwurfsaufrufe). `manifest.json` → invalid (§16.26). */
    /** §16.44-Ergänzung (3): {local, result}; result = vom Core bestätigte Revision {status:'accepted', rev} bzw. rejected{code};
     *  awaitbar wie bisher (wartet auf den lokalen Commit). */
    write(path: string, content: ArtifactContent): DraftWriteHandle;
    /** A7 – not_found, wenn die Datei nicht existiert. */
    remove(path: string): Promise<void>;
    /** A7 – not_found (Quelle fehlt), conflict (Ziel existiert). Verweise im Manifest (entry, handlers, styles) wandern mit (§16.26). */
    rename(from: string, to: string): Promise<void>;
    /** A5/§16.26 – einzige Quelle der Manifest-Felder im Entwurf (title, entry, handlers, …); null entfernt ein Feld bzw. einen Handler. */
    setManifest(fields: Record<string, unknown>): Promise<void>;
    /** A6 – vollständiger Inhalt von collections.schema.json; liefert den Diff gegen die Entwurfsbasis. */
    setCollections(content: string | CollectionsFile): Promise<{ base_app_ver: string | null; diff: CollectionDiff[] }>;
    discard(): Promise<void>;
    /** Basis des Entwurfs umsetzen (Änderungen bleiben). */
    rebase(ver: string | null): Promise<void>;
    /**
     * A2/A3 – Vergleich-und-Setzen gegen die aktive Version; base_app_ver = null legt eine neue App an; ohne Angabe gilt
     * die Basis des Entwurfs. result.rev = neue Version. Gründe: forbidden | stale_base | invalid (kein busy: eine laufende Veröffentlichung wird abgewartet, K-1c; detail z. B.
     * „Migration fehlt: <coll>“ oder „Verweis auf fehlende Datei: …“, §16.26). Bei Ablehnung bleibt der Entwurf erhalten.
     */
    publish(opts?: { base_app_ver?: string | null }): WriteHandle<{ diff?: Array<{ name: string; kind: CollectionDiff['kind'] }> }>;
  };
}

/** @deprecated Alter Name (66edb98) für StatusEntry / StatusStream. */
export type StatusZoneEntry = StatusEntry;
/** @deprecated */
export type StatusZone = StatusStream;
export type StatusEntry = Readonly<StatusItem & { app_id: string; ref: string | null; source: 'app' | 'kernel' | 'shell'; at: number }>;
/** §16.19 Nr. 1: Strom der Zustandseinträge; rendern macht die Shell. Gleicher (app_id, code) ersetzt. */
export interface StatusStream extends LiveQuery<StatusEntry> {
  forApp(appId: string): LiveQuery<StatusEntry>;
  /** Eigene Einträge der Shell (source 'shell'). */
  report(appId: string, item: StatusItem): void;
  clear(appId: string, code: string): void;
}

/** §16.19 Nr. 3 – je Tab und Nutzer, IDB window_state. Werte structured-clone-fähig. */
export interface WindowStateApi {
  get<T = unknown>(key: string): Promise<T | null>;
  put(key: string, value: unknown): Promise<void>;
  delete(key: string): Promise<void>;
}

export interface WindowState { title: string; header: HTMLElement | null; size: { width: number; height: number }; mode: string; closed: boolean }

export interface ShellKernelApi {
  apps: LiveQuery<AppEntry>;
  activation: {
    get(appId: string): LiveDoc<Activation | null>;
    onChange(cb: (ev: ActivationEvent) => void): () => void;
    /** §16.19 Nr. 2: Version für diesen Client failed, Zeiger zurück auf die letzte funktionierende, app_activation failed. */
    reportFailed(appId: string, ver: string, reason: string): Promise<Activation>;
  };
  artifacts: {
    read(hash: string): Promise<Blob>;
    manifest(appId: string, ver: string): Promise<AppManifest | null>;
    /** §16.39 Nr. 1 (A6), synchron: /~a/<app>@<ver>~shell@<sv>~lib:<name>@<M.m>…/<path>; sv = opts.shellVer, sonst aktuell aktive Shell.
     *  lib:<name> mit "<M.m>" → /~a/lib:<name>@<M.m>[~lib:…]/<path>; Shell: /~a/shell@<sv>~lib:…/<path>. Merkt sich nichts je app@ver. */
    url(appId: string, ver: string, path: string, opts?: { shellVer?: string; libs?: Record<string, string> }): string;
    /** §16.39 Bindung je MOUNT: beim Mount einmal aufrufen; eingefroren (Shell-Version + Lib-Minors), url(path) synchron. */
    bind(appId: string, ver: string, opts?: { shellVer?: string; libs?: Record<string, string> }): ArtifactBinding;
  };
  /** §16.39 Nr. 2: SW-Nachrichten; im Browser automatisch an navigator.serviceWorker gebunden. */
  libs: {
    /** §16.50: bedient lib-miss unabhängig vom auslösenden Client (Fenster/Worker). Antwort über den Port:
     *  {type:'ctox:lib-ready', lib, ver, path, sha, mime, size, body: ArrayBuffer} (sha256-geprüft) oder {type:'ctox:lib-abort', reason}
     *  (hash_mismatch | not_found | manifest | invalid | artifact_missing | timeout – vor den 15 s des SW). */
    onSwMessage(data: { type: 'ctox:lib-miss'; lib: string; major: string; path: string; ver: string; sha?: string | null; reason?: string; client_id?: string; client_type?: string } | { type: 'ctox:lib-hit'; hits: Array<{ lib: string; major: string; ver: string; path: string; sha?: string; n?: number; last_ms?: number }> }, port?: { postMessage(m: unknown): void }): Promise<unknown>;
    resolve(name: string, major: string | number): string | null;
    enforceBudget(): Promise<string[]>;
    readonly byteBudget: number;
  };
  /** K-1c Punkt 7, §16.29 B / §16.33: App-Store-Wege. Ablehnung im write_result: stale_base | migration_required | locally_modified | forbidden | not_found. */
  publishApp(appId: string, opts: { base_app_ver: string | null; draft_id?: string }): CommandHandle;
  updateApp(appId: string, opts: { base_app_ver: string | null; release_ver: string; discard_local?: boolean }): CommandHandle;
  rollbackApp(appId: string, opts: { base_app_ver: string; target_ver: string }): CommandHandle;
  releaseApp(appId: string, opts: { base_app_ver: string; channel?: string }): CommandHandle;
  removeApp(appId: string, opts: { base_app_ver: string }): CommandHandle;
  source(appId: string): SourceApi;
  metrics: CommandMetrics;
  leader: LiveDoc<{ isLeader: boolean; leader_gen: number; tab_id: string }>;
  identity: {
    readonly instance_id: string;
    readonly user_id: string;
    /** §16.43: eigene device_pairing_id („Dieses Gerät“); null, wenn der Bootstrap keine Gerätebindung liefert. */
    readonly device_id: string | null;
    /** Schließt DB, beendet Callbacks, sperrt Zugriff. Löschen nur mit deleteData:true. */
    logout(opts?: { deleteData?: boolean }): Promise<void>;
    /** Schließt diesen Kernel und liefert einen Kernel für (Instanz, Nutzer). */
    switchInstance(next: { instanceId?: string; userId?: string }): Promise<StubKernel>;
  };
  status: StatusStream;
  /** @deprecated Alias für `status` (seit §16.19). */
  statusZone: StatusStream;
  /** §16.19 Nr. 3 */
  windowState: WindowStateApi;
  /** Stub-Komfort: Fensterzustand je App (Titel/Header kommen aus api.shell). */
  window(appId: string): {
    state: LiveDoc<WindowState>;
    setSize(size: { width: number; height: number }): void;
    setMode(mode: string): void;
    reopen(): void;
  };
}

export declare function createShellKernel(kernel: StubKernel): ShellKernelApi;

// ============================================================================================================
// Teil 3 – Stub-Kernel und Sim-Hooks (nur Stub, kein Vertrag)
// ============================================================================================================

export interface StubKernelOptions {
  diagnostics?: CommandDiagnostics;
  instanceId?: string;
  userId?: string;
  /** Standard: `ctox-<instanceId>-<userId>` (§16.7). */
  dbName?: string;
  /** Simulierte Core-Latenz je Schritt (Standard 50). */
  latencyMs?: number;
  /** 'indexeddb' | 'memory'; Standard: IndexedDB, wenn vorhanden. */
  storage?: 'indexeddb' | 'memory';
  /** Native Fähigkeiten dieses Teilnehmers (für capabilities_required → needs_update). */
  capabilities?: string[];
  actor?: Partial<{ id: string; display_name: string; role: string; permissions: string[] }>;
  locale?: string;
  theme?: Record<string, string>;
  /** Startfrist der neuen Shell bei swapShell (Standard 9000, §16.19 Nr. 4). */
  shellStartTimeoutMs?: number;
  /** Alte Generation lokal behalten, bis die neue Version so lange ohne Rückfall lief (Standard 600000, §16.16). */
  generationGraceMs?: number;
  /** Tab-Identität für windowState (Standard: sessionStorage, sonst je Kernel neu). */
  tabKey?: string;
  /** K-1c Punkt 4: ein Fake-Core je Instanz für mehrere Nutzer (eigene DB je Nutzer); der Core setzt Rechte durch. */
  coreNamespace?: string;
  /** Nutzer, wie der Core ihn kennt (Rolle/Rechte überstimmen die Behauptung des Clients). */
  coreUser?: Partial<{ role: string; permissions: string[] }>;
  /** Takt der every-Trigger im Fake-Core (Test), Standard: Manifestwert, mindestens 60 s. */
  everyMs?: number;
  /** Laufzeit eines Fake-Harness-Agentenlaufs (Standard 100 ms). */
  agentDelayMs?: number;
  /** §16.37: Byte-Budget der Bibliotheksdateien (LRU), Standard 512 MiB. */
  libByteBudget?: number;
  /** §16.43: device_pairing_id aus der Gerätebindung (dieselbe id wie im hello device_proof). */
  deviceId?: string;
  /** Fake-Core: Gerätekennung für die deterministische device_id (Standard 'browser'). */
  deviceLabel?: string;
  /** Fake-Core: Laufzeit des simulierten Coding-Agent-Turns (§16.44, Standard 60 ms). */
  codingDelayMs?: number;
  /** false = Ein-Tab-Betrieb erzwingen. */
  multiTab?: boolean;
}

export interface PublishOptions {
  /** Pfad → Inhalt (UTF-8). In-Memory-Module registriert testkit/app-host.mjs, nicht der Kernel. */
  files?: Record<string, ArtifactContent>;
  entry?: string;
  /** Weitere Manifest-Felder (triggers, command_rights, libs, …). */
  manifestExtra?: Record<string, unknown>;
  title?: string;
  layout?: Partial<AppLayout>;
  styles?: string[];
  collections?: AppManifest['collections'];
  /** Wasserstand; Standard = aktueller Core-Kopf. Aktivierung wartet auf Checkpoint ≥ migratedAtSeq. */
  migratedAtSeq?: number;
  /** §16.4: diese Collections bekommen eine neue Generation `name@g<k+1>`. */
  incompatible?: string[];
  /** Migrationsfunktion je Collection für die neue Generation (Standard: Identität). Nur im Leiter-Tab. */
  migrate?: Record<string, (doc: any) => any>;
  capabilitiesRequired?: string[];
  minAppApi?: string;
}

export interface StubSim {
  scopeGrants():AppScopeControlMessage[];
  failAppGrant(appId:string):void;
  /** Nächster bestätigter Write wird abgelehnt. */
  rejectNext(reason?: string | {code: CtoxErrorCode; reason: string}): Promise<void>;
  /** Test Core: terminal CAS rejection, independent of the metadata write receipt. */
  rejectNextUpload(reason?: {code: CtoxErrorCode; reason: string}): Promise<void>;
  /** Nächster bestätigter Write wird `migrated`; `dropFields` wandern ins Core-Archiv. */
  migrateNext(opts?: { dropFields?: string[] }): Promise<void>;
  /** Nächster Befehl dieses Typs endet `failed`. */
  failCommand(type: string, error?: string): Promise<void>;
  /** Ergebnis-Handler für Befehle (läuft im Leiter-Tab). */
  onCommand(type: string, fn: (payload: any, ctx: { app_id: string; command_id: string }) => unknown): void;
  /** Live-Änderung aus dem Core; `{id, _deleted: true}` = Löschung. Liefert vergebene seq. */
  push(collection: string, docs: Array<{ id: string; _deleted?: boolean; [k: string]: unknown }> | { id: string; [k: string]: unknown }): Promise<number[]>;
  /** state 'offline' hält Senden und Zustellung an (Rückstand wächst). */
  setLink(link: Partial<{ state: 'online' | 'offline'; rtt_ms: number; relay: boolean }>): Promise<Metrics['link']>;
  setBacklog(n: number): Promise<void>;
  /** Veröffentlichen + Aktivieren (received → fetched → data-ready → activated|failed). Liefert den Endzustand. */
  publishAppVersion(appId: string, ver: string, opts?: PublishOptions): Promise<Activation | null>;
  /** §16.15a Nr. 3: die nächsten n Artefakt-Abrufe liefern beschädigte Bytes. */
  corruptNextFetch(n?: number): Promise<void>;
  /** Abgelöste Generationen sofort lokal verwerfen (statt nach generationGraceMs). */
  retireGenerationsNow(appId: string): Promise<number | undefined>;
  /** Loader-Rolle: Shell starten. */
  startShell(shell: ShellModule, opts?: { restored?: unknown }): Promise<boolean>;
  /** stopShell → snapshot (window_state) → startShell(kernel, {restored}); Fehler/10 s → vorherige Shell. */
  swapShell(next: ShellModule): Promise<{ swapped: boolean; restored: unknown; error?: string }>;
  setPermissions(list: string[]): void;
  setActor(partial: Partial<{ id: string; display_name: string; role: string; permissions: string[] }>): void;
  setLocale(locale: string): void;
  setTranslations(locale: string, dict: Record<string, string>): void;
  setTheme(tokens: Record<string, string>): void;
  /** LRU-Räumung simulieren: unbestätigte (gepinnte) Blobs bleiben. */
  evictBlobs(): Promise<number>;
  /** Simuliert einen fremden Leiter (leader_gen +1) → nächste Leiteroperation wird verworfen. */
  bumpLeaderGen(): Promise<void>;
  dropBroadcasts(on?: boolean): void;
  /** Leser-Abgleich über meta.local_seq; liefert Zahl nachgeholter Dokumente. */
  reconcile(): Promise<number>;
  inspect(collection: string, id: string): Promise<any>;
  outbox(): Promise<any[]>;
  archiveLocal(): Promise<any[]>;
  windowStateRows(): Promise<any[]>;
  artifactRows(): Promise<any[]>;
  inspectMeta(key: string): Promise<any>;
  /** Fake-Core: Zeilen einer Collection im Core (auch nicht abonnierte wie _audit). */
  coreRows(collection: string): Promise<any[]>;
  /** Fake-Harness: Ergebnis der nächsten Agentenläufe. */
  agentOutcome(fn: ((task: unknown) => { status: 'done' | 'failed'; result?: unknown } | Promise<{ status: 'done' | 'failed'; result?: unknown }>) | null): void;
  coreLog(): Array<{ app_id: string; level: string; msg: string; at: number }>;
  /** @deprecated Alias für `windowStateRows` (seit §16.19). */
  windowState(): Promise<any[]>;
  coreHead(): Promise<number>;
  /** Wartet, bis Outbox, Zustellung und Hintergrundaufgaben ruhen. */
  settle(timeoutMs?: number): Promise<true>;
  readonly checkpoint: string;
  readonly lastVisible: { app_id: string; collections: string[]; at: number } | null;
  readonly activationReports: Array<{ app_id: string; app_ver: string; state: ActivationState; at_seq: string; error?: string }>;
  readonly navigations: RouteEvent[];
  readonly dialogs: Array<{ app_id: string; kind: 'dialog' | 'menu'; spec: unknown }>;
  readonly openedFiles: Array<{ app_id: string; ref: string }>;
  readonly staleDiscards: number;
  readonly refetches: number;
}

export interface StubKernel {
  readonly instanceId: string;
  readonly userId: string;
  readonly dbName: string;
  readonly tabId: string;
  readonly ready: Promise<void>;
  readonly isLeader: boolean;
  readonly storageKind: 'indexeddb' | 'memory' | null;
  appApi(appId: string, appVer: string): CtoxAppApi;
  readonly tabKey: string;
  // Shell-Kernel-Schnittstelle direkt am Kernel (§16.15, §16.19) – identisch mit kernel.shell
  apps: ShellKernelApi['apps'];
  activation: ShellKernelApi['activation'];
  metrics: ShellKernelApi['metrics'];
  leader: ShellKernelApi['leader'];
  status: StatusStream;
  /** @deprecated Alias für `status` (seit §16.19). */
  statusZone: StatusStream;
  windowState: WindowStateApi;
  artifacts: ShellKernelApi['artifacts'];
  source: ShellKernelApi['source'];
  identity: ShellKernelApi['identity'];
  shell: ShellKernelApi;
  shellApi(): ShellKernelApi;
  /** §16.45 „Neu verbinden“: voller Neuaufbau des Transports (nie Seiten-Reload); im Leser über den Leiter. */
  reconnect(): Promise<{ link: Metrics['link']; collections: Metrics['collections'] }>;
  sim: StubSim;
  close(): Promise<void>;
  logout(opts?: { deleteData?: boolean }): Promise<void>;
}

export declare const APP_API_VERSION: '4.0';
export declare function compileQuery(q: Query): unknown;
export declare function runCompiledQuery<T>(docs: T[], compiled: unknown): T[];
export declare function uuidv7(): string;
export declare function sha256Hex(content: string | Blob | ArrayBuffer | ArrayBufferView): Promise<string>;
export declare function mergePatch<T>(target: T, patch: object): T;
export declare const SCHEMA_FILE: 'collections.schema.json';
export declare function parseCollectionsFile(input: string | CollectionsFile): { collections: Record<string, { schema: any; indexes: unknown[]; class: string | null; rights: unknown; client_limit: number | null; byte_budget: number | null }>; migrations: CollectionsFile['migrations'] };
export declare function classifyCollections(base: ReturnType<typeof parseCollectionsFile>, next: ReturnType<typeof parseCollectionsFile>, opts?: { baseVer?: string | null }): CollectionDiff[];
export declare function applyMigrationSteps<T extends object>(doc: T, steps: MigrationStep[]): T;

// Real local kernel. The stub-only entry selects an explicit MemoryCoreTransport.
export interface KernelOptions extends Omit<StubKernelOptions, 'instanceId'|'userId'> {
  instanceId: string;
  userId: string;
  transport?: KernelTransport;
  locks?: LockManager;
  epoch?: string;
  scopeHash?: string;
  /** Explicit identity-verified, stopped V3 source. Never guessed from database names. */
  legacyDatabase?: {name:string;instanceId:string;userId:string;quiesced:true};
}
export interface Subscription {
  epoch: string; collection: string; scope_hash: string; gen: number;
  checkpoint_seq: string; priority: 'P0'|'P1'|'P2';
}
export interface KernelPage {
  epoch: string; collection: string; scope_hash: string; gen: number;
  type?: 'page'|'live'; cursor_to?: string; head_seq?: string; complete: boolean; backlog?: number;
  package_id?:string; seq_from?:string; seq_to?:string; sig?:string|number[]; core_sent_at_ms?:number;
  docs: Array<{collection?:string; id:string; data?:unknown; doc?:unknown; body?:unknown;
    deleted?:boolean; op?:'put'|'upsert'|'delete'|'unproject'; seq:string; rev:string}>;
}
export interface KernelWrite {
  op_id:string; prev_op_id?:string|null; client_seq?:string; collection:string; id:string;
  base_rev:string|null; base_seq:string; gen:number; schema_ver:number; app_ver:string;
  app_id:string; op:'put'|'patch'|'delete'; body:unknown;
}
export declare class KernelTransport {
  grantApp(appId:string):Promise<AppScopeGrantToken>;
  scope?(collection:string):Promise<{epoch:string;collection:string;scope_hash:string;gen:number}>;
  connect(identity:{instanceId:string; userId:string}, events:{
    page(page:KernelPage):Promise<void>;
    write_result(result:WriteResult & {op_id:string}):Promise<void>;
    link(state:Metrics['link']):void;
    timing?(state:NonNullable<Metrics['timing']>):void;
    status?(appId:string,item:StatusItem):void;
    revoke?(message:{app_id?:string;collections:string[];rights_ver:string;artifacts?:string[]}):Promise<void>;
    actor?(state:Partial<{id:string;display_name:string;role:string;permissions:string[]}> & {id:string}):void;
  }):Promise<void>;
  subscribe(scope:Subscription):Promise<void>;
  write(entry:KernelWrite):Promise<(WriteResult & {doc?:unknown;deleted?:boolean})|void>;
  appActivation(report:{app_id:string;app_ver:string;state:ActivationState;at_seq:string}):Promise<void>;
  priority(hint:{app_id:string;collections:string[]}):void;
  artifact(hash:string):Promise<string>;
  blob(ref:string):Promise<Blob>;
  upload(ref:string,blob:Blob):Promise<void>;
  close():void;
}
export interface Kernel extends StubKernel {
  receivePage(page:KernelPage):Promise<void>;
  startShell(module:ShellModule,opts?:{restored?:unknown}):Promise<boolean>;
  swapShell(module:ShellModule):Promise<{swapped:boolean;restored:unknown;error?:string}>;
  loadModule(appId:string,version:string):Promise<AppModule|ShellModule>;
  evict(collection:string):Promise<number>;
}
export declare function createKernel(options:KernelOptions):Kernel;

export interface KernelLoader {
  readonly kernel:Kernel;
  readonly shellVersion:string|null;
  settle():Promise<void>;
  close():Promise<void>;
}
export interface LoaderOptions extends KernelOptions {
  serviceWorker?: false | '/sw.js';
  resolveShell?:(version:string,artifacts:ShellKernelApi['artifacts'])=>ShellModule|Promise<ShellModule>;
}
export declare function createLoader(options:LoaderOptions):Promise<KernelLoader>;
export declare function artifactUrl(appId:string,version:string,path:string,shellVersion:string):string;

export type ReliableLane='control'|'commands'|'interactive'|'live'|'initial'|'transfer';
export interface CodecAdapter {encode(bytes:Uint8Array):Promise<Uint8Array>;decode(bytes:Uint8Array):Promise<Uint8Array>}
export interface WebRTCTransportOptions {
  address?:string;
  channels?:Record<number,RTCDataChannel>;
  peer?:RTCPeerConnection;
  ownsConnection?:boolean;
  createPeer?:(address:string|undefined,identity:{instanceId:string;userId:string})=>Promise<{channels:Record<number,RTCDataChannel>;peer?:RTCPeerConnection;relay?:boolean;close?:()=>void}>;
  /** Host pairing/signer supplies a P4-a hello; no credential persistence here. */
  authSession?:KernelAuthSession;
  hello?:object|((identity:{instanceId:string;userId:string})=>object|Promise<object>);
  resolveScope?:(collection:string,session:object)=>{scope_hash:string;gen:number}|Promise<{scope_hash:string;gen:number}>;
  /** P6 policy verifies the complete signed package, including progress proof. */
  verifyPackage:(message:KernelPage,session:object)=>boolean|Promise<boolean>;
  codecs?:Partial<Record<'zstd'|'gzip',CodecAdapter>>;
  handshakeTimeoutMs?:number;
  transferTimeoutMs?:number;
  grantTimeoutMs?:number;
  /** Host/test clock seam for lease renewal only; handshake, delivery and close keep their own timers. */
  grantClock?:{now():number;setTimeout(callback:()=>void,delay:number):any;clearTimeout(timer:any):void};
  now?:()=>number;
}
export declare class WebRTCKernelTransport extends KernelTransport {
  constructor(options:WebRTCTransportOptions);
  readonly ready:Promise<unknown>;
  reconnect(address?:string):Promise<void>;
}
export declare function createReliableChannels(peer:RTCPeerConnection):Record<number,RTCDataChannel>;
export declare const PLATFORM_APP_ID:'ctox-platform';
export declare const PLATFORM_COLLECTIONS:readonly string[];
export declare function collectionOwner(collection:string,appId:string):string;

export interface ArtifactBinding {
  readonly appId: string;
  readonly ver: string;
  readonly shellVer: string | null;
  readonly libs: Readonly<Record<string, string>>;
  readonly root: string;
  url(path: string): string;
  /** §16.50: gleich-ursprüngliche Worker-URL unter der gebundenen Lib-Wurzel `/~a/lib:<name>@<M.m>[~lib:…]/<path>` (nie data:/blob:); nicht gebunden → not_found. */
  workerUrl(lib: string, path: string): string;
}

// ---- E38.4 v4 browser entry (boot.mjs) ----
/** P4-a hello offer exactly as decodeHelloFrame (rxdb/src/hello.mjs) accepts it. */
export interface V4HelloOffer {
  type:'hello'; protocol_ver:4; bootstrap_ver:number; app_api_ver:{min:string;max:string};
  /** Must equal V4LaunchContext.v4.instanceId. */
  instance_id:string; capabilities:string[];
  /** Minted by the daemon for this authenticated HTTP session; single use (fresh nonce per launch-context fetch). */
  device_proof:{device_id:string;public_key:string;nonce:string;signature:string};
  /** Shell collections only. App grants are acquired separately on the control lane (E55). */
  scopes_wanted:string[];
  /** Without a host zstd adapter only ['gzip'] is usable. */
  transport:{codecs:('gzip'|'zstd')[];max_frame_bytes:number;fragmentation:boolean;reassembly:boolean};
}
/**
 * Response of GET /api/business-os/launch-context (credentials same-origin). V2 fields (session, config,
 * designTemplates) stay untouched; v4 reads ONLY the `v4` member. Unknown members are ignored. No business data.
 */
export interface V4LaunchContext {
  v4:{
    instanceId:string;
    userId:string;
    /** actor.id === userId. */
    actor:{id:string;role:string;[key:string]:unknown};
    signaling:{
      /** ws:/wss: signaling endpoints, tried in order. */
      urls:string[];
      room:string;
      /** This browser's signaling peer id. */
      peerId:string;
      /** The daemon's native v4 peer id; the browser sends it the SDP offer. */
      nativePeerId:string;
      /** Signaling join token (query `token`). */
      token:string;
      iceServers:{urls:string|string[];username?:string;credential?:string}[];
    };
    hello:{offer:V4HelloOffer};
    /** Instance signing keys: verify stream packages (key_id) and scope tokens. Ed25519 raw public key, base64url (43 chars). */
    packageKeys:{keyId:string;alg:'Ed25519';publicKey:string}[];
  };
}
export type BootV4Reason='invalid_options'|'launch_context_unavailable'|'launch_context_invalid'|'package_verifier_missing'|'hello_rejected'|'transport_unavailable';
export declare class BootV4Error extends CtoxError {
  readonly name:'BootV4Error';
  /** invalid (invalid_options) | unavailable | invalid | forbidden (package_verifier_missing, hello_rejected) | unavailable (transport). */
  readonly reason:BootV4Reason;
}
export interface BootV4Options {
  /** Default '/api/business-os/launch-context'. */
  launchContextUrl?:string;
  /** Optional. Absent: createLoader loads the activated shell version via kernel.loadModule('shell', ver) (manifest entry,
   * v4/frame/index.js). Present but not a function: BootV4Error reason 'invalid_options'. */
  resolveShell?:(version:string,artifacts:ShellKernelApi['artifacts'])=>ShellModule|Promise<ShellModule>;
  fetchImpl?:typeof fetch;
  /** Host/test seam; default: built-in signaling client (WebSocket + RTCPeerConnection + createReliableChannels). */
  createPeer?:WebRTCTransportOptions['createPeer'];
  codecs?:WebRTCTransportOptions['codecs'];
  subtle?:SubtleCrypto;
  handshakeTimeoutMs?:number;
  /** Additional loader/kernel options; identity, actor, transport and resolveShell always come from the launch context. */
  loaderOptions?:Partial<Omit<LoaderOptions,'instanceId'|'userId'|'actor'|'transport'|'resolveShell'>>;
  /** Test/host seam; default createLoader. */
  loaderFactory?:(options:LoaderOptions)=>Promise<KernelLoader>;
}
export interface BootV4Result {
  readonly loader:KernelLoader;
  readonly kernel:Kernel;
  readonly transport:WebRTCKernelTransport;
  readonly launchContext:Readonly<V4LaunchContext['v4']>;
}
/** Leader tabs resolve after the authenticated hello; follower tabs after the loader is up. Never falls back to V2/HTTP data. */
export declare function bootV4(options?:BootV4Options):Promise<BootV4Result>;
export declare function parseV4LaunchContext(payload:unknown):Readonly<V4LaunchContext['v4']>;
export declare function fetchV4LaunchContext(url?:string,fetchImpl?:typeof fetch):Promise<Readonly<V4LaunchContext['v4']>>;
export declare const DEFAULT_LAUNCH_CONTEXT_URL:'/api/business-os/launch-context';

/** E55/E22: base64url(UTF8 JSON {claims,signature}); Ed25519 signature over UTF8
 * `ctox:v4:scope\0` + recursively key-sorted canonical JSON of claims.
 * exp = Unix seconds, rights_ver = canonical decimal u64; no padding in base64url. */
export interface AppScopeGrantClaims {
 instance_id:string; user_id:string; app_id:string; scope_hash:string; rights_ver:string; device_id:string; exp:number;
 collections:{collection:string;gen:number;rights:('read'|'write')[];field_mask_digest:string}[];
}
export type AppScopeGrantToken=string;
export type AppScopeControlMessage=
 |{type:'scope.grant.request';app_id:string}
 |{type:'scope.grant';token:AppScopeGrantToken}
 |{type:'forbidden';app_id:string;reason:string};

export interface KernelAuthSession {
 hello(identity:{instanceId:string;userId:string},challenge?:{nonce:string}):Promise<object>;
 acceptHello(frame:object):Promise<void>;
 acceptAppGrant(appId:string,token:AppScopeGrantToken):Promise<AppScopeGrantToken>;
 revokeApp(appId:string,rightsVer?:string):AppScopeGrantToken|undefined;
 scope(collection:string):Promise<{epoch:string;collection:string;scope_hash:string;gen:number;app_id?:string}>;
 tokens():string[]; close():void;
}
