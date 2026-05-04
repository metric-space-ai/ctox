"use client";

/**
 * Lexical integration adapted from the Operations Knowledge editor.
 */
import { CodeNode } from "@lexical/code";
import { AutoLinkNode, LinkNode, TOGGLE_LINK_COMMAND } from "@lexical/link";
import { $isListNode, ListItemNode, ListNode } from "@lexical/list";
import { LexicalComposer } from "@lexical/react/LexicalComposer";
import { ContentEditable } from "@lexical/react/LexicalContentEditable";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { LexicalErrorBoundary } from "@lexical/react/LexicalErrorBoundary";
import { HistoryPlugin } from "@lexical/react/LexicalHistoryPlugin";
import { LinkPlugin } from "@lexical/react/LexicalLinkPlugin";
import { ListPlugin } from "@lexical/react/LexicalListPlugin";
import { RichTextPlugin } from "@lexical/react/LexicalRichTextPlugin";
import { $isHeadingNode, $isQuoteNode, HeadingNode, QuoteNode } from "@lexical/rich-text";
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
import { useCallback, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import {
  formatBulletList,
  formatCode,
  formatHeading,
  formatNumberedList,
  formatParagraph,
  formatQuote
} from "./lexical-playground-transplant/toolbar-utils";

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

export function LexicalRichTextEditor({
  initialText,
  label,
  locale,
  namespace,
  placeholder,
  templateHref
}: {
  initialText: string;
  label: string;
  locale: Locale;
  namespace: string;
  placeholder?: string;
  templateHref?: string;
}) {
  const labels = locale === "de" ? deLabels : enLabels;
  const editorPlaceholder = placeholder ?? labels.placeholder;
  const initialEditorState = useMemo(() => buildEditorStateJson(initialText), [initialText]);

  return (
    <section className="invoice-lexical-field" aria-label={label}>
      <span>{label}</span>
      {templateHref ? (
        <a aria-label={labels.templates} className="invoice-lexical-template-button" href={templateHref} title={labels.templates}>
          <span className="invoice-template-icon-lines" aria-hidden="true">
            <i />
            <i />
            <i />
          </span>
          <span className="invoice-template-icon-caret" aria-hidden="true" />
        </a>
      ) : null}
      <LexicalComposer
        initialConfig={{
          editable: true,
          editorState: initialEditorState,
          namespace,
          nodes: [HeadingNode, QuoteNode, ListNode, ListItemNode, LinkNode, AutoLinkNode, CodeNode],
          onError(error) {
            throw error;
          },
          theme: editorTheme
        }}
      >
        <div className="invoice-lexical-body">
          <div className="invoice-lexical-page">
            <RichTextPlugin
              contentEditable={
                <ContentEditable
                  aria-placeholder={editorPlaceholder}
                  className="ctox-lexical-editor"
                  placeholder={<p className="ctox-lexical-placeholder">{editorPlaceholder}</p>}
                />
              }
              ErrorBoundary={LexicalErrorBoundary}
            />
          </div>
          <HistoryPlugin />
          <ListPlugin />
          <LinkPlugin />
          <InitialEditorStatePlugin editorState={initialEditorState} namespace={namespace} />
          <InitialTextPlugin namespace={namespace} text={initialText} />
          <VerticalLexicalToolbar labels={labels} />
        </div>
      </LexicalComposer>
    </section>
  );
}

function InitialEditorStatePlugin({ editorState, namespace }: { editorState: string; namespace: string }) {
  const [editor] = useLexicalComposerContext();

  useEffect(() => {
    const nextState = editor.parseEditorState(editorState);
    editor.setEditorState(nextState);
  }, [editor, editorState, namespace]);

  return null;
}

function InitialTextPlugin({ namespace, text }: { namespace: string; text: string }) {
  const [editor] = useLexicalComposerContext();

  useEffect(() => {
    if (!text.trim()) return;
    const timer = window.setTimeout(() => {
      editor.update(() => {
        const root = $getRoot();
        if (root.getTextContent().trim()) return;
        root.clear();
        appendTextParagraphs(text);
        root.selectStart();
      });
    }, 0);
    return () => window.clearTimeout(timer);
  }, [editor, namespace, text]);

  return null;
}

function buildEditorStateJson(value: string) {
  const paragraphs = value.split(/\n{2,}/).map((paragraph) => paragraph.trim()).filter(Boolean);
  const lines = paragraphs.length > 0 ? paragraphs : value.trim() ? [value.trim()] : [];
  return JSON.stringify({
    root: {
      children: lines.length > 0 ? lines.map((line) => ({
        children: [{
          detail: 0,
          format: 0,
          mode: "normal",
          style: "",
          text: line,
          type: "text",
          version: 1
        }],
        direction: null,
        format: "",
        indent: 0,
        textFormat: 0,
        textStyle: "",
        type: "paragraph",
        version: 1
      })) : [{
        children: [],
        direction: null,
        format: "",
        indent: 0,
        textFormat: 0,
        textStyle: "",
        type: "paragraph",
        version: 1
      }],
      direction: null,
      format: "",
      indent: 0,
      type: "root",
      version: 1
    }
  });
}

function appendTextParagraphs(value: string) {
  const paragraphs = value.split(/\n{2,}/).map((paragraph) => paragraph.trim()).filter(Boolean);
  const lines = paragraphs.length > 0 ? paragraphs : value.trim() ? [value.trim()] : [];
  lines.forEach((line) => {
    const paragraph = $createParagraphNode();
    paragraph.append($createTextNode(line));
    $getRoot().append(paragraph);
  });
}

function VerticalLexicalToolbar({ labels }: { labels: typeof enLabels }) {
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
    const topLevelElement = anchorNode.getKey() === "root" ? anchorNode : anchorNode.getTopLevelElementOrThrow();
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
    editor.registerCommand(SELECTION_CHANGE_COMMAND, () => {
      refreshToolbar();
      return false;
    }, COMMAND_PRIORITY_LOW),
    editor.registerCommand(CAN_UNDO_COMMAND, (payload) => {
      setState((current) => ({ ...current, canUndo: payload }));
      return false;
    }, COMMAND_PRIORITY_LOW),
    editor.registerCommand(CAN_REDO_COMMAND, (payload) => {
      setState((current) => ({ ...current, canRedo: payload }));
      return false;
    }, COMMAND_PRIORITY_LOW)
  ), [editor, refreshToolbar]);

  const togglePanel = (panel: "block" | "font" | "size" | "align") => {
    setOpenPanel((current) => current === panel ? null : panel);
  };
  const closePanel = () => setOpenPanel(null);

  return (
    <div className="runbook-editor-rail invoice-lexical-rail">
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
    <button aria-label={label} className={active ? "active" : ""} disabled={disabled} onClick={onClick} title={label} type="button">
      {children}
    </button>
  );
}

function EditorPanel({ children, label }: { children: ReactNode; label: string }) {
  return (
    <div className="runbook-editor-panel invoice-lexical-panel" role="menu">
      <span>{label}</span>
      {children}
    </div>
  );
}

function PanelButton({ active, label, onClick }: { active?: boolean; label: string; onClick: () => void }) {
  return <button className={active ? "active" : ""} onClick={onClick} role="menuitem" type="button">{label}</button>;
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
  if (href) editor.dispatchCommand(TOGGLE_LINK_COMMAND, href);
}

function insertLexicalBlock(editor: LexicalEditor) {
  editor.update(() => {
    const paragraph = $createParagraphNode();
    paragraph.append($createTextNode("New section"));
    $insertNodes([paragraph]);
  });
}

const enLabels = {
  alignLeft: "Align",
  blockStyle: "Block style",
  bold: "Bold",
  code: "Code",
  fontFamily: "Font",
  fontSize: "Size",
  highlight: "Highlight",
  insert: "Insert block",
  italic: "Italic",
  link: "Link",
  linkPrompt: "Paste a link URL",
  placeholder: "Write text...",
  redo: "Redo",
  textColor: "Text color",
  templates: "Text templates",
  undo: "Undo",
  underline: "Underline"
};

const deLabels = {
  alignLeft: "Ausrichtung",
  blockStyle: "Blockstil",
  bold: "Fett",
  code: "Code",
  fontFamily: "Schrift",
  fontSize: "Groesse",
  highlight: "Markieren",
  insert: "Block einfuegen",
  italic: "Kursiv",
  link: "Link",
  linkPrompt: "Link-URL einfuegen",
  placeholder: "Text schreiben...",
  redo: "Wiederholen",
  textColor: "Textfarbe",
  templates: "Textvorlagen",
  undo: "Rueckgaengig",
  underline: "Unterstrichen"
};
