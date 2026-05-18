import {
  importDocxToWordPortDocument,
  openWordPortPackage,
  type WordPortBlock,
  type WordPortDocument,
  type WordPortInline,
  type WordPortParagraph,
  type WordPortTable,
  type WordPortTableCell,
} from "../../../../../templates/business-basic/apps/web/lib/word-port";

export const DOCUMENT_FORMAT_ESM_VERSION = "0.2.0-wordport-format";

export type DocumentFormatImportResult = {
  document: WordPortDocument;
  diagnostics: unknown[];
};

export async function importDocx(input: ArrayBuffer | Uint8Array | Blob): Promise<DocumentFormatImportResult> {
  const pkg = await openWordPortPackage(input);
  const document = await importDocxToWordPortDocument(pkg);
  return {
    document,
    diagnostics: pkg.diagnostics ?? [],
  };
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
