"use client";

import { useEffect } from "react";

export function BusinessApiPrefix() {
  useEffect(() => {
    const basePath = (process.env.NEXT_PUBLIC_BASE_PATH ?? "").replace(/\/$/, "");
    if (!basePath || !basePath.startsWith("/")) return;
    if ((window.fetch as typeof window.fetch & { __businessApiPrefixed?: boolean }).__businessApiPrefixed) return;

    const originalFetch = window.fetch.bind(window);
    const prefixedFetch: typeof window.fetch & { __businessApiPrefixed?: boolean } = (input, init) => {
      if (typeof input === "string" && input.startsWith("/api/")) {
        return originalFetch(`${basePath}${input}`, init);
      }
      if (input instanceof Request) {
        const url = new URL(input.url);
        if (url.origin === window.location.origin && url.pathname.startsWith("/api/")) {
          const next = new Request(`${basePath}${url.pathname}${url.search}`, input);
          return originalFetch(next, init);
        }
      }
      return originalFetch(input, init);
    };
    prefixedFetch.__businessApiPrefixed = true;
    window.fetch = prefixedFetch;

    return () => {
      window.fetch = originalFetch;
    };
  }, []);

  return null;
}
