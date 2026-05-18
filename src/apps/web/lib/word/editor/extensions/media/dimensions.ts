import type { InsertInlineImageInput, WordImageResizeMetadata, WordMediaImageAttrs } from "./types";

export const EMU_PER_PIXEL_96_DPI = 9525;
export const DEFAULT_MIN_IMAGE_SIZE_PX = 20;

export function pxToEmu(px: number): number {
  return Math.round(px * EMU_PER_PIXEL_96_DPI);
}

export function emuToPxRounded(emu: number): number {
  return Math.round(emu / EMU_PER_PIXEL_96_DPI);
}

export function normalizeImageDimensions(input: Pick<InsertInlineImageInput, "widthEmu" | "heightEmu" | "widthPx" | "heightPx">): {
  widthEmu?: number;
  heightEmu?: number;
  widthPx?: number;
  heightPx?: number;
} {
  const widthPx = positiveNumber(input.widthPx) ?? (positiveNumber(input.widthEmu) ? emuToPxRounded(input.widthEmu as number) : undefined);
  const heightPx = positiveNumber(input.heightPx) ?? (positiveNumber(input.heightEmu) ? emuToPxRounded(input.heightEmu as number) : undefined);
  return {
    widthPx,
    heightPx,
    widthEmu: positiveNumber(input.widthEmu) ?? (widthPx ? pxToEmu(widthPx) : undefined),
    heightEmu: positiveNumber(input.heightEmu) ?? (heightPx ? pxToEmu(heightPx) : undefined),
  };
}

export function createResizeMetadata(input: {
  widthPx: number;
  heightPx?: number;
  prior?: WordImageResizeMetadata | null;
  lockAspectRatio?: boolean | null;
  minResizePx?: number;
  now?: () => Date;
}): WordImageResizeMetadata {
  const minSize = input.minResizePx ?? DEFAULT_MIN_IMAGE_SIZE_PX;
  const widthPx = Math.max(minSize, Math.round(input.widthPx));
  const heightPx = input.heightPx === undefined ? undefined : Math.max(minSize, Math.round(input.heightPx));
  return {
    ...input.prior,
    widthPx,
    heightPx,
    widthEmu: pxToEmu(widthPx),
    heightEmu: heightPx === undefined ? undefined : pxToEmu(heightPx),
    lockAspectRatio: input.lockAspectRatio ?? input.prior?.lockAspectRatio ?? true,
    resizedAt: (input.now ?? (() => new Date()))().toISOString(),
  };
}

export function imageAttrsWithDimensions(attrs: WordMediaImageAttrs, resize: WordImageResizeMetadata): WordMediaImageAttrs {
  return {
    ...attrs,
    widthEmu: resize.widthEmu ?? null,
    heightEmu: resize.heightEmu ?? null,
    widthPx: resize.widthPx ?? null,
    heightPx: resize.heightPx ?? null,
    lockAspectRatio: resize.lockAspectRatio,
    resize,
  };
}

function positiveNumber(value: number | null | undefined): number | undefined {
  return typeof value === "number" && Number.isFinite(value) && value > 0 ? value : undefined;
}
