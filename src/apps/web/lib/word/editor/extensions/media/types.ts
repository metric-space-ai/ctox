import type { RuntimeAttributeValue } from "../../runtime";

export type WordMediaSourceKind = "data-url" | "object-url" | "remote-url" | "empty";

export type WordMediaSourceRequest = {
  mediaId?: string | null;
  src?: string | null;
  contentType?: string | null;
  bytesBase64?: string | null;
  fileName?: string | null;
};

export type WordMediaSourceResult = {
  src: string;
  kind: WordMediaSourceKind;
  contentType?: string;
  revoke?: () => void;
};

export type WordMediaSafeSrcAdapter = {
  resolveImageSrc(input: WordMediaSourceRequest): WordMediaSourceResult | Promise<WordMediaSourceResult>;
};

export type WordImageResizeMetadata = {
  widthEmu?: number;
  heightEmu?: number;
  widthPx?: number;
  heightPx?: number;
  lockAspectRatio: boolean;
  resizedAt?: string;
};

export type WordMediaImageAttrs = Record<string, RuntimeAttributeValue> & {
  id?: string | null;
  mediaId?: string | null;
  src?: string | null;
  altText?: string | null;
  title?: string | null;
  widthEmu?: number | null;
  heightEmu?: number | null;
  widthPx?: number | null;
  heightPx?: number | null;
  lockAspectRatio?: boolean | null;
  decorative?: boolean | null;
  resize?: WordImageResizeMetadata | null;
};

export type WordMediaImageOptions = {
  htmlAttributes?: Record<string, RuntimeAttributeValue>;
  minResizePx?: number;
};

export type InsertInlineImageInput = {
  id?: string;
  mediaId: string;
  src?: string;
  altText?: string;
  title?: string;
  widthEmu?: number;
  heightEmu?: number;
  widthPx?: number;
  heightPx?: number;
  lockAspectRatio?: boolean;
  decorative?: boolean;
};
