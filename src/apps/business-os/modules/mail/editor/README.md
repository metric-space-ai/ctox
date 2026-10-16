# Mail content editor adapter

This directory is the boundary between the Mail module and its two content
editors. It contains plain browser ESM and has no package-manager or build-time
dependency.

## Public API

```js
import { createMailContentEditor } from './editor/mail-content-editor.mjs?v=1';

const editor = await createMailContentEditor({
  ctx,                         // shell-provided Business OS context
  host,                        // Element owned by the Mail composer
  mode: 'rich-text',           // or `html`
  documentArtifact: {          // optional Documents-owned Word draft
    documentId: 'doc_123',
    versionId: 'doc_123_v4',
    title: 'Follow-up template',
  },
  htmlDocument: {},            // Easy Email design JSON
  mergeTags: {},
  onCreateWordArtifact: async () => ({ documentId, versionId, title }),
  onEvent: ({ name, detail }) => {},
});

await editor.setMode('html');
const value = await editor.serialize();
await editor.destroy();
```

`serialize()` returns one of these envelopes:

```js
{ mode: 'rich-text', format: 'documents-artifact', documentArtifact }
{ mode: 'html', format: 'html', htmlDocument, html }
```

The handle also provides `setDocumentArtifact`, `setHtmlDocument`, `openWord`,
`openHtmlPanel`, `closeHtmlPanel`, `focus`, `setReadOnly`, `markClean`, `on`,
and the `mode`, `dirty`, and `documentArtifact` getters.

## Word ownership

Mail does not embed or fork the Word persistence stack. A rich-text body is a
reference to a record and version owned by the Documents module. `openWord()`
uses `ctx.openDesktopApp('documents', { args })`; Documents then owns the DOCX
bytes, version lifecycle, permissions, Office bridge, and artifact export.
The Mail group should persist only the normalized `documentArtifact` reference.

The reference includes a `deepLink` for navigation/audit context. The launcher
passes both the current Documents aliases (`record`, `version`) and the shell
record-focus aliases (`record_id`, `version_id`).

## Local Easy Email runtime contract

The adapter either receives `easyEmailRuntime`, `loadEasyEmailRuntime`, or
loads `../vendor/easy-email-editor/index.mjs` (App-Vendor `modules/mail/vendor/`). That local module must export:

```js
export async function createEasyEmailEditor({
  host, document, locale, theme, readOnly, mergeTags, onChange,
  panelHosts, requestPanel, logicBridge,
}) {
  return {
    ownsPanels: true,
    getDocument: async () => designJson,
    getHtml: async () => renderedHtml,
    setDocument: async (designJson) => {},
    setReadOnly: async (readOnly) => {}, // optional
    setActivePanel: async (nameOrNull) => {},
    getSelectedBlockId: () => 'content.children.0',
    onSelectionChange: (listener) => () => {},
    setMergeTags: async (liveTestData) => {},
    setLogicPreview: async ({ blockId, matched, testData, logic }) => {},
    focus: () => {},
    destroy: async () => {},
  };
}
```

`host` is always the large central canvas. The real Easy Email port owns its
Layout, Properties, and source-code panels and exposes them through its one
modal right drawer. The CTOX adapter owns the separate Logic panel in the same
right-drawer interaction model. No panel is a permanently visible sidebar.
At compact widths the drawer fills the editor surface and retains an explicit
close path.

Easy Email's `useFocusIdx()` emits a stable path such as
`content.children.0.children.1`; blocks are not required to have IDs. The
selection bridge therefore returns that path and emits it again after the
iframe/runtime is ready. `setMergeTags()` updates substitutions for live test
data. `setLogicPreview()` applies the current matching result to the canvas
without mutating the persisted template.

## Versioned block logic

Logic is persisted in the selected Easy Email block, inside the versioned
design source document:

```js
block.data.value.logic = {
  version: 1,
  root: {
    id: 'logic-group-…',
    kind: 'group',
    combinator: 'and', // or `or`
    children: [
      {
        id: 'logic-rule-…',
        kind: 'rule',
        field: 'contact.segment',
        operator: 'equals',
        valueType: 'string',
        value: 'kunde',
      },
    ],
  },
  testData: { contact: { segment: 'kunde' } },
};
```

Groups may be nested. Supported operators are `equals`, `not-equals`,
`contains`, `not-contains`, `exists`, `empty`, `greater`, and `less`; values
are typed as `string`, `number`, `boolean`, or `null`. The Logic drawer can
create, edit, delete, and reorder every node and shows both per-node results
and the live block visibility result. `logic-editor-v1.mjs` owns normalization,
immutable tree editing, evaluation, path-safe source persistence, and the DOM
editor.

The Easy Email port must remain a local static browser asset. It must not add
an HTTP data API, app-owned persistence, auth, or sync path. Mail owns saving
the serialized envelope through its shell-provided collection handles.
