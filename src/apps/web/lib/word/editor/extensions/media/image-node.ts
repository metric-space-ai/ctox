import type { DOMOutputSpec } from "prosemirror-model";
import { defineRuntimeNode, mergeRuntimeAttributes, type RuntimeAttributeValue } from "../../runtime";
import { normalizeImageDimensions } from "./dimensions";
import type { InsertInlineImageInput, WordMediaImageAttrs, WordMediaImageOptions } from "./types";

export const WordInlineImage = defineRuntimeNode<WordMediaImageOptions, Record<string, never>, WordMediaImageAttrs>({
  name: "image",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,
  draggable: true,

  addOptions() {
    return {
      minResizePx: 20,
      htmlAttributes: {
        class: "word-editor-image",
        loading: "lazy",
        decoding: "async",
      },
    };
  },

  addAttributes() {
    return {
      id: {
        default: null,
        rendered: false,
        parseDOM: (node) => node.getAttribute("data-word-id"),
      },
      mediaId: {
        default: null,
        parseDOM: (node) => node.getAttribute("data-word-media-id"),
        renderDOM: ({ mediaId }) => mediaId ? { "data-word-media-id": mediaId } : null,
      },
      src: {
        default: "",
        parseDOM: (node) => node.getAttribute("src"),
        renderDOM: ({ src }) => typeof src === "string" && src ? { src } : { src: "" },
      },
      altText: {
        default: "",
        parseDOM: (node) => node.getAttribute("alt"),
        renderDOM: ({ altText, decorative }) => ({ alt: decorative ? "" : String(altText ?? "") }),
      },
      title: {
        default: null,
        parseDOM: (node) => node.getAttribute("title"),
        renderDOM: ({ title }) => title ? { title } : null,
      },
      widthEmu: { default: null, rendered: false },
      heightEmu: { default: null, rendered: false },
      widthPx: { default: null, rendered: false },
      heightPx: { default: null, rendered: false },
      lockAspectRatio: { default: true, rendered: false },
      decorative: {
        default: false,
        renderDOM: ({ decorative }) => decorative ? { "aria-hidden": "true", role: "presentation" } : null,
      },
      resize: { default: null, rendered: false },
    };
  },

  parseDOM() {
    return [{ tag: "img[data-word-media-id]" }];
  },

  renderDOM({ node, htmlAttributes }): DOMOutputSpec {
    return ["img", mergeRuntimeAttributes(this.options.htmlAttributes, htmlAttributes, imageStyleAttrs(node.attrs))];
  },
});

export function createInlineImageAttrs(input: InsertInlineImageInput): WordMediaImageAttrs {
  const dimensions = normalizeImageDimensions(input);
  return {
    id: input.id ?? null,
    mediaId: input.mediaId,
    src: input.src ?? "",
    altText: input.decorative ? "" : input.altText ?? "",
    title: input.title ?? null,
    widthEmu: dimensions.widthEmu ?? null,
    heightEmu: dimensions.heightEmu ?? null,
    widthPx: dimensions.widthPx ?? null,
    heightPx: dimensions.heightPx ?? null,
    lockAspectRatio: input.lockAspectRatio ?? true,
    decorative: input.decorative ?? false,
    resize: null,
  };
}

function imageStyleAttrs(attrs: WordMediaImageAttrs): Record<string, RuntimeAttributeValue> | null {
  const widthPx = typeof attrs.widthPx === "number" ? attrs.widthPx : null;
  const heightPx = typeof attrs.heightPx === "number" ? attrs.heightPx : null;
  const style = [
    widthPx ? `width: ${widthPx}px` : "",
    heightPx ? `height: ${heightPx}px` : "",
  ].filter(Boolean).join("; ");
  return style ? { style } : null;
}
