import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import { __reactSettingsTestHooks as hooks } from './react-settings.js';

const reactSettingsSource = readFileSync(new URL('./react-settings.js', import.meta.url), 'utf8');

globalThis.document = {
  documentElement: {
    lang: 'de',
    dataset: {},
  },
};

function baseTemplate(overrides = {}) {
  return hooks.settingsTemplate({
    modules: [{ id: 'inventory', title: 'Inventory', description: '', entry: 'index.html' }],
    managedModules: [{ id: 'inventory', title: 'Inventory', description: '', entry: 'index.html' }],
    templates: [],
    session: { user: { id: 'owner', role: 'chef' } },
    syncConfig: {},
    user: { id: 'owner', display_name: 'Owner User', role: 'chef' },
    role: 'chef',
    isAdmin: true,
    canOpenAdmin: true,
    tab: 'users',
    commandStatus: '',
    subscriptionAuth: null,
    runtimeSettings: null,
    runtimeLoading: false,
    users: [{ id: 'owner', display_name: 'Owner User', role: 'chef', active: true }],
    canManageUsers: true,
    activity: { events: [], loading: false, loaded: false, error: '' },
    branding: {
      loading: false,
      document: {
        id: 'workspace-branding',
        name: 'Acme Corporate',
        custom: true,
        light: { bg: '#ffffff', text: '#111827' },
        dark: { bg: '#030712', text: '#f9fafb' },
        module_accents: {},
        updated_at_ms: 123,
      },
      jsonText: '{\n  "name": "Acme Corporate"\n}',
      error: '',
      canManage: true,
    },
    mcp: { loading: false, info: null, error: '', copied: '' },
    editingModuleId: '',
    governance: {
      founders: {
        inventory: [{ user_id: 'founder-a', active: true }],
      },
      releases: {},
    },
    channels: { accounts: [], data: {}, status: '' },
    ...overrides,
  });
}

test('settings user tab renders business-facing role labels', () => {
  const html = baseTemplate();
  assert.match(html, /Team & Zugaenge/);
  assert.match(html, /Teammitglied/);
  assert.match(html, /App-Verantwortliche:r/);
  assert.match(html, /Owner/);
  assert.doesNotMatch(html, />User<\/option>/);
  assert.doesNotMatch(html, />Founder<\/option>/);
  assert.doesNotMatch(html, />Chef<\/option>/);
  assert.doesNotMatch(html, /User Management/);
  assert.doesNotMatch(html, /Founder Review/);
});

test('settings users stay subscribed while WebRTC replication fills the collection', () => {
  assert.match(reactSettingsSource, /db\.collection\('business_users'\)\.\$\.subscribe/);
  assert.match(reactSettingsSource, /refreshUsers\(\)\.catch/);
  assert.match(reactSettingsSource, /usersSub\?\.unsubscribe/);
});

test('settings appearance tab renders admin branding controls', () => {
  const html = baseTemplate({ tab: 'appearance' });
  assert.match(html, /data-settings-tab="appearance"/);
  assert.match(html, /Corporate Design/);
  assert.match(html, /data-branding-json/);
  assert.match(html, /data-branding-save/);
  assert.match(html, /data-branding-reset/);
  assert.match(html, /Acme Corporate/);
});

test('settings appearance tab hides when branding permission is absent', () => {
  const html = baseTemplate({
    tab: 'runtime',
    branding: {
      loading: false,
      document: null,
      jsonText: '',
      error: '',
      canManage: false,
    },
  });
  assert.doesNotMatch(html, /data-settings-tab="appearance"/);
  assert.doesNotMatch(html, /data-branding-save/);
});

test('settings MCP panel does not expose unsupported native managed token issuance', () => {
  const html = baseTemplate({
    tab: 'mcp',
    mcp: {
      loading: false,
      error: '',
      copied: '',
      info: {
        ok: true,
        status: 'local_ready_managed_not_connected',
        mode: 'local',
        server_name: 'workspace-business-os-local',
        endpoint: 'http://127.0.0.1:8788/mcp',
        managed: {
          status: 'not_connected',
          endpoint: 'https://mcp.ctox.dev/mcp/workspace',
        },
      },
    },
  });

  assert.match(html, /Managed Endpoint kopieren/);
  assert.match(html, /Agent Tokens werden im ctox\.dev Dashboard rotiert/);
  assert.doesNotMatch(html, /data-mcp-issue-managed/);
  assert.doesNotMatch(html, /Agent Token erstellen/);
  assert.doesNotMatch(reactSettingsSource, /\/api\/business-os\/mcp\/client-token/);
});

test('settings user form keeps owner transfer owner-only', () => {
  const ownerHtml = baseTemplate();
  assert.match(ownerHtml, /<option value="chef">Owner<\/option>/);

  const adminHtml = baseTemplate({
    session: { user: { id: 'ops-admin', role: 'admin' } },
    user: { id: 'ops-admin', display_name: 'Ops Admin', role: 'admin' },
    role: 'admin',
    isAdmin: true,
    users: [{ id: 'ops-admin', display_name: 'Ops Admin', role: 'admin', active: true }],
    canManageUsers: true,
  });
  assert.match(adminHtml, /<option value="admin">Admin<\/option>/);
  assert.doesNotMatch(adminHtml, /<option value="chef">Owner<\/option>/);
});

test('settings user save keeps confirmed upsert result without stale projection reload', () => {
  const users = hooks.confirmedUsersAfterUpsert(
    [
      { id: 'alice', display_name: 'Alice Old', role: 'admin', active: true },
      { id: 'bob', display_name: 'Bob', role: 'user', active: true },
    ],
    null,
    { id: 'alice', display_name: 'Alice New', role: 'admin', active: true, updated_at_ms: 99 },
    { user: { id: 'owner', role: 'admin' } },
  );

  assert.deepEqual(users.map((user) => [user.id, user.display_name]), [
    ['alice', 'Alice New'],
    ['bob', 'Bob'],
  ]);
});

test('settings module tab renders responsible-app wording', () => {
  const html = baseTemplate({ tab: 'admin' });
  assert.match(html, /Verantwortlich: founder-a/);
  assert.match(html, /Verantwortliche:n zuweisen/);
  assert.match(html, /data-module-why="inventory"/);
  assert.match(html, /data-module-support-diagnostics="inventory"/);
  assert.match(html, /Warum\?/);
  assert.match(html, /Support-Paket/);
  assert.match(html, /Freigabe im App Store/);
  assert.match(html, /Settings zeigt Release und Rollback nur als Diagnose/);
  assert.doesNotMatch(html, /data-module-release="/);
  assert.doesNotMatch(html, /data-module-rollback="/);
  assert.doesNotMatch(html, /Founder:/);
  assert.doesNotMatch(html, /Founder zuweisen/);
  assert.doesNotMatch(html, /Keine Datenbereiche deklariert/);
});

test('settings module why diagnostics renders native policy result without raw payload keys', () => {
  const html = baseTemplate({
    tab: 'admin',
    moduleWhyDiagnostics: {
      inventory: {
        ok: true,
        schema_version: 1,
        kind: 'business_os_why_diagnostics',
        actor: {
          id: 'qa-user',
          display_name: 'QA User',
          role: 'user',
        },
        module: {
          id: 'inventory',
          title: 'Inventory',
          version: '0.9.0',
          runtime_installed: true,
        },
        lifecycle: {
          visibility_state: 'private',
          current_semver: '0.9.0',
          public: false,
        },
        decisions: {
          visibility: {
            allowed: true,
            permission: 'apps.view',
            reason_code: 'explicit_or_responsible_app_view',
            display_reason: 'Diese App ist durch App-Verantwortung sichtbar.',
            source: 'explicit_app_view_or_app_responsibility',
          },
          open: {
            allowed: true,
            permission: 'apps.view',
            reason_code: 'explicit_or_responsible_app_view',
            display_reason: 'Allowed.',
          },
          modify: {
            allowed: false,
            permission: 'apps.modify',
            reason_code: 'role_or_scope_denied',
            display_reason: 'App-Aenderungen bleiben Verantwortlichen, Admins oder Ownern vorbehalten.',
          },
          source: {
            allowed: false,
            permission: 'apps.source.view',
            reason_code: 'role_or_scope_denied',
            display_reason: 'This role is not allowed to perform this action for the selected scope.',
          },
          release: {
            allowed: false,
            permission: 'apps.release',
            reason_code: 'role_or_scope_denied',
            display_reason: 'Freigaben brauchen ein Release-Recht fuer diese App.',
          },
          rollback: {
            allowed: false,
            permission: 'apps.rollback',
            reason_code: 'role_or_scope_denied',
            display_reason: 'Rollback braucht ein Rollback-Recht fuer diese App.',
          },
        },
        data_access: {
          status: 'reviewed',
          completed: true,
          areas: [
            {
              collection: 'inventory_items',
              read_review_state: 'granted',
              write_review_state: 'locked',
              read_decision: {
                allowed: true,
                permission: 'data.read',
                display_reason: 'Datenbereich ist zum Lesen freigegeben.',
                collection_decision: { grant_id: 'DO_NOT_LEAK_GRANT_ID' },
              },
              write_decision: {
                allowed: false,
                permission: 'data.write',
                reason_code: 'role_or_scope_denied',
                display_reason: 'Schreiben braucht ein Datenrecht fuer diesen Bereich.',
                module_decision: { reason_code: 'DO_NOT_LEAK_REASON_CODE' },
              },
            },
          ],
        },
        prompt: 'DO_NOT_LEAK_PROMPT',
        token: 'DO_NOT_LEAK_TOKEN',
        selected_text: 'DO_NOT_LEAK_SELECTION',
      },
    },
  });

  assert.match(html, /data-why-diagnostics="inventory"/);
  assert.match(html, /QA User · Teammitglied/);
  assert.match(html, /private · v0\.9\.0/);
  assert.match(html, /App ändern/);
  assert.match(html, /Nicht erlaubt/);
  assert.match(html, /1 Datenbereich\(e\): 1 lesbar, 0 schreibbar/);
  assert.match(html, /Inventory Items/);
  assert.match(html, /Lesen Erlaubt/);
  assert.match(html, /Schreiben Nicht erlaubt/);
  assert.doesNotMatch(html, /policy_decision/);
  assert.doesNotMatch(html, /collection_decision/);
  assert.doesNotMatch(html, /module_decision/);
  assert.doesNotMatch(html, /reason_code/);
  assert.doesNotMatch(html, /apps\.modify/);
  assert.doesNotMatch(html, /role_or_scope_denied/);
  assert.doesNotMatch(html, /Allowed\./);
  assert.doesNotMatch(html, /This role is not allowed/);
  assert.doesNotMatch(html, /DO_NOT_LEAK/);
});

test('settings module tab renders read-only agent grant boundary', () => {
  const html = baseTemplate({
    tab: 'admin',
    governance: {
      founders: {
        inventory: [{ user_id: 'founder-a', active: true }],
      },
      releases: {},
      permission_model: {
        explicit_grants: [
          {
            grant_id: 'grant-agent-inventory-view',
            subject_type: 'mcp_actor',
            subject_id: 'chatgpt:agent',
            permission: 'apps.view',
            scope_type: 'module',
            scope_id: 'inventory',
            active: true,
          },
          {
            grant_id: 'grant-agent-customer-read',
            subject_type: 'mcp_actor',
            subject_id: 'chatgpt:agent',
            permission: 'data.read',
            scope_type: 'collection',
            scope_id: 'customer_accounts',
            active: true,
          },
        ],
      },
    },
  });

  assert.match(html, /data-agent-grant-boundary/);
  assert.match(html, /Agent- und App-Zugriff/);
  assert.match(html, /2 Sonderfreigaben/);
  assert.match(html, /Änderungen laufen über Owner\/Admin-Policy/);
  assert.match(html, /Agent/);
  assert.match(html, /chatgpt:agent/);
  assert.match(html, /App sehen/);
  assert.match(html, /App Inventory/);
  assert.match(html, /Daten lesen/);
  assert.match(html, /Datenbereich Customer Accounts \(customer_accounts\)/);
  assert.doesNotMatch(html, /grant-agent-inventory-view/);
  assert.doesNotMatch(html, /data-agent-grant-save/);
});

test('settings module tab renders projected release and data access facts', () => {
  const html = baseTemplate({
    tab: 'admin',
    managedModules: [{
      id: 'inventory',
      title: 'Inventory',
      description: '',
      entry: 'installed-modules/inventory/index.html',
      install_scope: 'installed',
      version: '1.2.0',
      lifecycle: {
        runtime_installed: true,
        release_status: 'released',
        release_state: {
          status: 'released',
          current: {
            version_id: 'version-current',
            version: 4,
            target_version: '1.2.0',
          },
          history_count: 4,
        },
        rollback_target: {
          version_id: 'version-prev',
          version: 3,
          target_version: '1.1.0',
        },
        data_access: {
          status: 'reviewed',
          completed: true,
          granted_collection_ids: ['inventory_items'],
          locked_collection_ids: ['supplier_prices'],
          review_is_evidence_only: true,
          grants_implied: false,
          areas: [
            { collection: 'inventory_items', decision: 'granted' },
            { collection: 'supplier_prices', decision: 'locked' },
          ],
        },
      },
    }],
    governance: {
      founders: {
        inventory: [{ user_id: 'founder-a', active: true }],
      },
      releases: {
        inventory: [{ version_id: 'version-prev', version: 3, status: 'rolled_back' }],
      },
    },
  });

  assert.match(html, /data-module-release-projection="inventory"/);
  assert.match(html, /<select disabled aria-label="Rollback-Versionen nur Diagnose">/);
  assert.match(html, /Rollback nur Diagnose/);
  assert.doesNotMatch(html, /data-rollback-version=/);
  assert.doesNotMatch(html, /data-module-rollback=/);
  assert.match(html, /Freigabe:<\/b> Aktuell v1\.2\.0/);
  assert.match(html, /Rollback:<\/b> Rollback-Ziel v1\.1\.0/);
  assert.match(html, /Datenzugriff:<\/b> Freigegeben: Inventory Items \(inventory_items\); Gesperrt: Supplier Prices \(supplier_prices\)/);
  assert.match(html, /Review:<\/b> Review ist Nachweis; Datenrechte bleiben explizit\./);
  assert.doesNotMatch(html, /granted_collection_ids/);
  assert.doesNotMatch(html, /locked_collection_ids/);
});

test('settings module support diagnostics renders support-safe artifact summary without raw keys', () => {
  const html = baseTemplate({
    tab: 'admin',
    moduleSupportDiagnostics: {
      inventory: {
        ok: true,
        kind: 'business_os_support_diagnostics_artifact',
        schema_version: 1,
        artifact_schema: 'ctox.business_os.support_diagnostics.v1',
        generated_at_ms: 1781712006000,
        redaction: {
          profile: 'support-safe-v1',
          excluded_fields: [
            'prompt',
            'selected_text',
            'message_body',
            'record_payload',
            'payload_json',
            'token',
            'secret',
          ],
        },
        actor: {
          id: 'ops-admin',
          display_name: 'Ops Admin',
          role: 'admin',
        },
        scope: {
          module_id: 'inventory',
        },
        diagnostics: {
          why: {
            kind: 'business_os_why_diagnostics_summary',
            module: {
              id: 'inventory',
              title: 'Inventory',
              version: '1.2.0',
            },
            lifecycle: {
              visibility_state: 'team',
              current_semver: '1.2.0',
            },
            decisions: {
              modify: {
                allowed: false,
                permission: 'apps.modify',
                reason_code: 'role_or_scope_denied',
                display_reason: 'DO_NOT_LEAK_REASON',
              },
              release: {
                allowed: true,
                permission: 'apps.release',
                reason_code: 'role_default',
                display_reason: 'DO_NOT_LEAK_RELEASE_REASON',
              },
            },
            data_access: {
              areas: [
                {
                  collection: 'inventory_items',
                  read_decision: { allowed: true, permission: 'data.read' },
                  write_decision: { allowed: false, permission: 'data.write' },
                },
              ],
            },
          },
        },
        activity: {
          count: 2,
          events: [
            {
              policy_decision: {
                permission: 'apps.modify',
                reason_code: 'DO_NOT_LEAK_ACTIVITY_REASON',
              },
              client_scope: {
                source: 'DO_NOT_LEAK_SOURCE',
              },
            },
          ],
        },
        prompt: 'DO_NOT_LEAK_PROMPT',
        selected_text: 'DO_NOT_LEAK_SELECTION',
        token: 'DO_NOT_LEAK_TOKEN',
        secret: 'DO_NOT_LEAK_SECRET',
        payload_json: { marker: 'DO_NOT_LEAK_PAYLOAD' },
      },
    },
  });

  assert.match(html, /data-support-diagnostics="inventory"/);
  assert.match(html, /data-support-schema="ctox\.business_os\.support_diagnostics\.v1"/);
  assert.match(html, /data-redaction-profile="support-safe-v1"/);
  assert.match(html, /CTOX Support-Diagnose/);
  assert.match(html, /Support-sicher/);
  assert.match(html, /Keine Nachrichteninhalte, KI-Eingaben, Datensatzinhalte oder Zugangswerte enthalten/);
  assert.match(html, /Inventory/);
  assert.match(html, /Team sichtbar · v1\.2\.0/);
  assert.match(html, /2 Ereignisse zusammengefasst/);
  assert.match(html, /Ändern gesperrt · Freigabe erlaubt/);
  assert.match(html, /1 Datenbereich\(e\): 1 lesbar, 0 schreibbar/);
  assert.doesNotMatch(html, /policy_decision/);
  assert.doesNotMatch(html, /reason_code/);
  assert.doesNotMatch(html, /apps\.modify/);
  assert.doesNotMatch(html, /apps\.release/);
  assert.doesNotMatch(html, /payload_json/);
  assert.doesNotMatch(html, /record_payload/);
  assert.doesNotMatch(html, /selected_text/);
  assert.doesNotMatch(html, /message_body/);
  assert.doesNotMatch(html, /\bprompt\b/i);
  assert.doesNotMatch(html, /\btoken\b/i);
  assert.doesNotMatch(html, /\bsecret\b/i);
  assert.doesNotMatch(html, /DO_NOT_LEAK/);
});

test('settings source does not expose an active stale release dispatch path', () => {
  assert.match(reactSettingsSource, /commandType:\s*['"]ctox\.business_os\.why['"]/);
  assert.match(reactSettingsSource, /commandType:\s*['"]ctox\.business_os\.support\.export_diagnostics['"]/);
  assert.match(reactSettingsSource, /data-module-why="\$\{escapeAttr\(mod\.id\)\}"/);
  assert.match(reactSettingsSource, /data-module-support-diagnostics="\$\{escapeAttr\(mod\.id\)\}"/);
  assert.doesNotMatch(reactSettingsSource, /commandType:\s*['"]ctox\.module\.release['"]/);
  assert.doesNotMatch(reactSettingsSource, /commandType:\s*['"]ctox\.module\.rollback['"]/);
  assert.doesNotMatch(reactSettingsSource, /data-module-release="\$\{/);
  assert.doesNotMatch(reactSettingsSource, /data-module-rollback="\$\{/);
  assert.doesNotMatch(reactSettingsSource, /data-rollback-version="\$\{/);
});

test('settings activity tab renders business-facing audit labels', () => {
  const html = baseTemplate({
    tab: 'activity',
    activity: {
      events: [
        {
          type: 'business_os.user.changed',
          observed_at_ms: 1781712000000,
          payload: {
            event_type: 'business_os.user.changed',
            actor: { display_name: 'Ops Admin' },
            previous: { id: 'member', display_name: 'Member', role: 'user', active: true },
            current: { id: 'member', display_name: 'Member', role: 'founder', active: true },
          },
        },
        {
          type: 'business_os.app_responsibility.changed',
          observed_at_ms: 1781712001000,
          payload: {
            event_type: 'business_os.app_responsibility.changed',
            actor: { display_name: 'Ops Admin' },
            module_id: 'inventory',
            user_id: 'module-owner',
            current: { module_id: 'inventory', user_id: 'module-owner', active: true },
          },
        },
        {
          type: 'business_os.policy.denied',
          observed_at_ms: 1781712002000,
          payload: {
            event_type: 'business_os.policy.denied',
            actor: { display_name: 'Viewer' },
            command_type: 'ctox.module.save',
            policy_decision: {
              permission: 'apps.modify',
              reason_code: 'role_or_scope_denied',
            },
          },
        },
        {
          type: 'business_os.policy.allowed',
          observed_at_ms: 1781712002500,
          payload: {
            event_type: 'business_os.policy.allowed',
            actor: { display_name: 'Ops Admin' },
            command_type: 'ctox.module.save',
            policy_decision: {
              permission: 'apps.modify',
              reason_code: 'allowed',
            },
          },
        },
        {
          type: 'business_os.external_approval.decided',
          observed_at_ms: 1781712003000,
          payload: {
            event_type: 'business_os.external_approval.decided',
            actor: { display_name: 'Ops Admin' },
            approval_id: 'approval-1',
            message_id: 'msg-1',
            decision: 'approved',
            message: {
              id: 'msg-1',
              approval_status: 'approved',
            },
          },
        },
        {
          type: 'business_os.module.release.succeeded',
          observed_at_ms: 1781712004000,
          payload: {
            event_type: 'business_os.module.release.succeeded',
            actor: { display_name: 'Release Owner' },
            summary: {
              module_id: 'inventory',
              target_version: '1.0.0',
              release_channel: 'team',
              data_access_review: {
                locked_collection_ids: ['secret_inventory_collection'],
              },
            },
          },
        },
        {
          type: 'business_os.module.rollback.succeeded',
          observed_at_ms: 1781712005000,
          payload: {
            event_type: 'business_os.module.rollback.succeeded',
            actor: { display_name: 'Release Owner' },
            summary: {
              module_id: 'inventory',
              version_id: 'version_baseline_secret_id',
              rollback_check: 'completed',
            },
          },
        },
      ],
      loading: false,
      loaded: true,
      error: '',
    },
  });
  assert.match(html, /Aktivität/);
  assert.match(html, /Teammitglied aktualisiert/);
  assert.match(html, /Member: Teammitglied -&gt; App-Verantwortliche:r/);
  assert.match(html, /App-Verantwortung aktualisiert/);
  assert.match(html, /module-owner ist verantwortlich für inventory/);
  assert.match(html, /Aktion blockiert/);
  assert.match(html, /Modul ändern wurde blockiert/);
  assert.match(html, /Aktion erlaubt/);
  assert.match(html, /Modul ändern wurde erlaubt/);
  assert.match(html, /Externe Freigabe entschieden/);
  assert.match(html, /msg-1: freigegeben/);
  assert.match(html, /App-Version veröffentlicht/);
  assert.match(html, /inventory: Version 1\.0\.0 wurde für Team veröffentlicht/);
  assert.match(html, /App-Rollback angewendet/);
  assert.match(html, /inventory: Rollback auf gewählte Version wurde angewendet/);
  assert.doesNotMatch(html, /business_os\.user\.changed/);
  assert.doesNotMatch(html, /business_os\.policy\.allowed/);
  assert.doesNotMatch(html, /business_os\.module\.release/);
  assert.doesNotMatch(html, /business_os\.module\.rollback/);
  assert.doesNotMatch(html, /business_os\.external_approval\.decided/);
  assert.doesNotMatch(html, /reason_code/);
  assert.doesNotMatch(html, /apps\.modify/);
  assert.doesNotMatch(html, /role_or_scope_denied/);
  assert.doesNotMatch(html, /locked_collection_ids/);
  assert.doesNotMatch(html, /secret_inventory_collection/);
  assert.doesNotMatch(html, /version_baseline_secret_id/);
  assert.doesNotMatch(html, /rollback_check/);
});
