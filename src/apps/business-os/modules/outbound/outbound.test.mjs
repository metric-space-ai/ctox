import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
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

test('outbound import extracts company rows from uploaded Excel workbooks', async () => {
  const buffer = minimalXlsxWorkbook([
    ['Company', 'Website', 'City'],
    ['A GmbH', 'https://a.example', 'Berlin'],
    ['B GmbH', 'https://b.example', 'Hamburg'],
  ]);
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
  assert.deepEqual(rows.map((row) => row.name), ['A GmbH', 'B GmbH']);
});

function minimalXlsxWorkbook(rows) {
  const sheetRows = rows.map((cells, rowIndex) => {
    const rowNumber = rowIndex + 1;
    const xmlCells = cells.map((cell, columnIndex) => {
      const ref = `${columnName(columnIndex)}${rowNumber}`;
      return `<c r="${ref}" t="inlineStr"><is><t>${escapeXml(cell)}</t></is></c>`;
    }).join('');
    return `<row r="${rowNumber}">${xmlCells}</row>`;
  }).join('');
  return zipStoreEntries({
    '[Content_Types].xml': `<?xml version="1.0" encoding="UTF-8"?>
      <Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
        <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
        <Default Extension="xml" ContentType="application/xml"/>
      </Types>`,
    'xl/workbook.xml': `<?xml version="1.0" encoding="UTF-8"?>
      <workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
        xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
        <sheets><sheet name="Outbound" sheetId="1" r:id="rId1"/></sheets>
      </workbook>`,
    'xl/_rels/workbook.xml.rels': `<?xml version="1.0" encoding="UTF-8"?>
      <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
      </Relationships>`,
    'xl/worksheets/sheet1.xml': `<?xml version="1.0" encoding="UTF-8"?>
      <worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
        <sheetData>${sheetRows}</sheetData>
      </worksheet>`,
  });
}

function zipStoreEntries(entries) {
  const localParts = [];
  const centralParts = [];
  let offset = 0;
  for (const [name, content] of Object.entries(entries)) {
    const nameBytes = Buffer.from(name);
    const data = Buffer.from(content);
    const local = Buffer.alloc(30 + nameBytes.length);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(0, 6);
    local.writeUInt16LE(0, 8);
    local.writeUInt32LE(0, 10);
    local.writeUInt32LE(0, 14);
    local.writeUInt32LE(data.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(nameBytes.length, 26);
    nameBytes.copy(local, 30);
    localParts.push(local, data);

    const central = Buffer.alloc(46 + nameBytes.length);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(20, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(0, 8);
    central.writeUInt16LE(0, 10);
    central.writeUInt32LE(0, 12);
    central.writeUInt32LE(0, 16);
    central.writeUInt32LE(data.length, 20);
    central.writeUInt32LE(data.length, 24);
    central.writeUInt16LE(nameBytes.length, 28);
    central.writeUInt32LE(offset, 42);
    nameBytes.copy(central, 46);
    centralParts.push(central);
    offset += local.length + data.length;
  }
  const centralDirectoryOffset = offset;
  const centralDirectory = Buffer.concat(centralParts);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(centralParts.length, 8);
  end.writeUInt16LE(centralParts.length, 10);
  end.writeUInt32LE(centralDirectory.length, 12);
  end.writeUInt32LE(centralDirectoryOffset, 16);
  return Buffer.concat([...localParts, centralDirectory, end]);
}

function columnName(index) {
  let value = index + 1;
  let name = '';
  while (value > 0) {
    const remainder = (value - 1) % 26;
    name = String.fromCharCode(65 + remainder) + name;
    value = Math.floor((value - 1) / 26);
  }
  return name;
}

function escapeXml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

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

test('campaign briefing save spawns a CTOX chat task for the setup skill', () => {
  assert.match(bundledSource, /ctox-business-os-chat-submit/);
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

test('outbound prompt tasks use the same spawned chat event as context menu actions', () => {
  const events = [];
  const previousWindow = globalThis.window;
  globalThis.window = {
    dispatchEvent(event) {
      events.push(event);
      return true;
    },
  };
  try {
    hooks.dispatchOutboundPromptTask({
      text: 'Bitte richte diese Outbound-Kampagne ein.',
      commandId: 'cmd-context-chat-1',
      recordId: 'camp-1',
      title: 'Outbound Campaign einrichten',
      instruction: 'Nutze den Outbound Skill.',
      requiredSkills: ['business-os-outbound-campaign-setup'],
      writebackContract: { command_type: 'outbound.campaign.apply_setup' },
      payload: { prompt: 'Bitte richte diese Outbound-Kampagne ein.' },
      clientContext: { outbound_action: 'campaign-setup-briefing' },
    });
  } finally {
    globalThis.window = previousWindow;
  }

  assert.equal(events.length, 1);
  assert.equal(events[0].type, 'ctox-business-os-chat-submit');
  assert.equal(events[0].detail.action, 'context-chat');
  assert.equal(events[0].detail.reuseActive, false);
  assert.equal(events[0].detail.command_type, 'business_os.chat.task');
  assert.deepEqual(events[0].detail.required_skills, ['business-os-outbound-campaign-setup']);
  assert.equal(events[0].detail.writeback_contract.command_type, 'outbound.campaign.apply_setup');
  assert.equal(events[0].detail.client_context.action, 'context-chat');
  assert.equal(events[0].detail.client_context.outbound_action, 'campaign-setup-briefing');
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
