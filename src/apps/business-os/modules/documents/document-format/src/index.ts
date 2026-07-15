import {
  exportWordPortDocumentToDocx,
  importDocxToWordPortDocument,
  openWordPortPackage,
  type WordPortBlock,
  type WordPortDocument,
  type WordPortInline,
  type WordPortParagraph,
  type WordPortTable,
  type WordPortTableCell,
} from "@ctox-word-port-archive";

export const DOCUMENT_FORMAT_ESM_VERSION = "0.2.0-wordport-format";

export type DocumentFormatImportResult = {
  document: WordPortDocument;
  diagnostics: unknown[];
};

export type DocxMailMergeResult = {
  bytes: Uint8Array;
  mergedFields: string[];
  missingFields: string[];
  replacedText: string[];
  missingTextReplacements: string[];
};

export async function importDocx(input: ArrayBuffer | Uint8Array | Blob): Promise<DocumentFormatImportResult> {
  const pkg = await openWordPortPackage(input);
  const document = await importDocxToWordPortDocument(pkg);
  return {
    document,
    diagnostics: pkg.diagnostics ?? [],
  };
}

export async function mergeDocxFields(
  input: ArrayBuffer | Uint8Array | Blob,
  values: Record<string, unknown>,
  options: { strict?: boolean; textReplacements?: Record<string, unknown> } = {},
): Promise<DocxMailMergeResult> {
  const pkg = await openWordPortPackage(input);
  const document = await importDocxToWordPortDocument(pkg);
  const report = materializeDocxMergeFields(document, values);
  const textReport = materializeDocxTextReplacements(document, options.textReplacements ?? {});
  if (options.strict !== false && (report.missingFields.length || textReport.missingTextReplacements.length)) {
    const missing = [...report.missingFields, ...textReport.missingTextReplacements];
    throw new Error(`DOCX merge values are missing for: ${missing.join(", ")}`);
  }
  return {
    bytes: await exportWordPortDocumentToDocx(document, { package: pkg }),
    ...report,
    ...textReport,
  };
}

export function materializeDocxMergeFields(
  document: WordPortDocument,
  values: Record<string, unknown>,
): Pick<DocxMailMergeResult, "mergedFields" | "missingFields"> {
  const normalizedValues = new Map(
    Object.entries(values ?? {}).map(([key, value]) => [key.trim().toLowerCase(), value == null ? "" : String(value)]),
  );
  const merged = new Set<string>();
  const missing = new Set<string>();

  visitDocumentValue(document, (reference) => {
    const field = reference.field;
    const name = mergeFieldName(field?.instruction);
    if (!field || !name) return;
    const value = normalizedValues.get(name.toLowerCase());
    if (value === undefined) {
      missing.add(name);
      return;
    }
    materializeFieldReference(field, value);
    merged.add(name);
  });
  materializePlainMergePlaceholders(document, normalizedValues, merged, missing);

  return {
    mergedFields: [...merged].sort((left, right) => left.localeCompare(right)),
    missingFields: [...missing].sort((left, right) => left.localeCompare(right)),
  };
}

export function materializeDocxTextReplacements(
  document: WordPortDocument,
  replacements: Record<string, unknown>,
): Pick<DocxMailMergeResult, "replacedText" | "missingTextReplacements"> {
  const replaced = new Set<string>();
  const missing = new Set<string>();
  for (const [search, rawReplacement] of Object.entries(replacements ?? {})) {
    const replacement = rawReplacement == null ? "" : String(rawReplacement);
    const count = replaceDocumentText(document, search, replacement);
    if (count) replaced.add(search);
    else missing.add(search);
  }
  return {
    replacedText: [...replaced].sort((left, right) => left.localeCompare(right)),
    missingTextReplacements: [...missing].sort((left, right) => left.localeCompare(right)),
  };
}

function replaceDocumentText(value: unknown, search: string, replacement: string): number {
  if (!search) return 0;
  if (Array.isArray(value)) {
    return value.reduce((count, item) => count + replaceDocumentText(item, search, replacement), 0);
  }
  if (!value || typeof value !== "object") return 0;
  const candidate = value as Record<string, unknown>;
  let count = 0;
  for (const [key, item] of Object.entries(candidate)) {
    if (key === "text" && typeof item === "string" && item.includes(search)) {
      candidate[key] = item.split(search).join(replacement);
      count += item.split(search).length - 1;
      continue;
    }
    count += replaceDocumentText(item, search, replacement);
  }
  return count;
}

function materializePlainMergePlaceholders(
  value: unknown,
  values: Map<string, string>,
  merged: Set<string>,
  missing: Set<string>,
): void {
  if (Array.isArray(value)) {
    value.forEach((item) => materializePlainMergePlaceholders(item, values, merged, missing));
    return;
  }
  if (!value || typeof value !== "object") return;
  const candidate = value as Record<string, unknown>;
  for (const [key, item] of Object.entries(candidate)) {
    if (key === "text" && typeof item === "string") {
      candidate[key] = item.replace(/«\s*([^»]+?)\s*»/g, (token, rawName: string) => {
        const name = rawName.trim();
        const replacement = values.get(name.toLowerCase());
        if (replacement === undefined) {
          missing.add(name);
          return token;
        }
        merged.add(name);
        return replacement;
      });
      continue;
    }
    materializePlainMergePlaceholders(item, values, merged, missing);
  }
}

type MutableFieldReference = NonNullable<Extract<WordPortInline, { type: "reference" }>["field"]>;
type XmlNode = NonNullable<MutableFieldReference["rawNode"]>;

function visitDocumentValue(value: unknown, visit: (reference: Extract<WordPortInline, { type: "reference" }>) => void): void {
  if (Array.isArray(value)) {
    value.forEach((item) => visitDocumentValue(item, visit));
    return;
  }
  if (!value || typeof value !== "object") return;
  const candidate = value as Record<string, unknown>;
  if (candidate.type === "reference") visit(value as Extract<WordPortInline, { type: "reference" }>);
  Object.values(candidate).forEach((item) => visitDocumentValue(item, visit));
}

function mergeFieldName(instruction: string | undefined): string | undefined {
  const match = String(instruction ?? "").match(/^\s*MERGEFIELD\s+(?:"([^"]+)"|([^\\\s]+))/i);
  return (match?.[1] ?? match?.[2])?.trim() || undefined;
}

function materializeFieldReference(field: MutableFieldReference, value: string): void {
  const resultRuns = field.kind === "complex"
    ? complexResultRuns(field.rawNodes ?? [])
    : simpleResultRuns(field.rawNode);
  if (resultRuns.length) {
    setResultRunText(resultRuns, value);
    field.kind = "complex";
    field.rawNodes = resultRuns;
    delete field.rawNode;
  } else {
    delete field.rawNodes;
    delete field.rawNode;
    field.supported = false;
  }
  field.resultText = value;
  field.displayText = value;
}

function complexResultRuns(nodes: XmlNode[]): XmlNode[] {
  let collecting = false;
  const result: XmlNode[] = [];
  for (const node of nodes) {
    const fieldType = descendantAttribute(node, "w:fldChar", "w:fldCharType");
    if (fieldType === "separate") {
      collecting = true;
      continue;
    }
    if (fieldType === "end") break;
    if (collecting) result.push(cloneXmlNode(node));
  }
  return result;
}

function simpleResultRuns(node: XmlNode | undefined): XmlNode[] {
  return (node?.children ?? []).map(cloneXmlNode);
}

function descendantAttribute(node: XmlNode, name: string, attribute: string): string | undefined {
  if (node.name === name) return node.attributes?.[attribute];
  for (const child of node.children ?? []) {
    const value = descendantAttribute(child, name, attribute);
    if (value !== undefined) return value;
  }
  return undefined;
}

function setResultRunText(nodes: XmlNode[], value: string): void {
  const textNodes: XmlNode[] = [];
  nodes.forEach((node) => collectXmlNodes(node, "w:t", textNodes));
  if (!textNodes.length) {
    const run = nodes.find((node) => node.name === "w:r") ?? nodes[0];
    run.children = [...(run.children ?? []), wordTextNode(value)];
    return;
  }
  textNodes.forEach((node, index) => {
    node.children = [{ name: "#text", text: index === 0 ? value : "" }];
    if (index === 0 && /^\s|\s$/.test(value)) {
      node.attributes = { ...(node.attributes ?? {}), "xml:space": "preserve" };
    }
  });
}

function collectXmlNodes(node: XmlNode, name: string, result: XmlNode[]): void {
  if (node.name === name) result.push(node);
  (node.children ?? []).forEach((child) => collectXmlNodes(child, name, result));
}

function wordTextNode(value: string): XmlNode {
  return {
    name: "w:t",
    ...(/^\s|\s$/.test(value) ? { attributes: { "xml:space": "preserve" } } : {}),
    children: [{ name: "#text", text: value }],
  };
}

function cloneXmlNode(node: XmlNode): XmlNode {
  return JSON.parse(JSON.stringify(node)) as XmlNode;
}

export function importMarkdown(markdown: string): DocumentFormatImportResult {
  return {
    document: markdownToWordPortDocument(markdown),
    diagnostics: [],
  };
}

export function exportMarkdown(document: WordPortDocument): string {
  return (document.body?.blocks ?? []).map(blockToMarkdown).join("\n\n").replace(/\n{3,}/g, "\n\n").trimEnd() + "\n";
}

export function getDocumentText(document: WordPortDocument): string {
  return (document.body?.blocks ?? []).map(blockText).filter(Boolean).join("\n");
}

function paragraphText(paragraph: WordPortParagraph): string {
  return paragraph.runs.map(inlineText).join("");
}

function blockText(block: WordPortBlock): string {
  if (block.type === "paragraph") return paragraphText(block);
  if (block.type === "table") {
    return block.rows.map((row) => row.cells.map(cellText).join("\t")).join("\n");
  }
  if (block.type === "blockWrapper") return block.blocks.map(blockText).join("\n");
  return "";
}

function cellText(cell: WordPortTableCell): string {
  return cell.blocks.map(blockText).join("\n");
}

function inlineText(inline: WordPortInline): string {
  if (inline.type === "text") return inline.text;
  if (inline.type === "run") return inline.content.map(inlineText).join("");
  if (inline.type === "tab") return "\t";
  if (inline.type === "break") return "\n";
  if (inline.type === "hyperlink") return inline.content.map(inlineText).join("");
  if (inline.type === "inlineWrapper") return inline.content.map(inlineText).join("");
  if (inline.type === "reference") return inline.field?.displayText ?? "";
  if (inline.type === "drawing") return inline.altText ? `[${inline.altText}]` : "";
  return "";
}

function markdownToWordPortDocument(markdown: string): WordPortDocument {
  const blocks: WordPortBlock[] = [];
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  let paragraph: string[] = [];
  let tableRows: string[][] = [];
  const flushParagraph = () => {
    if (!paragraph.length) return;
    blocks.push(markdownParagraph(paragraph.join("\n")));
    paragraph = [];
  };
  const flushTable = () => {
    if (!tableRows.length) return;
    blocks.push({
      type: "table",
      rows: tableRows.map((cells) => ({
        type: "tableRow",
        cells: cells.map((cell) => ({
          type: "tableCell",
          blocks: [markdownParagraph(cell.trim())],
        })),
      })),
    });
    tableRows = [];
  };

  for (const line of lines) {
    if (!line.trim()) {
      flushParagraph();
      flushTable();
      continue;
    }
    if (/^\s*\|.*\|\s*$/.test(line)) {
      flushParagraph();
      const cells = line.trim().replace(/^\|/, "").replace(/\|$/, "").split("|");
      if (!cells.every((cell) => /^:?-{3,}:?$/.test(cell.trim()))) tableRows.push(cells);
      continue;
    }
    flushTable();
    paragraph.push(line);
  }
  flushParagraph();
  flushTable();
  return {
    type: "document",
    body: { type: "body", blocks },
    source: { documentAttributes: { "data-document-mode": "markdown" } },
  };
}

function markdownParagraph(text: string): WordPortParagraph {
  const heading = text.match(/^(#{1,6})\s+(.*)$/);
  const quote = text.match(/^>\s?(.*)$/);
  const bullet = text.match(/^[-*+]\s+(.*)$/);
  const ordered = text.match(/^\d+[.)]\s+(.*)$/);
  const cleanText = heading?.[2] ?? quote?.[1] ?? bullet?.[1] ?? ordered?.[1] ?? text;
  const paragraph: WordPortParagraph = {
    type: "paragraph",
    runs: [{ type: "text", text: cleanText }],
  };
  if (heading) {
    paragraph.effectiveProperties = { styleId: `Heading${heading[1].length}` };
  }
  if (quote) {
    paragraph.effectiveProperties = { ...(paragraph.effectiveProperties ?? {}), styleId: "Quote" };
  }
  if (bullet || ordered) {
    paragraph.effectiveProperties = {
      ...(paragraph.effectiveProperties ?? {}),
      listRendering: {
        markerText: bullet ? "-" : "1.",
        markerSuffixText: " ",
        level: 0,
        numberingId: ordered ? "markdown-ordered" : "markdown-bullet",
        abstractId: ordered ? "markdown-ordered" : "markdown-bullet",
        numberingType: ordered ? "decimal" : "bullet",
        path: [1],
        suffix: "space",
      },
    };
  }
  return paragraph;
}

function blockToMarkdown(block: WordPortBlock): string {
  if (block.type === "paragraph") return paragraphToMarkdown(block);
  if (block.type === "table") return tableToMarkdown(block);
  if (block.type === "blockWrapper") return block.blocks.map(blockToMarkdown).join("\n\n");
  return `<!-- Unsupported block: ${block.name} -->`;
}

function paragraphToMarkdown(paragraph: WordPortParagraph): string {
  const text = paragraphText(paragraph);
  const styleId = paragraph.effectiveProperties?.styleId ?? "";
  const heading = styleId.match(/^Heading([1-6])$/i);
  if (heading) return `${"#".repeat(Number(heading[1]))} ${text}`;
  const list = paragraph.effectiveProperties?.listRendering;
  if (list?.numberingType === "bullet") return `- ${text}`;
  if (list?.numberingType === "decimal") return `1. ${text}`;
  if (/quote/i.test(styleId)) return `> ${text}`;
  return text;
}

function tableToMarkdown(table: WordPortTable): string {
  const rows = table.rows.map((row) => row.cells.map((cell) => cellText(cell).replace(/\s+/g, " ").trim()));
  if (!rows.length) return "";
  const columnCount = Math.max(...rows.map((row) => row.length));
  const normalize = (row: string[]) => Array.from({ length: columnCount }, (_, index) => row[index] ?? "");
  const [head, ...body] = rows.map(normalize);
  return [
    `| ${head.join(" | ")} |`,
    `| ${head.map(() => "---").join(" | ")} |`,
    ...body.map((row) => `| ${row.join(" | ")} |`),
  ].join("\n");
}
