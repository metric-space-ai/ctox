# Composition root — the load-bearing prerequisite

**Without this, parallel subagent ports collapse.** Every pattern in this skill presupposes a working hexagonal composition root. Do not skip.

## The 5 files

```
src/lib/composition/
  types.ts              ← Container, Repositories, Gateways, Config interfaces
  build-container.ts    ← env reads happen ONLY here; returns Container
  test-container.ts     ← same Container shape with all fakes
  build-services.ts     ← CoreServices factory taking Container
  index.ts              ← getContainer + getServices process-wide singletons
```

## types.ts shape

```ts
import type { PgDatabase, PgQueryResultHKT } from "drizzle-orm/pg-core";
import * as schema from "@/lib/db/schema";

export interface Config {
  databaseUrl: string;
  authSecret: string;
  // ... typed env values
  resendApiKey?: string;
  blobReadWriteToken?: string;
  redisUrl?: string;
  sentryDsn?: string;
  // ...
}

export interface Repositories {
  principals: PrincipalRepository;
  projects: ProjectRepository;
  workPackages: WorkPackageRepository;
  // ... one slot per module
  // Sub-namespaces for modules with multiple repos:
  costs: {
    timeEntries: TimeEntryRepository;
    activities: ActivityRepository;
    // ...
  };
  meetings: {
    meetings: MeetingRepository;
    agendaItems: AgendaItemRepository;
    // ...
  };
  // ...
}

export interface Gateways {
  blobStorage: BlobStorage;
  mailSender: MailSender;
  inngestClient: InngestClient;
  notificationCascade: NotificationCascade;
  errorReporter: ErrorReporter;
  rateLimiter: RateLimiter;
  auditSink: AuditEventSink;
  // ... one slot per external/cross-cutting concern
}

export interface Container {
  clock: Clock;
  ids: IdGenerator;
  logger: Logger;
  passwordHasher: PasswordHasher;
  repositories: Repositories;
  gateways: Gateways;
  db: PgDatabase<PgQueryResultHKT, typeof schema>;
}
```

## build-container.ts shape

```ts
import { drizzle } from "drizzle-orm/postgres-js";
import postgres from "postgres";
import * as schema from "@/lib/db/schema";

export function readConfigFromEnv(): Config {
  const databaseUrl = process.env.DATABASE_URL;
  if (!databaseUrl) throw new Error("composition: DATABASE_URL is required");
  const authSecret = process.env.AUTH_SECRET;
  if (!authSecret) throw new Error("composition: AUTH_SECRET is required");
  return {
    databaseUrl,
    authSecret,
    resendApiKey: process.env.RESEND_API_KEY || undefined,
    blobReadWriteToken: process.env.BLOB_READ_WRITE_TOKEN || undefined,
    // ...
  };
}

export function buildContainer(config: Config): Container {
  const client = postgres(config.databaseUrl, { max: 10 });
  const db = drizzle(client, { schema });

  // 1. Build leaf adapters
  const clock = realClock;
  const ids = realIds;
  const logger = createRealLogger();

  // 2. Build gateways from config
  const gateways = buildGatewaysFromConfig(config, logger, { db, clock });

  // 3. Build repositories from drizzle
  const repositories = buildRepositoriesFromDrizzle(db);

  return { clock, ids, logger, passwordHasher: bcryptHasher, repositories, gateways, db };
}

function buildRepositoriesFromDrizzle(db: Db): Repositories {
  return {
    principals: createDrizzlePrincipalRepository(db),
    projects: createDrizzleProjectRepository(db),
    workPackages: createDrizzleWorkPackageRepository(db),
    // CRITICAL: every entry must be Drizzle in production. NOT memory.
    // The 20-in-memory-port BUG-11 pattern from the OpenProject port is what
    // happens when this rule is violated.
    costs: {
      timeEntries: createDrizzleTimeEntryRepository(db),
      activities: createDrizzleActivityRepository(db),
      // ...
    },
    // ...
  };
}
```

## test-container.ts shape

```ts
export function createTestContainer(opts?: TestContainerOpts): Container {
  return {
    clock: opts?.clock ?? createFakeClock(),
    ids: opts?.ids ?? createFakeIds(),
    logger: createFakeLogger(),
    passwordHasher: fakePasswordHasher,
    repositories: {
      principals: createMemoryPrincipalRepository(opts?.principalsSeed),
      projects: createMemoryProjectRepository(opts?.projectsSeed),
      // ... every slot mirrored, but with memory adapters and seedable
    },
    gateways: {
      blobStorage: createFakeBlobStorage(),
      mailSender: createFakeMailSender(),
      // ...
    },
    db: undefined as unknown as Db, // most tests don't need it
  };
}
```

## build-services.ts shape

```ts
export interface CoreServices {
  users: UsersService;
  projects: ProjectsService;
  workPackages: WorkPackagesService;
  members: MembersService;
  // ... one slot per service
}

export function buildCoreServices(container: Container): CoreServices {
  const { clock, ids, logger, repositories, gateways, db } = container;
  const assertCan = createAssertCan(repositories.members, repositories.principals);

  const users = createUsersService({
    principals: repositories.principals,
    clock, ids, logger, assertCan,
  });

  const members = createMembersService({
    port: createDrizzleMembersAdminPort(db),  // ← Drizzle, NOT memory
    clock, logger, assertCan,
    users: repositories.principals,
    roles: { list: () => repositories.roles.list() },
  });

  // ... etc
  return { users, projects, workPackages, members, /* ... */ };
}
```

## index.ts — singletons NOT React.cache

```ts
/**
 * UX-TEST-FIX (2026-05-02): React.cache is per-RSC-request scope, so every
 * HTTP request opened a fresh Postgres pool that was never released. Result:
 * `PostgresError: sorry, too many clients already` after ~30 page loads.
 *
 * Use a globalThis-symbol-keyed singleton instead so the SAME container
 * (and SAME postgres pool) is reused across requests. The globalThis
 * indirection is required in Next.js dev mode where HMR re-evaluates
 * module bodies but keeps the global object — without it the dev pool
 * leaks across hot reloads.
 */
import { buildContainer, readConfigFromEnv } from "./build-container";
import { buildCoreServices, type CoreServices } from "./build-services";
import type { Container } from "./types";

interface ContainerCache {
  container?: Container;
  services?: CoreServices;
}

const GLOBAL_KEY = Symbol.for("<app-name>.composition");

function getCache(): ContainerCache {
  const g = globalThis as unknown as Record<symbol, ContainerCache>;
  if (!g[GLOBAL_KEY]) g[GLOBAL_KEY] = {};
  return g[GLOBAL_KEY];
}

export function getContainer(): Container {
  const c = getCache();
  if (!c.container) c.container = buildContainer(readConfigFromEnv());
  return c.container;
}

export function getServices(): CoreServices {
  const c = getCache();
  if (!c.services) c.services = buildCoreServices(getContainer());
  return c.services;
}
```

**Apply the same pattern to Auth.js handle** (`getAuth()` in `lib/auth/session.ts`) — also a globalThis singleton, NOT React.cache, for the same reason.

## Reference module shape

The reference module is **the** template every subsequent agent will copy. Spend the time to get it right.

```
src/modules/users/             ← ONE complete module written by hand
  schema.ts                      Drizzle table definition
  repositories.ts                interface(s) + record types
  drizzle/repositories.ts        production impl(s)
  memory/repositories.ts         test impl(s) — same surface
  service.ts                     createUsersService(deps): UsersService
  service.test.ts                tests using only fakes
  setup.ts                       registers permissions, menu items, jobs
```

### service.ts shape

```ts
import { z } from "zod";
import type { Clock } from "@/lib/clock/types";
import type { IdGenerator } from "@/lib/ids/types";
import type { Logger } from "@/lib/logger/types";
import type { AssertCan } from "@/lib/permissions/assert";
import type { Result } from "@/lib/result";
import { ok, err } from "@/lib/result";

export interface UsersServiceDeps {
  principals: PrincipalRepository;
  clock: Clock;
  ids: IdGenerator;
  logger: Logger;
  assertCan: AssertCan;
}

export interface UsersService {
  create(actor: Actor, command: unknown): Promise<Result<PrincipalRecord, CreateUserError>>;
  list(actor: Actor, opts?: ListOpts): Promise<Result<PrincipalRecord[], { code: "permission_denied" }>>;
  // ...
}

const createUserSchema = z.object({
  login: z.string().min(1),
  mail: z.string().email(),
  // ...
});

export type CreateUserError =
  | { code: "validation"; details: unknown }
  | { code: "permission_denied" }
  | { code: "duplicate_login" }
  | { code: "duplicate_mail" };

export function createUsersService(deps: UsersServiceDeps): UsersService {
  const { principals, clock, logger, assertCan } = deps;

  return {
    async create(actor, raw) {
      try {
        await assertCan(actor.id, "manage_users");
      } catch (e) {
        if (e instanceof PermissionDenied) return err({ code: "permission_denied" });
        throw e;
      }
      const parsed = createUserSchema.safeParse(raw);
      if (!parsed.success) return err({ code: "validation", details: parsed.error.issues });
      const cmd = parsed.data;

      if (await principals.findByLogin(cmd.login)) return err({ code: "duplicate_login" });
      if (await principals.findByMail(cmd.mail)) return err({ code: "duplicate_mail" });

      const created = await principals.createUser(cmd, clock.now());
      logger.log("info", "user.created", { userId: created.id, actorId: actor.id });
      return ok(created);
    },

    // ...
  };
}
```

### service.test.ts shape

```ts
import { describe, expect, it } from "vitest";
import { createFakeClock } from "@/lib/clock/fake";
import { createFakeIds } from "@/lib/ids/fake";
import { createFakeLogger } from "@/lib/logger/fake";
import { createMemoryPrincipalRepository } from "@/lib/repositories/memory/principals";
import { createMemoryMemberRepository, createMemoryRolePermissionRepository } from "@/lib/repositories/memory/members";
import { createUsersService } from "./users";

interface BuildOpts {
  permissions?: string[];
  principalsSeed?: PrincipalRecord[];
}

function buildHarness(opts: BuildOpts = {}) {
  const principals = createMemoryPrincipalRepository(opts.principalsSeed ?? []);
  const memberRoles = [{ memberId: 1, roleId: 1 }];
  const rolePermissions = (opts.permissions ?? []).map((p) => ({ roleId: 1, permission: p }));
  const members = createMemoryMemberRepository([{ id: 1, userId: 1, projectId: null }], { memberRoles, rolePermissions });
  const rolePerms = createMemoryRolePermissionRepository({ memberRoles, rolePermissions });
  const assertCan = createAssertCan(members, principals);
  const service = createUsersService({
    principals,
    clock: createFakeClock(),
    ids: createFakeIds(),
    logger: createFakeLogger(),
    assertCan,
  });
  return { service, principals };
}

describe("UsersService.create", () => {
  it("creates a user when actor has manage_users permission", async () => {
    const { service } = buildHarness({ permissions: ["manage_users"] });
    const r = await service.create({ id: 1 }, { login: "alice", mail: "alice@example.com" });
    expect(r.ok).toBe(true);
  });

  it("returns permission_denied when actor lacks the permission", async () => {
    const { service } = buildHarness({ permissions: [] });
    const r = await service.create({ id: 1 }, { login: "alice", mail: "alice@example.com" });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.code).toBe("permission_denied");
  });

  // ... more tests, all using ONLY fakes
});
```

## CONVENTIONS.md must include

```markdown
## §6 Refactor-on-the-fly translation table

| Source pattern | Target pattern | Why |
|---|---|---|
| `acts_as_journalized` (Rails) | Explicit `recordJournal()` in service | TS prefers explicit |
| `before_save :foo` (Rails) | Explicit step in service before write | No magic side effects |
| `validates :x, presence: true` (Rails) | Zod schema | TS-native validation |
| Singleton DB connection | Factory + DI | Testable, parallelizable |

## §7 Forbidden constructs

- `process.env` reads outside `lib/composition/build-container.ts`
- `export const x = createXxx(...)` at module scope (singleton exports)
- `vi.mock()` of project files in tests
- `new Date()` or `Date.now()` in business logic — use injected `Clock`
- `crypto.randomUUID()` in business logic — use injected `IdGenerator`
- Cross-module imports from `@/modules/<other>/*` — share via core helpers

## §8 Testability invariants

- Every service factory takes `deps` as a typed argument.
- Every test constructs the service via the factory with **only fakes**.
- Memory repositories pass the same observable contract as Drizzle ones.
- Tests run under happy-dom for components, no real DB, no real network.
- Tests complete in <2s per file.
```

## Why this matters for parallelism

The composition root is the ONE file that gets edited by every wave. Pattern 9 (cross-cutting waves can land red mid-flight) holds if and only if every agent's edit is **monotone-additive** — adding a slot, registering an entry, importing a new factory.

If the composition root requires removing/reshaping existing slots to register new ones, every wave conflicts. With the layout above, additions are free and removals are rare (only during refactor passes).

This is the structural reason 8 parallel agents can extend the same `build-container.ts` simultaneously and still converge. Without this discipline, parallelism is impossible at this scale.
