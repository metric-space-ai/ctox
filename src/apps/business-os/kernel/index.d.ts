// CTOX App-API 4.0 – Typpaket (Quelle für ctox-app-api.d.ts, §16.10).
// Vertrag: ctox-sync-v4-architektur.md §10.2, §10.2a, §16.7, §16.10, §16.15.
// Teil 1: normative App-API. Teil 2: Shell-Kernel-API (nur Shell). Teil 3: Stub-Kernel + Sim-Hooks (nur Stub).

// ============================================================================================================
// Teil 1 – App-API (an jede App übergeben, app_api_ver = 4.0)
// ============================================================================================================

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
  status: 'accepted' | 'rejected' | 'migrated';
  rev?: string;
  reason?: string;
  archived_ref?: string;
  /** Stub/Draht: globale Folgenummer der bestätigten Änderung (§2.2). */
  seq?: string; // unsigned decimal on the v4 wire (§16.1)
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
  status: 'queued' | 'accepted' | 'running' | 'done' | 'failed';
  progress?: number;
  result?: unknown;
  error?: string;
};
export interface CommandHandle {
  /** ID des Befehlsdokuments in `business_commands`. */
  id: string;
  /** Gepushter Status, kein Warten auf Annahme. */
  status: LiveDoc<CommandState>;
  local: Promise<void>;
  result: Promise<WriteResult>;
  cancel(): boolean;
}

export type CtoxErrorCode = 'invalid' | 'forbidden' | 'not_found' | 'conflict' | 'revoked' | 'expired' | 'quota' | 'unavailable';
export declare class CtoxError extends Error {
  readonly name: 'CtoxError';
  readonly code: CtoxErrorCode;
  readonly path?: string;
  constructor(code: CtoxErrorCode, message?: string, extra?: { path?: string });
}

export type StatusItem = { level: 'info' | 'warn' | 'error'; code: string; text: string; ref?: string };

export type Metrics = {
  backlog: number;
  outbox: { count: number; oldest_ms: number };
  link: { state: string; rtt_ms: number; relay: boolean };
};

export interface RouteEvent { from: string; to: string; params: Record<string, unknown> }

/** §10.2a Nr. 2 – gehört der Shell, versioniert mit app_api_ver. */
export interface ShellApi {
  setTitle(title: string): void;
  /** Explizit, keine DOM-Heuristik. */
  registerHeader(spec: unknown): void;
  /** Unbekanntes Ziel → Fehler in der Zustandszone, `false`, nie stiller Fallback. */
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

export type FileMeta = { id: string; name: string; mime: string; size: number; ref: string; owner_app: string };
export interface FileUploadHandle extends WriteHandle<{ ref: string }> {
  /** sha256 des Inhalts (hex). */
  ref: Promise<string>;
  upload: { progress: LiveDoc<{ sent: number; total: number }> };
}
/** §10.2a Nr. 4 + §16.10. Metadaten = Dokumente der Collection `files`. */
export interface FilesApi {
  /** Lokaler Blob-Cache, sonst Spur 4 (Prüfung beim Empfang). Fehler: not_found | unavailable | revoked. */
  get(ref: string): Promise<Blob>;
  put(blob: Blob, meta?: { id?: string; name?: string; mime?: string }): FileUploadHandle;
}

export interface CtoxAppApi {
  version: '4.0';
  app: { id: string; ver: string };
  // Lesen – nur lokal, nie Netzwerk, keine Lesesperre
  query<T = any>(collection: string, q: Query): LiveQuery<T>;
  get<T = any>(collection: string, id: string): LiveDoc<T | null>;
  // Schreiben – lokal sofort, Outbox
  put<T>(collection: string, doc: T & { id: string }): WriteHandle;
  patch(collection: string, id: string, patch: object): WriteHandle;
  remove(collection: string, id: string): WriteHandle;
  // Befehle – Dokument mit gepushtem Status
  command(type: string, payload: object): CommandHandle;
  // Priorität (E1) – reiner Hinweis
  setVisible(collections: string[]): void;
  // Zustand – nur an die Zustandszone der Shell (Anf. 27)
  status: {
    report(item: StatusItem): void;
    clear(code: string): void;
  };
  // Messwerte (lesend, für die Zustandszone; Apps zeigen sie nicht selbst). Leser-Tabs: link = Zustand des Leiters.
  metrics: LiveDoc<Metrics>;
  shell: ShellApi;
  actor: Actor;
  files: FilesApi;
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
};
export type Activation = {
  app_id: string;
  activated_ver: string | null;
  target_ver: string | null;
  previous_ver?: string | null;
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
  handlers: Record<string, string>;
  min_app_api: string;
  capabilities_required: string[];
  provenance: { base_release: string | null; base_hash: string | null; locally_modified: boolean };
  author: string;
  created_at: string;
};

export interface SourceApi {
  /** Recht apps.source.view. Ohne `ver`: aktive Version plus Entwurfsdateien. */
  list(ver?: string): Promise<string[]>;
  /** Recht apps.source.view. Ohne `ver`: Entwurf, falls vorhanden, sonst aktive Version. */
  read(path: string, ver?: string): Promise<string>;
  draft: {
    /** Recht apps.source.edit. */
    write(path: string, text: string): Promise<void>;
    setCollections(schemaDiff: unknown): Promise<void>;
    /** Vergleich-und-Setzen gegen die aktive Version; result.rev = neue Version. Gründe: forbidden | stale_base | busy. */
    publish(opts: { base_app_ver: string }): WriteHandle;
  };
}

export type StatusZoneEntry = Readonly<StatusItem & { app_id: string; ref: string | null; source: 'app' | 'kernel'; at: number }>;
export interface StatusZone extends LiveQuery<StatusZoneEntry> {
  forApp(appId: string): LiveQuery<StatusZoneEntry>;
  clear(appId: string, code: string): void;
}

export interface WindowState { title: string; header: unknown; size: { width: number; height: number }; mode: string; closed: boolean }

export interface ShellKernelApi {
  apps: LiveQuery<AppEntry>;
  activation: {
    get(appId: string): LiveDoc<Activation | null>;
    onChange(cb: (ev: ActivationEvent) => void): () => void;
  };
  artifacts: {
    read(hash: string): Promise<Blob>;
    manifest(appId: string, ver: string): Promise<AppManifest | null>;
  };
  source(appId: string): SourceApi;
  metrics: LiveDoc<Metrics>;
  leader: LiveDoc<{ isLeader: boolean; leader_gen: number; tab_id: string }>;
  identity: {
    readonly instance_id: string;
    readonly user_id: string;
    /** Schließt DB, beendet Callbacks, sperrt Zugriff. Löschen nur mit deleteData:true. */
    logout(opts?: { deleteData?: boolean }): Promise<void>;
    /** Schließt diesen Kernel und liefert einen Kernel für (Instanz, Nutzer). */
    switchInstance(next: { instanceId?: string; userId?: string }): Promise<StubKernel>;
  };
  statusZone: StatusZone;
  /** Stub-Komfort: Fensterzustand je App (Titel/Header kommen aus api.shell). */
  window(appId: string): {
    state: LiveDoc<WindowState>;
    setSize(size: { width: number; height: number }): void;
    setMode(mode: string): void;
    reopen(): void;
  };
  /** Stub-Komfort: App in `root` mounten; Remount bei Aktivierung inkl. snapshot → window_state → restored, 10-s-Rückfall. */
  mount(root: unknown, appId: string): Promise<{ app_id: string; readonly ver: string; readonly api: CtoxAppApi; snapshot(): unknown; unmount(): Promise<void> }>;
}

export declare function createShellKernel(kernel: StubKernel): ShellKernelApi;

// ============================================================================================================
// Teil 3 – Stub-Kernel und Sim-Hooks (nur Stub, kein Vertrag)
// ============================================================================================================

export interface StubKernelOptions {
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
  /** Rückfall-Frist beim Remount (Standard 10000, §16.10). */
  remountTimeoutMs?: number;
  /** false = Ein-Tab-Betrieb erzwingen. */
  multiTab?: boolean;
}

export interface PublishOptions {
  /** Pfad → Inhalt (UTF-8). Einstieg `entry` (Standard index.js) wird bei fehlendem `module` importiert. */
  files?: Record<string, string>;
  entry?: string;
  /** In-Memory-Modul (nur dieser Tab), statt Code aus Artefakten zu laden. */
  module?: AppModule;
  title?: string;
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
  /** Nächster bestätigter Write wird abgelehnt. */
  rejectNext(reason?: string): Promise<void>;
  /** Nächster bestätigter Write wird `migrated`; `dropFields` wandern ins Core-Archiv. */
  migrateNext(opts?: { dropFields?: string[] }): Promise<void>;
  /** Nächster Befehl dieses Typs endet `failed`. */
  failCommand(type: string, error?: string): Promise<void>;
  /** Ergebnis-Handler für Befehle (läuft im Leiter-Tab). */
  onCommand(type: string, fn: (payload: any, ctx: { app_id: string; command_id: string }) => unknown): void;
  /** Live-Änderung aus dem Core; `{id, _deleted: true}` = Löschung. Liefert vergebene seq. */
  push(collection: string, docs: Array<{ id: string; _deleted?: boolean; [k: string]: unknown }> | { id: string; [k: string]: unknown }): Promise<number[]>;
  /** state 'offline' hält Senden und Zustellung an (Rückstand wächst). */
  setLink(link: Partial<{ state: 'online' | 'offline' | 'connecting' | string; rtt_ms: number; relay: boolean }>): Promise<Metrics['link']>;
  setBacklog(n: number): Promise<void>;
  /** Veröffentlichen + Aktivieren (received → fetched → data-ready → activated|failed). Liefert den Endzustand. */
  publishAppVersion(appId: string, ver: string, opts?: PublishOptions): Promise<Activation | null>;
  registerModule(appId: string, ver: string, module: AppModule): void;
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
  /** Zustandszone (live, je App filterbar). */
  statusZone: StatusZone;
  shell: ShellKernelApi;
  shellApi(): ShellKernelApi;
  sim: StubSim;
  close(): Promise<void>;
  logout(opts?: { deleteData?: boolean }): Promise<void>;
}

export declare const APP_API_VERSION: '4.0';
export declare function createStubKernel(opts?: StubKernelOptions): StubKernel;
export declare function compileQuery(q: Query): unknown;
export declare function runCompiledQuery<T>(docs: T[], compiled: unknown): T[];
export declare function uuidv7(): string;
export declare function sha256Hex(content: string | Blob | ArrayBuffer | ArrayBufferView): Promise<string>;
export declare function mergePatch<T>(target: T, patch: object): T;

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
  cursor_to: string; head_seq?: string; complete: boolean; backlog?: number;
  docs: Array<{collection?:string; id:string; data?:unknown; doc?:unknown;
    deleted?:boolean; op?:'put'|'delete'|'unproject'; seq:string; rev:string}>;
}
export interface KernelWrite {
  op_id:string; prev_op_id?:string|null; collection:string; id:string;
  base_rev:string|null; base_seq:string; gen:number; schema_ver:number; app_ver:string;
  app_id:string; op:'put'|'patch'|'delete'; body:unknown;
}
export declare class KernelTransport {
  connect(identity:{instanceId:string; userId:string}, events:{
    page(page:KernelPage):Promise<void>;
    write_result(result:WriteResult & {op_id:string}):Promise<void>;
    link(state:Metrics['link']):void;
    actor(state:Partial<{id:string;display_name:string;role:string;permissions:string[]}> & {id:string}):void;
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
export declare function artifactUrl(appId:string,version:string,path:string):string;
