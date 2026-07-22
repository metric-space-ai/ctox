import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import fs from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

if (typeof globalThis.DOMParser !== 'function') {
  class TestXmlNode {
    constructor(name = '', attrs = {}, parent = null) {
      this.name = name;
      this.attrs = attrs;
      this.parent = parent;
      this.children = [];
      this.text = '';
    }

    get textContent() {
      return `${this.text}${this.children.map((child) => child.textContent).join('')}`;
    }

    getAttribute(name) {
      return this.attrs[name] || '';
    }

    getAttributeNS(_namespace, name) {
      return this.getAttribute(name) || this.getAttribute(`r:${name}`);
    }

    querySelector(selector) {
      return this.querySelectorAll(selector)[0] || null;
    }

    querySelectorAll(selector) {
      const parts = String(selector || '').trim().split(/\s+/).filter(Boolean);
      if (!parts.length) return [];
      let scope = [this];
      for (const part of parts) {
        scope = scope.flatMap((node) => node.descendantsByName(part));
      }
      return scope;
    }

    descendantsByName(name) {
      const result = [];
      for (const child of this.children) {
        if (child.name === name || child.name.endsWith(`:${name}`)) result.push(child);
        result.push(...child.descendantsByName(name));
      }
      return result;
    }
  }

  globalThis.DOMParser = class {
    parseFromString(xml) {
      const root = new TestXmlNode('#document');
      const stack = [root];
      const tokens = String(xml || '').match(/<!--[\s\S]*?-->|<[^>]+>|[^<]+/g) || [];
      for (const token of tokens) {
        if (token.startsWith('<!--') || token.startsWith('<?')) continue;
        if (token.startsWith('</')) {
          if (stack.length > 1) stack.pop();
          continue;
        }
        if (token.startsWith('<')) {
          const selfClosing = /\/>\s*$/.test(token);
          const body = token.replace(/^</, '').replace(/\/?>$/, '').trim();
          const [name = '', ...attrParts] = body.match(/[^\s=]+(?:=(?:"[^"]*"|'[^']*'))?/g) || [];
          if (!name || name.startsWith('!')) continue;
          const attrs = {};
          attrParts.forEach((part) => {
            const match = part.match(/^([^=]+)=["']([\s\S]*)["']$/);
            if (match) attrs[match[1]] = match[2];
          });
          const node = new TestXmlNode(name, attrs, stack.at(-1));
          stack.at(-1).children.push(node);
          if (!selfClosing) stack.push(node);
          continue;
        }
        stack.at(-1).text += token;
      }
      return root;
    }
  };
}

const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
});

const [{ text: bundledSource }] = bundledModule.outputFiles;
const { __outboundTestHooks: hooks } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

test('presentation layer stays compact and shell-native', async () => {
  const css = await fs.readFile(new URL('./index.css', import.meta.url), 'utf8');
  const js = await fs.readFile(new URL('./index.js', import.meta.url), 'utf8');
  const html = await fs.readFile(new URL('./index.html', import.meta.url), 'utf8');
  const source = `${css}\n${js}`;
  const surfacePattern = new RegExp(['ctox-pane--gla' + 'ss', 'gla' + 'ss', 'Prem' + 'ium'].join('|'), 'i');
  const sidePattern = new RegExp('border-' + '(?:left|right)\\s*:\\s*(?:[2-9]|[0-9]{2,})px');
  const radiusPattern = new RegExp('border-' + 'radius:\\s*(?:8|10|12|14|16|18|20|24)px');
  const shadowPattern = new RegExp('box-' + 'shadow:\\s*(?:0|inset|rgba|color-mix|var\\(--panel-shadow\\)|var\\(--shadow-sm\\)|var\\(--shadow-md\\))');
  const gradientPattern = new RegExp(['linear-grad' + 'ient', 'radial-grad' + 'ient'].join('|'));

  assert.doesNotMatch(source, surfacePattern);
  assert.doesNotMatch(source, sidePattern);
  assert.doesNotMatch(source, radiusPattern);
  assert.doesNotMatch(source, shadowPattern);
  assert.doesNotMatch(source, gradientPattern);
  // The module frame is the standard kit scaffold: workspace + panes + the
  // declarative column resizer driving --ctox-left-width.
  assert.match(html, /ctox-workspace ctox-workspace--two-pane outbound-module/);
  assert.match(html, /data-resizer-var="--ctox-left-width"/);
  assert.match(html, /data-campaign-editor-modal hidden/);
  assert.match(css, /\.outbound-mailserver-domain-card/);
  assert.match(css, /\.outbound-left\s*\{[\s\S]*grid-column:\s*1;[\s\S]*grid-template-rows:\s*auto auto minmax\(0, 1fr\) auto;/);
  assert.match(css, /\.outbound-center\s*\{[\s\S]*grid-column:\s*3;[\s\S]*grid-template-rows:\s*auto minmax\(0, 1fr\);/);
  assert.match(css, /grid-template-columns:\s*minmax\(280px,[^;]+\) 12px minmax\(360px, 1fr\)/);
});

test('campaign column uses shell-owned grammar and stable in-place selection', async () => {
  const js = await fs.readFile(new URL('./index.js', import.meta.url), 'utf8');
  const activeJs = await fs.readFile(new URL('./active-outreach.js', import.meta.url), 'utf8');
  const css = await fs.readFile(new URL('./index.css', import.meta.url), 'utf8');

  assert.match(js, /ctox-pane-grammar-change/);
  assert.match(js, /data-pg-search/);
  assert.match(js, /data-pg-view="cards"/);
  assert.match(js, /data-pg-view="list"/);
  assert.match(js, /data-pg-tray-toggle/);
  assert.match(js, /data-pg-reset/);
  assert.match(js, /data-pg-band=/);
  assert.match(js, /ctox-pane-body ctox-well/);
  assert.match(js, /ctox-pane-footer/);
  assert.match(js, /import-campaign-records/);
  assert.match(js, /export-campaign-records/);
  assert.match(js, /type="file" accept="application\/json,\.json"/);
  assert.match(js, /new Blob\(\[JSON\.stringify\(payload, null, 2\)\], \{ type: 'application\/json' \}\)/);
  assert.match(js, /root\.__ctoxPaneGrammar/);
  assert.match(js, /const needsChrome = !root\.querySelector\('\[data-campaign-list\]'\)/);
  assert.match(js, /const sameCampaignChrome = root\.dataset\.renderedCampaignId === campaign\.id/);
  assert.match(js, /function updateCampaignSelectionInPlace/);
  assert.match(js, /row\.classList\.toggle\('is-selected', selected\)/);
  assert.match(js, /function updateQualificationSelectionInPlace/);
  assert.match(js, /state\.ctx\?\.storageScope\?\.get\?\.\(OUTBOUND_CENTER_SPLIT_KEY\)/);
  assert.match(js, /state\.ctx\?\.storageScope\?\.set\?\.\(OUTBOUND_CENTER_SPLIT_KEY/);
  assert.match(js, /openBusinessChat = state\.ctx\?\.openBusinessChat/);
  assert.match(activeJs, /function selectEngagementInPlace/);
  assert.doesNotMatch(`${js}\n${activeJs}`, /localStorage|sessionStorage|window\.dispatchEvent|ctox-business-os-chat-submit/);
  assert.doesNotMatch(css, /\.ctox-filterbar\s*\{|\.ctox-filter-tray\s*\{|\.ctox-view-switch\s*\{|\.ctox-well\s*\{|\.ctox-pane-footer\s*\{/);
});

test('campaign scope recovers existing outbound rows for the only visible campaign', () => {
  const scoped = hooks.campaignScopedRows({
    campaigns: [{ id: 'outbound_default_campaign', name: 'Outbound Firmenqualifizierung' }],
    sources: [{ id: 'src-1', campaign_id: 'legacy-campaign', title: 'Legacy import' }],
    companies: [{ id: 'co-1', campaign_id: 'legacy-campaign', name: 'Acme GmbH' }],
    pipeline: [{ id: 'pipe-1', campaign_id: 'legacy-campaign', company_id: 'co-1', company_name: 'Acme GmbH' }],
  }, 'outbound_default_campaign');

  assert.equal(scoped.recovered, true);
  assert.deepEqual(scoped.companies.map((item) => item.id), ['co-1']);
  assert.deepEqual(scoped.pipeline.map((item) => item.id), ['pipe-1']);
});

test('campaign scope does not mix unrelated rows when direct campaign data exists', () => {
  const scoped = hooks.campaignScopedRows({
    campaigns: [
      { id: 'camp-a', name: 'A' },
      { id: 'camp-b', name: 'B' },
    ],
    sources: [],
    companies: [
      { id: 'co-a', campaign_id: 'camp-a', name: 'A GmbH' },
      { id: 'co-b', campaign_id: 'camp-b', name: 'B GmbH' },
    ],
    pipeline: [
      { id: 'pipe-a', campaign_id: 'camp-a', company_id: 'co-a', company_name: 'A GmbH' },
      { id: 'pipe-b', campaign_id: 'camp-b', company_id: 'co-b', company_name: 'B GmbH' },
    ],
  }, 'camp-a');

  assert.deepEqual(scoped.companies.map((item) => item.id), ['co-a']);
  assert.deepEqual(scoped.pipeline.map((item) => item.id), ['pipe-a']);
});

test('outbound import validation requires source-specific input', () => {
  assert.equal(hooks.validateOutboundImportPayload({ title: '', source_type: 'text', source: { text: 'Acme' } }).valid, false);
  assert.equal(hooks.validateOutboundImportPayload({ title: 'Import', source_type: 'text', source: { text: '' } }).valid, false);
  assert.equal(hooks.validateOutboundImportPayload({ title: 'Import', source_type: 'url', source: { url: 'not-a-url' } }).valid, false);
  assert.equal(hooks.validateOutboundImportPayload({ title: 'Import', source_type: 'excel', source: { files: [] } }).valid, false);
  assert.equal(hooks.validateOutboundImportPayload({ title: 'Import', source_type: 'excel', source: { files: [{ name: 'companies.csv' }] } }).valid, true);
});

test('research source policy includes DACH defaults and credential references only', () => {
  const sources = hooks.normalizeResearchSources();
  const ids = sources.map((source) => source.id);

  assert.ok(ids.includes('northdata.de'));
  assert.ok(ids.includes('leadfeeder.com'));
  assert.ok(ids.includes('dnbhoovers.com'));
  assert.ok(ids.includes('companyhouse.de'));
  assert.ok(ids.includes('linkedin.com'));
  assert.ok(ids.includes('xing.com'));
  assert.ok(ids.includes('firmenabc.at'));
  assert.ok(ids.includes('moneyhouse.ch'));
  assert.ok(ids.includes('zefix.ch'));

  const policy = hooks.researchSourcePolicyForPrompt({ researchSources: sources });
  assert.ok(policy.preferred_sources.includes('moneyhouse.ch'));
  assert.ok(policy.preferred_sources.includes('zefix.ch'));
  assert.ok(policy.include_private.includes('leadfeeder.com'));
  assert.ok(policy.include_private.includes('dnbhoovers.com'));
  assert.ok(policy.include_private.includes('linkedin.com'));
  assert.ok(policy.include_private.includes('xing.com'));
  assert.equal(policy.min_independent_sources, 2);
  assert.equal(policy.allow_generic_web_fallback, true);
  assert.equal(policy.secret_value_in_payload, false);
  assert.ok(policy.source_adapters.some((source) => source.source_id === 'northdata.de' && source.target_key === 'northdata-de'));
  assert.ok(policy.source_adapters.some((source) => source.source_id === 'companyhouse.de' && source.target_key === 'companyhouse-de'));
  assert.ok(policy.source_adapters.some((source) => source.source_id === 'firmenabc.at' && source.target_key === 'firmenabc-at'));
  assert.ok(policy.source_adapters.some((source) => source.source_id === 'moneyhouse.ch' && source.target_key === 'moneyhouse-ch'));
  assert.ok(policy.source_adapters.every((source) => source.secret_value_in_payload === false));
  assert.equal(JSON.stringify(policy).includes('credential_value'), false);
});

test('research source URLs normalize aliases and disabled sources stay out of policy', () => {
  assert.equal(hooks.normalizeResearchSourceId('https://www.moneyhouse.ch/de/company/acme-123'), 'moneyhouse.ch');
  assert.equal(hooks.normalizeResearchSourceId('https://www.zefix.admin.ch/'), 'zefix.ch');
  assert.equal(hooks.normalizeResearchSourceId('https://app.dnbhoovers.com/login'), 'dnbhoovers.com');

  const sources = hooks.normalizeResearchSources([
    { id: 'linkedin.com', enabled: false },
    { url: 'https://app.dnbhoovers.com/login', requiresCredential: true, credentialSecretName: 'DNB_DIRECT_API_KEY' },
    { id: 'research.partner.example', label: 'Partner Research', url: 'https://research.partner.example/', requiresCredential: true, credentialSecretName: 'PARTNER_RESEARCH_TOKEN' },
  ]);
  const policy = hooks.researchSourcePolicyForPrompt({ researchSources: sources });

  assert.equal(policy.preferred_sources.includes('linkedin.com'), false);
  assert.ok(policy.preferred_sources.includes('dnbhoovers.com'));
  assert.ok(policy.preferred_sources.includes('research.partner.example'));
  assert.ok(policy.include_private.includes('dnbhoovers.com'));
  assert.ok(policy.include_private.includes('research.partner.example'));
  assert.ok(policy.sources.some((source) => source.id === 'research.partner.example' && source.credential_secret_name === 'PARTNER_RESEARCH_TOKEN'));
  assert.ok(policy.source_adapters.some((source) => source.source_id === 'research.partner.example' && source.target_key === 'research-partner-example'));
});

test('research source adapter lifecycle is command-bus based', () => {
  assert.match(bundledSource, /outbound_research_adapters/);
  assert.match(bundledSource, /outbound\.research_source\.generate_adapter/);
  assert.match(bundledSource, /outbound\.research_source\.test/);
  assert.match(bundledSource, /outbound\.research_source\.auth_assist/);
  assert.match(bundledSource, /universal-scraping/);
  assert.doesNotMatch(bundledSource, /\/api\/business-os\/research-source/);
});

test('research source adapter command result preserves server status', () => {
  const adapter = hooks.researchAdapterFromCommandResult({
    status: 'completed',
    result: {
      ok: true,
      adapter: {
        id: 'adapter-1',
        status: 'adapter_ready',
        scrape_status: 'registered',
        auth_status: 'not_required',
        last_run_id: 'cmd-1',
        payload: {
          scrape_registry_effect: { script_registered: true },
          secret_value_in_payload: false,
        },
      },
    },
  });
  const patch = hooks.researchAdapterPatchFromServer(adapter);

  assert.equal(patch.status, 'adapter_ready');
  assert.equal(patch.scrape_status, 'registered');
  assert.equal(patch.auth_status, 'not_required');
  assert.equal(patch.payload.secret_value_in_payload, false);
});

test('outbound import extracts company rows from uploaded Excel workbooks', async (t) => {
  const workbookPath = process.env.OUTBOUND_XLSX_FIXTURE;
  if (!workbookPath) {
    t.skip('set OUTBOUND_XLSX_FIXTURE to a .xlsx workbook path to run this import test');
    return;
  }
  const buffer = await fs.readFile(workbookPath);
  const rows = await hooks.extractRowsFromPayload({
    source_type: 'excel',
    source: {
      files: [
        {
          name: 'Personalvermittler.xlsx',
          base64: buffer.toString('base64'),
        },
      ],
    },
  });

  assert.ok(rows.length > 0, 'expected at least one company row from the uploaded workbook');
  assert.ok(rows.every((row) => row.name), 'every extracted row needs a company name');
});

test('every campaign idea template is actionable and channel-explicit', () => {
  for (const lang of ['de', 'en']) {
    const templates = hooks.campaignIdeaTemplates(lang);
    const ids = new Set();
    const titles = new Set();

    assert.equal(templates.length, 20);

    for (const template of templates) {
      assert.ok(template.id.startsWith(`${lang}-`), `${template.id} should use the language prefix`);
      assert.equal(ids.has(template.id), false, `${template.id} must be unique`);
      assert.equal(titles.has(template.title), false, `${template.title} must be unique`);
      ids.add(template.id);
      titles.add(template.title);

      assert.ok(template.title.length >= 12, `${template.id} needs a useful title`);
      assert.ok(template.text.length >= 180, `${template.id} needs a concrete natural-language briefing`);
      assert.match(template.text, /(?:ich möchte|I want to)/i, `${template.id} should read like a natural user request`);

      if (template.id.includes('-mail-')) {
        assert.match(template.text, /(?:E-Mail|email)/i, `${template.id} must explicitly name email as the channel`);
      }
      if (template.id.includes('-letter-')) {
        assert.match(template.text, /(?:Brief|Briefe|physical letter|physical letters|letter templates|printable letters)/i, `${template.id} must explicitly name physical letters as the channel`);
      }

      const prompt = hooks.campaignSetupPrompt(
        {
          id: `camp-${template.id}`,
          name: template.title,
          payload: {
            briefing: template.text,
            briefing_template_id: template.id,
            briefing_language: lang,
          },
        },
        `cmd-${template.id}`,
        template,
      );

      assert.match(prompt, new RegExp(template.id.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')), `${template.id} should be included in setup prompt`);
      assert.match(prompt, new RegExp(template.title.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')), `${template.id} title should be included in setup prompt`);
      assert.match(prompt, /outbound\.campaign\.apply_setup/);
      assert.match(prompt, /keine HTTP-Datenkanaele/i);
    }
  }
});

test('campaign briefing uses stored free text as the central campaign instruction', () => {
  const campaign = {
    objective: 'Old objective',
    payload: {
      scope: 'Old scope',
      briefing: 'Ich möchte 100 Handwerksbetriebe per Mail anschreiben.',
    },
  };

  assert.equal(hooks.campaignBriefing(campaign), 'Ich möchte 100 Handwerksbetriebe per Mail anschreiben.');
  assert.equal(hooks.campaignBriefingSummary(campaign), 'Ich möchte 100 Handwerksbetriebe per Mail anschreiben.');
});

test('campaign briefing save opens the current CTOX Business Chat contract for the setup skill', () => {
  assert.match(bundledSource, /openBusinessChat/);
  assert.doesNotMatch(bundledSource, /ctox-business-os-chat-submit|window\.dispatchEvent/);
  assert.match(bundledSource, /business-os-outbound-campaign-setup/);
  assert.match(bundledSource, /outbound\.campaign\.briefing\.update/);
  assert.match(bundledSource, /function dispatchOutboundPromptTask/);
  assert.match(bundledSource, /action:\s*['"]context-chat['"]/);
  assert.match(bundledSource, /reuseActive:\s*false/);
  assert.match(bundledSource, /business_os\.chat\.task/);
  assert.match(bundledSource, /outbound\.campaign\.apply_setup/);
  assert.doesNotMatch(bundledSource, /\/api\/business-os\/commands/);

  const prompt = hooks.campaignSetupPrompt(
    {
      id: 'camp-1',
      name: 'Nord-Handwerk',
      payload: {
        briefing: 'Ich möchte 100 Handwerksbetriebe in Norddeutschland per Mail anschreiben.',
        briefing_template_id: 'de-mail-handwerk-nord',
      },
    },
    'cmd-setup-1',
    { id: 'de-mail-handwerk-nord', title: 'Handwerk in Norddeutschland per E-Mail' },
  );

  assert.match(prompt, /Nutze den CTOX Skill business-os-outbound-campaign-setup/);
  assert.match(prompt, /keine HTTP-Datenkanaele/i);
  assert.match(prompt, /outbound\.campaign\.apply_setup/);
  assert.match(prompt, /cmd-setup-1/);
  assert.match(prompt, /selected_template_id: de-mail-handwerk-nord/);
  assert.match(prompt, /selected_template_title: Handwerk in Norddeutschland per E-Mail/);
});

test('outbound prompt tasks use ctx.openBusinessChat with the context-chat contract', () => {
  const opened = [];
  const detail = hooks.dispatchOutboundPromptTask({
    text: 'Bitte richte diese Outbound-Kampagne ein.',
    commandId: 'cmd-context-chat-1',
    recordId: 'camp-1',
    title: 'Outbound Campaign einrichten',
    instruction: 'Nutze den Outbound Skill.',
    requiredSkills: ['business-os-outbound-campaign-setup'],
    writebackContract: { command_type: 'outbound.campaign.apply_setup' },
    payload: { prompt: 'Bitte richte diese Outbound-Kampagne ein.' },
    clientContext: { outbound_action: 'campaign-setup-briefing' },
    openBusinessChat(value) { opened.push(value); },
  });

  assert.equal(opened.length, 1);
  assert.equal(opened[0], detail);
  assert.equal(detail.action, 'context-chat');
  assert.equal(detail.reuseActive, false);
  assert.equal(detail.command_type, 'business_os.chat.task');
  assert.deepEqual(detail.required_skills, ['business-os-outbound-campaign-setup']);
  assert.equal(detail.writeback_contract.command_type, 'outbound.campaign.apply_setup');
  assert.equal(detail.client_context.action, 'context-chat');
  assert.equal(detail.client_context.outbound_action, 'campaign-setup-briefing');
});

test('campaign editor keeps template briefing drafts across rerenders', () => {
  assert.match(bundledSource, /campaignEditDrafts:\s*(?:\/\* @__PURE__ \*\/\s*)?new Map\(\)/);
  assert.match(bundledSource, /function syncCampaignEditDraftFromEditor/);
  assert.match(bundledSource, /state\.campaignEditDrafts\.get\(campaign\.id\)/);
  assert.match(bundledSource, /data-campaign-idea-template/);
  assert.match(bundledSource, /data-original-briefing=.*escapeHtml\d*\(originalBriefing\)/);
  assert.match(bundledSource, /data-campaign-edit-save/);
  assert.match(bundledSource, /saveButton\.disabled = !name \|\| !dirty/);
  assert.match(bundledSource, /syncCampaignEditDraftFromEditor\(editor\);\s*updateCampaignEditSaveState\(editor\);/);
});

test('campaign editor rerenders templates when shell language changes', () => {
  assert.match(bundledSource, /function applyOutboundLanguage/);
  assert.match(bundledSource, /ctox-business-os-preferences/);
  assert.match(bundledSource, /ctox-business-os-language/);
  assert.match(bundledSource, /syncCampaignEditDraftFromEditor\(editor\);\s*render\(true\);/);
  assert.match(bundledSource, /campaignIdeaTemplates\(lang = state\.lang\)/);
});
