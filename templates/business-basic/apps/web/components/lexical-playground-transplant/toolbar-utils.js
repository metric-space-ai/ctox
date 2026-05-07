/**
 * Adapted from facebook/lexical packages/lexical-playground ToolbarPlugin/utils.ts (MIT).
 * The original playground helpers are reduced to the document operations CTOX needs.
 */
import { $createCodeNode } from "@lexical/code";
import {
  INSERT_ORDERED_LIST_COMMAND,
  INSERT_UNORDERED_LIST_COMMAND
} from "@lexical/list";
import {
  $createHeadingNode,
  $createQuoteNode
} from "@lexical/rich-text";
import { $patchStyleText, $setBlocksType } from "@lexical/selection";
import {
  $addUpdateTag,
  $createParagraphNode,
  $getSelection,
  $isRangeSelection,
  SKIP_DOM_SELECTION_TAG,
  SKIP_SELECTION_FOCUS_TAG
} from "lexical";

export const MIN_ALLOWED_FONT_SIZE = 8;
export const MAX_ALLOWED_FONT_SIZE = 72;
export const DEFAULT_FONT_SIZE = 15;

export const UpdateFontSizeType = {
  increment: 1,
  decrement: 2
};

export function calculateNextFontSize(currentFontSize, updateType) {
  if (!updateType) return currentFontSize;
  if (updateType === UpdateFontSizeType.decrement) {
    if (currentFontSize > MAX_ALLOWED_FONT_SIZE) return MAX_ALLOWED_FONT_SIZE;
    if (currentFontSize >= 48) return currentFontSize - 12;
    if (currentFontSize >= 24) return currentFontSize - 4;
    if (currentFontSize >= 14) return currentFontSize - 2;
    if (currentFontSize >= 9) return currentFontSize - 1;
    return MIN_ALLOWED_FONT_SIZE;
  }
  if (currentFontSize < MIN_ALLOWED_FONT_SIZE) return MIN_ALLOWED_FONT_SIZE;
  if (currentFontSize < 12) return currentFontSize + 1;
  if (currentFontSize < 20) return currentFontSize + 2;
  if (currentFontSize < 36) return currentFontSize + 4;
  if (currentFontSize <= 60) return currentFontSize + 12;
  return MAX_ALLOWED_FONT_SIZE;
}

export function updateFontSizeInSelection(editor, newFontSize, updateType, skipRefocus = false) {
  const getNextFontSize = (prevFontSize) => {
    const base = prevFontSize ? Number(prevFontSize.slice(0, -2)) : DEFAULT_FONT_SIZE;
    return `${calculateNextFontSize(base, updateType)}px`;
  };

  editor.update(() => {
    if (skipRefocus) $addUpdateTag(SKIP_DOM_SELECTION_TAG);
    const selection = $getSelection();
    if (selection) {
      $patchStyleText(selection, { "font-size": newFontSize || getNextFontSize });
    }
  });
}

export function formatParagraph(editor) {
  editor.update(() => {
    $addUpdateTag(SKIP_SELECTION_FOCUS_TAG);
    $setBlocksType($getSelection(), () => $createParagraphNode());
  });
}

export function formatHeading(editor, headingSize) {
  editor.update(() => {
    $addUpdateTag(SKIP_SELECTION_FOCUS_TAG);
    $setBlocksType($getSelection(), () => $createHeadingNode(headingSize));
  });
}

export function formatBulletList(editor) {
  editor.update(() => {
    $addUpdateTag(SKIP_SELECTION_FOCUS_TAG);
    editor.dispatchCommand(INSERT_UNORDERED_LIST_COMMAND, undefined);
  });
}

export function formatNumberedList(editor) {
  editor.update(() => {
    $addUpdateTag(SKIP_SELECTION_FOCUS_TAG);
    editor.dispatchCommand(INSERT_ORDERED_LIST_COMMAND, undefined);
  });
}

export function formatQuote(editor) {
  editor.update(() => {
    $addUpdateTag(SKIP_SELECTION_FOCUS_TAG);
    $setBlocksType($getSelection(), () => $createQuoteNode());
  });
}

export function formatCode(editor) {
  editor.update(() => {
    $addUpdateTag(SKIP_SELECTION_FOCUS_TAG);
    const selection = $getSelection();
    if ($isRangeSelection(selection)) {
      $setBlocksType(selection, () => $createCodeNode());
    }
  });
}
