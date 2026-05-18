import type { WordMediaSafeSrcAdapter, WordMediaSourceRequest, WordMediaSourceResult } from "./types";

const rasterImageTypes = new Set(["image/png", "image/jpeg", "image/jpg", "image/gif", "image/webp", "image/bmp"]);

export type WordMediaSafeSrcAdapterOptions = {
  allowDataUrls?: boolean;
  allowBlobUrls?: boolean;
  allowRemoteUrls?: boolean;
  trustedRemoteOrigins?: string[];
  allowedContentTypes?: Iterable<string>;
};

export function createWordMediaSafeSrcAdapter(options: WordMediaSafeSrcAdapterOptions = {}): WordMediaSafeSrcAdapter {
  const allowedTypes = new Set(options.allowedContentTypes ?? rasterImageTypes);
  const trustedOrigins = new Set(options.trustedRemoteOrigins ?? []);
  return {
    resolveImageSrc(input) {
      const contentType = normalizeContentType(input.contentType);
      if (input.bytesBase64 && contentType && allowedTypes.has(contentType) && options.allowDataUrls !== false) {
        return { src: `data:${contentType};base64,${input.bytesBase64}`, kind: "data-url", contentType };
      }

      const src = String(input.src ?? "").trim();
      if (!src) return emptyResult(contentType);

      if (src.startsWith("data:")) {
        if (options.allowDataUrls === false) return emptyResult(contentType);
        const dataContentType = normalizeContentType(src.slice(5, src.indexOf(";") > 0 ? src.indexOf(";") : undefined));
        return dataContentType && allowedTypes.has(dataContentType)
          ? { src, kind: "data-url", contentType: dataContentType }
          : emptyResult(contentType);
      }

      if (src.startsWith("blob:")) {
        return options.allowBlobUrls === false ? emptyResult(contentType) : { src, kind: "object-url", contentType };
      }

      const remote = safeRemoteUrl(src, trustedOrigins, options.allowRemoteUrls === true);
      return remote ? { src: remote, kind: "remote-url", contentType } : emptyResult(contentType);
    },
  };
}

function safeRemoteUrl(src: string, trustedOrigins: Set<string>, allowRemoteUrls: boolean): string | null {
  try {
    const url = new URL(src, typeof window === "undefined" ? "http://localhost" : window.location.href);
    if (url.protocol !== "http:" && url.protocol !== "https:") return null;
    if (url.origin === (typeof window === "undefined" ? "http://localhost" : window.location.origin)) return url.href;
    return allowRemoteUrls && trustedOrigins.has(url.origin) ? url.href : null;
  } catch {
    return null;
  }
}

function normalizeContentType(contentType: string | null | undefined): string | undefined {
  return contentType?.split(";")[0]?.trim().toLowerCase() || undefined;
}

function emptyResult(contentType?: string): WordMediaSourceResult {
  return { src: "", kind: "empty", contentType };
}
