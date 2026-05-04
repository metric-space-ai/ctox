"use client";

/**
 * Lexical integration adapted from Meta's lexical-playground ToolbarPlugin,
 * ContentEditable, theme, and draggable block plugin patterns (MIT).
 */
import { CodeNode } from "@lexical/code";
import { $generateHtmlFromNodes, $generateNodesFromDOM } from "@lexical/html";
import { AutoLinkNode, LinkNode, TOGGLE_LINK_COMMAND } from "@lexical/link";
import {
  $isListNode,
  ListItemNode,
  ListNode
} from "@lexical/list";
import { LexicalComposer } from "@lexical/react/LexicalComposer";
import { ContentEditable } from "@lexical/react/LexicalContentEditable";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { DraggableBlockPlugin_EXPERIMENTAL } from "@lexical/react/LexicalDraggableBlockPlugin";
import { LexicalErrorBoundary } from "@lexical/react/LexicalErrorBoundary";
import { HistoryPlugin } from "@lexical/react/LexicalHistoryPlugin";
import { LinkPlugin } from "@lexical/react/LexicalLinkPlugin";
import { ListPlugin } from "@lexical/react/LexicalListPlugin";
import { OnChangePlugin } from "@lexical/react/LexicalOnChangePlugin";
import { RichTextPlugin } from "@lexical/react/LexicalRichTextPlugin";
import {
  $isHeadingNode,
  $isQuoteNode,
  HeadingNode,
  QuoteNode
} from "@lexical/rich-text";
import { $patchStyleText } from "@lexical/selection";
import { mergeRegister } from "@lexical/utils";
import type { EditorState, EditorThemeClasses, ElementFormatType, LexicalEditor } from "lexical";
import {
  $createParagraphNode,
  $createTextNode,
  $getRoot,
  $getSelection,
  $insertNodes,
  $isElementNode,
  $isRangeSelection,
  CAN_REDO_COMMAND,
  CAN_UNDO_COMMAND,
  COMMAND_PRIORITY_LOW,
  FORMAT_ELEMENT_COMMAND,
  FORMAT_TEXT_COMMAND,
  REDO_COMMAND,
  SELECTION_CHANGE_COMMAND,
  UNDO_COMMAND
} from "lexical";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import {
  formatBulletList,
  formatCode,
  formatHeading,
  formatNumberedList,
  formatParagraph,
  formatQuote
} from "../lexical-playground-transplant/toolbar-utils";
import type {
  CtoxKnowledgeSkill,
  CtoxRunbook,
  CtoxRunbookItem,
  CtoxSkillFile,
  CtoxSkillbook
} from "../../lib/ctox-knowledge-store";

type Locale = "en" | "de";
type ToolbarBlockType = "paragraph" | "h1" | "h2" | "h3" | "quote" | "code" | "bullet" | "number";

const editorTheme: EditorThemeClasses = {
  heading: {
    h1: "ctox-lexical-h1",
    h2: "ctox-lexical-h2",
    h3: "ctox-lexical-h3"
  },
  link: "ctox-lexical-link",
  list: {
    listitem: "ctox-lexical-list-item",
    nested: { listitem: "ctox-lexical-nested-list-item" },
    olDepth: [
      "ctox-lexical-ol1",
      "ctox-lexical-ol2",
      "ctox-lexical-ol3",
      "ctox-lexical-ol4",
      "ctox-lexical-ol5"
    ],
    ul: "ctox-lexical-ul"
  },
  paragraph: "ctox-lexical-paragraph",
  quote: "ctox-lexical-quote",
  text: {
    bold: "ctox-lexical-bold",
    code: "ctox-lexical-inline-code",
    italic: "ctox-lexical-italic",
    strikethrough: "ctox-lexical-strikethrough",
    underline: "ctox-lexical-underline"
  }
};

export function RunbookDocumentViewer({
  files,
  groupId,
  locale,
  runbook,
  runbookItems,
  selectedFile,
  selectedSkill,
  skillbook
}: {
  files: CtoxSkillFile[];
  groupId?: string;
  locale: Locale;
  runbook?: CtoxRunbook;
  runbookItems: CtoxRunbookItem[];
  selectedFile?: CtoxSkillFile;
  selectedSkill: CtoxKnowledgeSkill;
  skillbook?: CtoxSkillbook;
}) {
  const labels = locale === "de" ? deLabels : enLabels;
  const [editing, setEditing] = useState(false);
  const [message, setMessage] = useState("");
  const initialHtml = useMemo(
    () => runbook
      ? buildRunbookHtml({ labels, runbook, runbookItems, skillbook })
      : buildSkillAssetHtml({ labels, selectedFile, selectedSkill }),
    [labels, runbook, runbookItems, selectedFile, selectedSkill, skillbook]
  );
  const documentKey = runbook?.id ?? `${selectedSkill.id}:${selectedFile?.relativePath ?? "skill"}`;

  useEffect(() => {
    setEditing(false);
    setMessage("");
  }, [documentKey]);

  return (
    <section
      className="runbook-document"
      data-context-item
      data-context-file-path={selectedFile?.relativePath}
      data-context-group={groupId}
      data-context-label={runbook?.title ?? selectedFile?.relativePath ?? selectedSkill.title}
      data-context-module="operations"
      data-context-record-id={documentKey}
      data-context-record-type={runbook ? "ctox_runbook" : "ctox_skill_file"}
      data-context-skill-id={selectedSkill.id}
      data-context-source-path={selectedSkill.sourcePath}
      data-context-submodule="knowledge"
    >
      <header className="runbook-document-head">
        <div>
          <span>{runbook ? labels.runbook : labels.skillFile}</span>
          <h2>{runbook?.title ?? selectedSkill.title}</h2>
        </div>
        <button
          aria-label={editing ? labels.closeEditor : labels.editRunbook}
          aria-pressed={editing}
          className={editing ? "active" : ""}
          onClick={() => setEditing((value) => !value)}
          title={editing ? labels.closeEditor : labels.editRunbook}
          type="button"
        >
          ✎
        </button>
      </header>

      <LexicalComposer
        initialConfig={{
          editable: editing,
          namespace: `ctox-runbook-${documentKey}`,
          nodes: [HeadingNode, QuoteNode, ListNode, ListItemNode, LinkNode, AutoLinkNode, CodeNode],
          onError(error) {
            throw error;
          },
          theme: editorTheme
        }}
      >
        <div className="runbook-document-body" data-editing={editing ? "true" : "false"}>
          <div className="runbook-page">
            <RichTextPlugin
              contentEditable={
                <ContentEditable
                  aria-placeholder={labels.editorPlaceholder}
                  className="ctox-lexical-editor"
                  placeholder={<p className="ctox-lexical-placeholder">{labels.editorPlaceholder}</p>}
                />
              }
              ErrorBoundary={LexicalErrorBoundary}
            />
          </div>
          <HistoryPlugin />
          <ListPlugin />
          <LinkPlugin />
          <InitialHtmlPlugin documentKey={documentKey} html={initialHtml} />
          <EditablePlugin editable={editing} />
          <OnChangePlugin ignoreHistoryMergeTagChange onChange={() => setMessage("")} />
          <CtoxDraggableBlocksPlugin enabled={editing} />
          <VerticalLexicalToolbar
            labels={labels}
            onSave={(html) => queueRunbookEdit({
              html,
              labels,
              runbook,
              selectedSkill,
              setMessage
            })}
            visible={editing}
          />
        </div>
      </LexicalComposer>
      {message ? <p className="runbook-document-status">{message}</p> : null}
    </section>
  );
}

function InitialHtmlPlugin({ documentKey, html }: { documentKey: string; html: string }) {
  const [editor] = useLexicalComposerContext();
  const loadedKeyRef = useRef("");

  useEffect(() => {
    if (loadedKeyRef.current === documentKey) return;
    loadedKeyRef.current = documentKey;
    editor.update(() => {
      const root = $getRoot();
      root.clear();
      const parser = new DOMParser();
      const dom = parser.parseFromString(html, "text/html");
      const nodes = $generateNodesFromDOM(editor, dom);
      if (nodes.length > 0) {
        root.append(...nodes);
      } else {
        root.append($createParagraphNode());
      }
      root.selectStart();
    });
  }, [documentKey, editor, html]);

  return null;
}

function EditablePlugin({ editable }: { editable: boolean }) {
  const [editor] = useLexicalComposerContext();

  useEffect(() => {
    editor.setEditable(editable);
  }, [editable, editor]);

  return null;
}

function CtoxDraggableBlocksPlugin({ enabled }: { enabled: boolean }) {
  const menuRef = useRef<HTMLDivElement | null>(null);
  const targetLineRef = useRef<HTMLDivElement | null>(null);
  const [anchorElem, setAnchorElem] = useState<HTMLElement | null>(null);

  useEffect(() => {
    setAnchorElem(document.querySelector(".runbook-page") as HTMLElement | null);
  }, []);

  if (!enabled || !anchorElem) return null;

  return (
    <DraggableBlockPlugin_EXPERIMENTAL
      anchorElem={anchorElem}
      isOnMenu={(element) => Boolean(element.closest(".ctox-block-drag-menu"))}
      menuComponent={<div className="ctox-block-drag-menu" ref={menuRef} title="Move section">⋮⋮</div>}
      menuRef={menuRef}
      targetLineComponent={<div className="ctox-block-target-line" ref={targetLineRef} />}
      targetLineRef={targetLineRef}
    />
  );
}

function VerticalLexicalToolbar({
  labels,
  onSave,
  visible
}: {
  labels: typeof enLabels;
  onSave: (html: string) => void;
  visible: boolean;
}) {
  const [editor] = useLexicalComposerContext();
  const [openPanel, setOpenPanel] = useState<"block" | "font" | "size" | "align" | null>(null);
  const [state, setState] = useState({
    blockType: "paragraph" as ToolbarBlockType,
    canRedo: false,
    canUndo: false,
    elementFormat: "left" as ElementFormatType,
    isBold: false,
    isItalic: false,
    isUnderline: false
  });

  const refreshToolbar = useCallback(() => {
    const selection = $getSelection();
    if (!$isRangeSelection(selection)) return;
    const anchorNode = selection.anchor.getNode();
    const topLevelElement = anchorNode.getKey() === "root"
      ? anchorNode
      : anchorNode.getTopLevelElementOrThrow();
    let blockType: ToolbarBlockType = "paragraph";
    if ($isHeadingNode(topLevelElement)) {
      blockType = topLevelElement.getTag() as ToolbarBlockType;
    } else if ($isQuoteNode(topLevelElement)) {
      blockType = "quote";
    } else if (topLevelElement.getType() === "code") {
      blockType = "code";
    } else if ($isListNode(topLevelElement)) {
      blockType = topLevelElement.getListType() as ToolbarBlockType;
    }
    const elementFormat = $isElementNode(topLevelElement) ? topLevelElement.getFormatType() : "left";
    setState((current) => ({
      ...current,
      blockType,
      elementFormat,
      isBold: selection.hasFormat("bold"),
      isItalic: selection.hasFormat("italic"),
      isUnderline: selection.hasFormat("underline")
    }));
  }, []);

  useEffect(() => mergeRegister(
    editor.registerUpdateListener(({ editorState }: { editorState: EditorState }) => {
      editorState.read(refreshToolbar);
    }),
    editor.registerCommand(
      SELECTION_CHANGE_COMMAND,
      () => {
        refreshToolbar();
        return false;
      },
      COMMAND_PRIORITY_LOW
    ),
    editor.registerCommand(
      CAN_UNDO_COMMAND,
      (payload) => {
        setState((current) => ({ ...current, canUndo: payload }));
        return false;
      },
      COMMAND_PRIORITY_LOW
    ),
    editor.registerCommand(
      CAN_REDO_COMMAND,
      (payload) => {
        setState((current) => ({ ...current, canRedo: payload }));
        return false;
      },
      COMMAND_PRIORITY_LOW
    )
  ), [editor, refreshToolbar]);

  const togglePanel = (panel: "block" | "font" | "size" | "align") => {
    setOpenPanel((current) => current === panel ? null : panel);
  };
  const closePanel = () => setOpenPanel(null);

  return (
    <div className="runbook-editor-rail" data-visible={visible ? "true" : "false"} aria-hidden={!visible}>
      <div className="runbook-editor-stack">
        <EditorToolButton disabled={!state.canUndo} label={labels.undo} onClick={() => editor.dispatchCommand(UNDO_COMMAND, undefined)}>↶</EditorToolButton>
        <EditorToolButton disabled={!state.canRedo} label={labels.redo} onClick={() => editor.dispatchCommand(REDO_COMMAND, undefined)}>↷</EditorToolButton>
      </div>
      <div className="runbook-editor-stack">
        <EditorToolButton active={openPanel === "block"} label={labels.blockStyle} onClick={() => togglePanel("block")}>{blockTypeIcon(state.blockType)}</EditorToolButton>
        <EditorToolButton active={openPanel === "font"} label={labels.fontFamily} onClick={() => togglePanel("font")}>T</EditorToolButton>
        <EditorToolButton active={openPanel === "size"} label={labels.fontSize} onClick={() => togglePanel("size")}>15</EditorToolButton>
      </div>
      <div className="runbook-editor-stack">
        <EditorToolButton active={state.isBold} label={labels.bold} onClick={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, "bold")}>B</EditorToolButton>
        <EditorToolButton active={state.isItalic} label={labels.italic} onClick={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, "italic")}>I</EditorToolButton>
        <EditorToolButton active={state.isUnderline} label={labels.underline} onClick={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, "underline")}>U</EditorToolButton>
        <EditorToolButton label={labels.code} onClick={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, "code")}>&lt;/&gt;</EditorToolButton>
        <EditorToolButton label={labels.link} onClick={() => applyLexicalLink(editor, labels)}>⌁</EditorToolButton>
      </div>
      <div className="runbook-editor-stack">
        <EditorToolButton label={labels.textColor} onClick={() => patchSelectionStyle(editor, "color", "#5e7a31")}>A</EditorToolButton>
        <EditorToolButton label={labels.highlight} onClick={() => patchSelectionStyle(editor, "background-color", "#fff2a8")}>▰</EditorToolButton>
        <EditorToolButton label={labels.insert} onClick={() => insertLexicalBlock(editor)}>＋</EditorToolButton>
        <EditorToolButton active={openPanel === "align"} label={labels.alignLeft} onClick={() => togglePanel("align")}>{alignmentIcon(state.elementFormat)}</EditorToolButton>
      </div>
      <div className="runbook-editor-stack">
        <EditorToolButton label={labels.save} onClick={() => onSave(exportLexicalHtml(editor))}>✓</EditorToolButton>
      </div>
      {openPanel === "block" ? (
        <EditorPanel label={labels.blockStyle}>
          {blockOptions.map((option) => (
            <PanelButton
              active={state.blockType === option.value}
              key={option.value}
              label={option.label}
              onClick={() => {
                formatBlock(editor, option.value);
                closePanel();
              }}
            />
          ))}
        </EditorPanel>
      ) : null}
      {openPanel === "font" ? (
        <EditorPanel label={labels.fontFamily}>
          {fontOptions.map((option) => (
            <PanelButton
              key={option.value}
              label={option.label}
              onClick={() => {
                patchSelectionStyle(editor, "font-family", option.value);
                closePanel();
              }}
            />
          ))}
        </EditorPanel>
      ) : null}
      {openPanel === "size" ? (
        <EditorPanel label={labels.fontSize}>
          {fontSizeOptions.map((option) => (
            <PanelButton
              key={option.value}
              label={option.label}
              onClick={() => {
                patchSelectionStyle(editor, "font-size", option.value);
                closePanel();
              }}
            />
          ))}
        </EditorPanel>
      ) : null}
      {openPanel === "align" ? (
        <EditorPanel label={labels.alignLeft}>
          {alignmentOptions.map((option) => (
            <PanelButton
              active={state.elementFormat === option.value}
              key={option.value}
              label={option.label}
              onClick={() => {
                editor.dispatchCommand(FORMAT_ELEMENT_COMMAND, option.value);
                closePanel();
              }}
            />
          ))}
        </EditorPanel>
      ) : null}
    </div>
  );
}

function EditorToolButton({
  active,
  children,
  disabled,
  label,
  onClick
}: {
  active?: boolean;
  children: ReactNode;
  disabled?: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      aria-label={label}
      className={active ? "active" : ""}
      disabled={disabled}
      onClick={onClick}
      title={label}
      type="button"
    >
      {children}
    </button>
  );
}

function EditorPanel({ children, label }: { children: ReactNode; label: string }) {
  return (
    <div className="runbook-editor-panel" role="menu">
      <span>{label}</span>
      {children}
    </div>
  );
}

function PanelButton({
  active,
  label,
  onClick
}: {
  active?: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button className={active ? "active" : ""} onClick={onClick} role="menuitem" type="button">
      {label}
    </button>
  );
}

const blockOptions: Array<{ label: string; value: ToolbarBlockType }> = [
  { label: "Normal", value: "paragraph" },
  { label: "Heading 1", value: "h1" },
  { label: "Heading 2", value: "h2" },
  { label: "Heading 3", value: "h3" },
  { label: "Quote", value: "quote" },
  { label: "Bulleted list", value: "bullet" },
  { label: "Numbered list", value: "number" },
  { label: "Code block", value: "code" }
];

const fontOptions = [
  { label: "Arial", value: "Arial" },
  { label: "Inter", value: "Inter" },
  { label: "Georgia", value: "Georgia" },
  { label: "Mono", value: "Courier New" }
];

const fontSizeOptions = [
  { label: "13 px", value: "13px" },
  { label: "15 px", value: "15px" },
  { label: "18 px", value: "18px" },
  { label: "24 px", value: "24px" }
];

const alignmentOptions: Array<{ label: string; value: ElementFormatType }> = [
  { label: "Left", value: "left" },
  { label: "Center", value: "center" },
  { label: "Right", value: "right" },
  { label: "Justify", value: "justify" }
];

function blockTypeIcon(blockType: ToolbarBlockType) {
  if (blockType === "h1") return "H1";
  if (blockType === "h2") return "H2";
  if (blockType === "h3") return "H3";
  if (blockType === "quote") return "Q";
  if (blockType === "bullet") return "•";
  if (blockType === "number") return "1.";
  if (blockType === "code") return "</>";
  return "P";
}

function alignmentIcon(format: ElementFormatType) {
  if (format === "center") return "≣";
  if (format === "right") return "☰";
  if (format === "justify") return "☷";
  return "≡";
}

function formatBlock(editor: LexicalEditor, blockType: ToolbarBlockType) {
  if (blockType === "bullet") {
    formatBulletList(editor);
    return;
  }
  if (blockType === "number") {
    formatNumberedList(editor);
    return;
  }
  if (blockType === "h1" || blockType === "h2" || blockType === "h3") formatHeading(editor, blockType);
  else if (blockType === "quote") formatQuote(editor);
  else if (blockType === "code") formatCode(editor);
  else formatParagraph(editor);
}

function patchSelectionStyle(editor: LexicalEditor, property: string, value: string) {
  editor.update(() => {
    const selection = $getSelection();
    if ($isRangeSelection(selection)) {
      $patchStyleText(selection, { [property]: value });
    }
  });
}

function applyLexicalLink(editor: LexicalEditor, labels: typeof enLabels) {
  const href = window.prompt(labels.linkPrompt);
  if (href) {
    editor.dispatchCommand(TOGGLE_LINK_COMMAND, href);
  }
}

function insertLexicalBlock(editor: LexicalEditor) {
  editor.update(() => {
    const paragraph = $createParagraphNode();
    paragraph.append($createTextNode("New section"));
    $insertNodes([paragraph]);
  });
}

function exportLexicalHtml(editor: LexicalEditor) {
  let html = "";
  editor.getEditorState().read(() => {
    html = $generateHtmlFromNodes(editor, null);
  });
  return html;
}

async function queueRunbookEdit({
  html,
  labels,
  runbook,
  selectedSkill,
  setMessage
}: {
  html: string;
  labels: typeof enLabels;
  runbook?: CtoxRunbook;
  selectedSkill: CtoxKnowledgeSkill;
  setMessage: (message: string) => void;
}) {
  const recordId = runbook?.id ?? selectedSkill.id;
  const response = await fetch("/api/ctox/queue-tasks", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      instruction: `Review and persist the Knowledge editor changes for ${runbook ? "runbook" : "skill asset"} ${recordId}. Preserve CTOX skill hierarchy, section order, inline Lexical formatting, and create the required SQLite update or file patch plan.`,
      context: {
        source: "operations/knowledge-lexical-editor",
        items: [{
          moduleId: "operations",
          submoduleId: "knowledge",
          recordType: runbook ? "ctox_runbook" : "ctox_skill_asset",
          recordId,
          label: runbook?.title ?? selectedSkill.title
        }],
        html
      }
    })
  });
  const payload = await response.json().catch(() => null) as { queued?: boolean; core?: { taskId?: string | null } } | null;
  setMessage(payload?.queued ? `${labels.queued}${payload.core?.taskId ? ` · ${payload.core.taskId}` : ""}` : labels.queueFailed);
}

function buildRunbookHtml({
  labels,
  runbook,
  runbookItems,
  skillbook
}: {
  labels: typeof enLabels;
  runbook: CtoxRunbook;
  runbookItems: CtoxRunbookItem[];
  skillbook?: CtoxSkillbook;
}) {
  const itemHtml = runbookItems.map((item) => {
    const triggers = arrayField(item.structured.trigger_phrases);
    const verification = arrayField(item.structured.verification);
    const escalation = arrayField(item.structured.escalate_when);
    const guidance = stringField(item.structured.expected_guidance) || item.chunkText;
    const blocker = stringField(item.structured.earliest_blocker);
    return `
      <h2>${escapeHtml(item.label)} · ${escapeHtml(item.title)}</h2>
      <p><strong>${labels.problemClass}</strong> ${escapeHtml(item.problemClass)}</p>
      ${triggers.length ? `<p><strong>${labels.triggers}</strong> ${escapeHtml(triggers.join(" · "))}</p>` : ""}
      ${blocker ? `<p><strong>${labels.blocker}</strong> ${escapeHtml(blocker)}</p>` : ""}
      <p><strong>${labels.guidance}</strong> ${escapeHtml(guidance)}</p>
      ${verification.length ? `<h3>${labels.verification}</h3><ul>${verification.map((value) => `<li>${escapeHtml(value)}</li>`).join("")}</ul>` : ""}
      ${escalation.length ? `<h3>${labels.escalation}</h3><ul>${escalation.map((value) => `<li>${escapeHtml(value)}</li>`).join("")}</ul>` : ""}
    `;
  }).join("");

  return `
    <h1>${escapeHtml(runbook.title)}</h1>
    <p>${escapeHtml(runbook.summary || runbook.problemDomain)}</p>
    <p><strong>${labels.skillbook}</strong> ${escapeHtml(skillbook?.title ?? runbook.skillbookId)}</p>
    <p><strong>${labels.status}</strong> ${escapeHtml(runbook.status)} · v${escapeHtml(runbook.version)}</p>
    <p><strong>${labels.domain}</strong> ${escapeHtml(runbook.problemDomain)}</p>
    ${itemHtml || `<p>${labels.noRunbookItems}</p>`}
  `;
}

function buildSkillAssetHtml({
  labels,
  selectedFile,
  selectedSkill
}: {
  labels: typeof enLabels;
  selectedFile?: CtoxSkillFile;
  selectedSkill: CtoxKnowledgeSkill;
}) {
  const content = selectedFile?.contentText?.trim();
  return `
    <p><strong>${labels.skillFile}</strong> ${escapeHtml(selectedFile?.relativePath ?? labels.noSourcePath)}</p>
    <p><strong>${labels.skill}</strong> ${escapeHtml(selectedSkill.title)}</p>
    <p><strong>${labels.source}</strong> ${escapeHtml(selectedSkill.sourcePath ?? labels.noSourcePath)}</p>
    ${content ? markdownToHtml(content) : `<p>${labels.noFileContent}</p>`}
  `;
}

function markdownToHtml(markdown: string) {
  const lines = markdown.replace(/\r\n/g, "\n").split("\n");
  const html: string[] = [];
  let paragraph: string[] = [];
  let listType: "ul" | "ol" | null = null;
  let codeBlock: string[] | null = null;
  let inFrontMatter = false;

  const flushParagraph = () => {
    if (paragraph.length === 0) return;
    html.push(`<p>${inlineMarkdown(paragraph.join(" "))}</p>`);
    paragraph = [];
  };
  const closeList = () => {
    if (!listType) return;
    html.push(`</${listType}>`);
    listType = null;
  };

  lines.forEach((line, index) => {
    if (index === 0 && line.trim() === "---") {
      inFrontMatter = true;
      html.push("<h2>Metadata</h2><dl>");
      return;
    }
    if (inFrontMatter) {
      if (line.trim() === "---") {
        inFrontMatter = false;
        html.push("</dl>");
        return;
      }
      const [key, ...rest] = line.split(":");
      if (key && rest.length > 0) html.push(`<dt>${escapeHtml(key.trim())}</dt><dd>${escapeHtml(rest.join(":").trim())}</dd>`);
      return;
    }
    if (line.startsWith("```")) {
      flushParagraph();
      closeList();
      if (codeBlock) {
        html.push(`<pre><code>${escapeHtml(codeBlock.join("\n"))}</code></pre>`);
        codeBlock = null;
      } else {
        codeBlock = [];
      }
      return;
    }
    if (codeBlock) {
      codeBlock.push(line);
      return;
    }

    const trimmed = line.trim();
    if (!trimmed) {
      flushParagraph();
      closeList();
      return;
    }

    const heading = /^(#{1,4})\s+(.+)$/.exec(trimmed);
    if (heading) {
      flushParagraph();
      closeList();
      const level = Math.min(heading[1].length + 1, 3);
      html.push(`<h${level}>${inlineMarkdown(heading[2])}</h${level}>`);
      return;
    }

    const bullet = /^[-*]\s+(.+)$/.exec(trimmed);
    if (bullet) {
      flushParagraph();
      if (listType !== "ul") {
        closeList();
        listType = "ul";
        html.push("<ul>");
      }
      html.push(`<li>${inlineMarkdown(bullet[1])}</li>`);
      return;
    }

    const numbered = /^\d+\.\s+(.+)$/.exec(trimmed);
    if (numbered) {
      flushParagraph();
      if (listType !== "ol") {
        closeList();
        listType = "ol";
        html.push("<ol>");
      }
      html.push(`<li>${inlineMarkdown(numbered[1])}</li>`);
      return;
    }

    closeList();
    paragraph.push(trimmed);
  });

  flushParagraph();
  closeList();
  const openCodeBlock = codeBlock as string[] | null;
  if (openCodeBlock !== null) html.push(`<pre><code>${escapeHtml(openCodeBlock.join("\n"))}</code></pre>`);
  return html.join("");
}

function inlineMarkdown(value: string) {
  return escapeHtml(value)
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*]+)\*/g, "<em>$1</em>");
}

function arrayField(value: unknown) {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}

function stringField(value: unknown) {
  return typeof value === "string" ? value : "";
}

function escapeHtml(value: string) {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

const enLabels = {
  alignLeft: "Text alignment",
  assets: "Assets",
  blockStyle: "Block style",
  blocker: "Blocker",
  bold: "Bold",
  closeEditor: "Close editor",
  code: "Code",
  skillAssets: "Skill assets",
  domain: "Domain",
  editRunbook: "Edit runbook",
  editorPlaceholder: "Write the runbook...",
  escalation: "Escalation",
  fontFamily: "Font family",
  fontSize: "Font size",
  guidance: "Guidance",
  highlight: "Highlight",
  insert: "Insert block",
  italic: "Italic",
  link: "Link",
  linkPrompt: "Paste link URL",
  noAssets: "No files are materialized for this skill yet.",
  noFileContent: "No content is materialized for this file yet.",
  noRunbookItems: "No runbook items are linked yet.",
  noSourcePath: "No source path recorded.",
  problemClass: "Problem class",
  queued: "Queued",
  queueFailed: "Queue failed",
  redo: "Redo",
  runbook: "Runbook",
  save: "Save through CTOX",
  skill: "Skill",
  skillbook: "Skillbook",
  skillFile: "Skill file",
  skillFiles: "Skill files",
  source: "Source",
  status: "Status",
  textColor: "Text color",
  triggers: "Triggers",
  underline: "Underline",
  undo: "Undo",
  verification: "Verification"
};

const deLabels = {
  alignLeft: "Textausrichtung",
  assets: "Assets",
  blockStyle: "Blockstil",
  blocker: "Blocker",
  bold: "Fett",
  closeEditor: "Editor schließen",
  code: "Code",
  skillAssets: "Custom Skill Assets",
  domain: "Domain",
  editRunbook: "Runbook editieren",
  editorPlaceholder: "Runbook schreiben...",
  escalation: "Eskalation",
  fontFamily: "Schriftart",
  fontSize: "Schriftgröße",
  guidance: "Anleitung",
  highlight: "Highlight",
  insert: "Block einfügen",
  italic: "Kursiv",
  link: "Link",
  linkPrompt: "Link-URL einfügen",
  noAssets: "Für diesen Skill sind noch keine Dateien materialisiert.",
  noFileContent: "Für diese Datei ist noch kein Inhalt materialisiert.",
  noRunbookItems: "Es sind noch keine Runbook-Items verknüpft.",
  noSourcePath: "Kein Source-Pfad gespeichert.",
  problemClass: "Problemklasse",
  queued: "Queued",
  queueFailed: "Queue fehlgeschlagen",
  redo: "Wiederholen",
  runbook: "Runbook",
  save: "Über CTOX speichern",
  skill: "Skill",
  skillbook: "Skillbook",
  skillFile: "Skill-Datei",
  skillFiles: "Skill-Dateien",
  source: "Quelle",
  status: "Status",
  textColor: "Textfarbe",
  triggers: "Trigger",
  underline: "Unterstrichen",
  undo: "Rückgängig",
  verification: "Verifikation"
};
