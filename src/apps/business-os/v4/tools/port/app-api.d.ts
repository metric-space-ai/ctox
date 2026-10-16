// CTOX App-API 4.0 – Port-Grundlage für alle App-Stränge (Shell v4, Strang K).
//
// Teil 1 unten ist WÖRTLICH der normative Teil des Kern-Stubs ~/.local/state/ctox-sync-v4/stub/index.d.ts
// (Stand 15.10.2026 12:40, Zeilen 1–191; Teil 2/3 dort = Shell-Kernel-API und Stub-Hooks, für Apps tabu).
// Abweichungen: keine. Bei Änderung des Stubs diese Datei neu ableiten:
//   sed -n "1,$(grep -n '^export interface AppModule' <stub>/index.d.ts | cut -d: -f1)p" <stub>/index.d.ts
//
// Vertrag offen (nicht im Stub festgelegt, gemessen am Stub-Verhalten; Ports dürfen sich NICHT darauf verlassen):
//   OFFEN-A  ShellApi.dialog/menu: Form von `spec` frei; Stub löst mit `spec.default ?? null` (README Nr. 22).
//            knowledge nutzt {kind:'confirm', title, text, confirm_label, default:false}.
//   OFFEN-B  ShellApi.t(key): Schlüsselraum (App-Locales aus dem Manifest?) offen; Stub liefert den Schlüssel
//            selbst, wenn unbekannt. Ports behalten eingebettete Rückfalltexte.
//   OFFEN-C  ShellApi.registerHeader(spec): Form offen.
//   OFFEN-D  Es gibt keine Icon-API und keine Drawer-API in 4.0 (ctx.getActionIcon, ctx.openLeftDrawer… entfallen
//            ersatzlos): Apps halten Glyphen und Drawer selbst, Drawer liegen im App-root.
//   OFFEN-E  `$options` steht nur im Stub, nicht in Architektur §10.2a Nr. 5.
//   OFFEN-F  `ctox:shared/x.js` funktioniert nur über den Service Worker (§16.15). Eine native Import-Map mit
//            Schlüssel "ctox:shared/" greift in Chromium headless 1223 NICHT (gemessen 15.10. 12:55: CORS-Block
//            „protocol schemes … http, https“). Tests müssen das Umschreiben selbst nachbilden.
//   OFFEN-G  mount darf ein Promise liefern (AppMount); binnen 10 s auflösen (§16.10).

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
  seq?: number;
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

/** Zweites Argument von `api.shell.openSource` (Strang LOOP Runde 2). */
export interface OpenSourceOptions {
  /** Reiter neben dem Dateibaum der Code-Ansicht. */
  tab?: 'files' | 'reports';
  /** Meldung (business_module_reports.id) der Ziel-App, die im Reiter „Meldungen“ markiert wird. */
  reportId?: string;
}

/** §10.2a Nr. 2 – gehört der Shell, versioniert mit app_api_ver. */
export interface ShellApi {
  setTitle(title: string): void;
  /** Explizit, keine DOM-Heuristik. */
  registerHeader(spec: unknown): void;
  /** Unbekanntes Ziel → Fehler in der Zustandszone, `false`, nie stiller Fallback. */
  navigate(appId: string, params?: Record<string, unknown>): boolean;
  onRoute(cb: (ev: RouteEvent) => void): () => void;
  /** Quelltext-Ansicht einer App (Standard: die eigene): öffnet bzw. holt das App-Fenster und schaltet es auf „Code“
   *  (Umschalter App | Code im Fenster, FR6a); nur mit apps.source.view.
   *  Nicht möglich → Fehler `shell.openSource` in der Zustandszone, `false`. Ersetzt ctx.openModuleSource.
   *  `options` (Strang LOOP Runde 2): `tab` wählt den Reiter neben dem Dateibaum, `reportId` markiert die Meldung im
   *  Reiter „Meldungen“ (nur `reportId` → `tab: 'reports'`); andere Felder oder Werte → CtoxError `invalid`. */
  openSource(appId?: string, options?: OpenSourceOptions): boolean;
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
    /** Vollbild als Fensterfunktion der Shell (Element-Vollbild des Fensterinhalts; Esc beendet). Apps bauen keinen
     *  eigenen Vollbild-Mechanismus (kein requestFullscreen). `true`, wenn das Fenster danach im Vollbild ist. */
    fullscreen(): Promise<boolean>;
    /** Beendet das Vollbild dieses Fensters. `true`, wenn danach kein Vollbild mehr besteht. */
    exitFullscreen(): Promise<boolean>;
    readonly isFullscreen: boolean;
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
