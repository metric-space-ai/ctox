import { NodeSelection } from "prosemirror-state";
import { defineRuntimeExtension } from "../../runtime";
import { createResizeMetadata, imageAttrsWithDimensions } from "./dimensions";
import { createInlineImageAttrs } from "./image-node";
import type { InsertInlineImageInput, WordMediaImageAttrs, WordMediaImageOptions } from "./types";

export const WordMediaCommands = defineRuntimeExtension<WordMediaImageOptions>({
  name: "wordMediaCommands",

  addOptions() {
    return { minResizePx: 20 };
  },

  addCommands() {
    return {
      insertInlineImage:
        (input: InsertInlineImageInput) =>
        ({ state, dispatch }) => {
          const image = state.schema.nodes.image;
          if (!image || !input.mediaId) return false;
          if (dispatch) dispatch(state.tr.replaceSelectionWith(image.create(createInlineImageAttrs(input))).scrollIntoView());
          return true;
        },

      updateSelectedImage:
        (attrs: Partial<WordMediaImageAttrs>) =>
        ({ state, dispatch }) => {
          const selection = state.selection;
          if (!(selection instanceof NodeSelection) || selection.node.type.name !== "image") return false;
          if (dispatch) dispatch(state.tr.setNodeMarkup(selection.from, undefined, { ...selection.node.attrs, ...attrs }));
          return true;
        },

      setSelectedImageAltText:
        (altText: string) =>
        ({ state, dispatch }) => {
          const selection = state.selection;
          if (!(selection instanceof NodeSelection) || selection.node.type.name !== "image") return false;
          const attrs = { ...selection.node.attrs, altText, decorative: false };
          if (dispatch) dispatch(state.tr.setNodeMarkup(selection.from, undefined, attrs));
          return true;
        },

      setSelectedImageDimensions:
        (widthPx: number, heightPx?: number) =>
        ({ state, dispatch }) => {
          const selection = state.selection;
          if (!(selection instanceof NodeSelection) || selection.node.type.name !== "image") return false;
          const current = selection.node.attrs as WordMediaImageAttrs;
          const resize = createResizeMetadata({
            widthPx,
            heightPx,
            prior: current.resize,
            lockAspectRatio: current.lockAspectRatio,
            minResizePx: this.options.minResizePx,
          });
          if (dispatch) dispatch(state.tr.setNodeMarkup(selection.from, undefined, imageAttrsWithDimensions(current, resize)));
          return true;
        },
    };
  },
});
