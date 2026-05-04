"use client";

import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { useEffect } from "react";

type CtoxNavigateEvent = CustomEvent<{ href?: string }>;

const lastSubmodulePrefix = "ctox:last-submodule:";

export function ClientNavigationBridge() {
  const pathname = usePathname();
  const router = useRouter();
  const searchParams = useSearchParams();

  useEffect(() => {
    const route = parseAppRoute(pathname);
    if (!route?.submodule) return;
    window.localStorage.setItem(`${lastSubmodulePrefix}${route.module}`, route.submodule);
  }, [pathname]);

  useEffect(() => {
    document.querySelectorAll<HTMLAnchorElement>(".module-nav a").forEach((anchor) => {
      const href = anchor.getAttribute("href");
      const rememberedHref = href ? rememberedModuleHref(href) : null;
      if (rememberedHref) anchor.setAttribute("href", rememberedHref);
    });
  }, [pathname, searchParams]);

  useEffect(() => {
    const navigate = (href: string) => {
      const target = internalAppHref(rememberedModuleHref(href) ?? href);
      if (!target) return false;
      router.push(target, { scroll: false });
      return true;
    };

    const onClick = (event: MouseEvent) => {
      if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
      const target = event.target instanceof Element ? event.target.closest("a") : null;
      if (!target || target.target === "_blank" || target.hasAttribute("download") || target.dataset.hardNavigation === "true") return;

      const href = target.getAttribute("href");
      if (!href || !navigate(href)) return;
      event.preventDefault();
    };

    const onCtoxNavigate = (event: Event) => {
      const href = (event as CtoxNavigateEvent).detail?.href;
      if (!href || !navigate(href)) return;
      event.preventDefault();
    };

    window.addEventListener("click", onClick, true);
    window.addEventListener("ctox:navigate", onCtoxNavigate);
    return () => {
      window.removeEventListener("click", onClick, true);
      window.removeEventListener("ctox:navigate", onCtoxNavigate);
    };
  }, [router]);

  return <span data-client-navigation-bridge hidden />;
}

function rememberedModuleHref(href: string) {
  const url = new URL(href, window.location.href);
  if (url.origin !== window.location.origin) return null;
  const route = parseAppRoute(url.pathname);
  if (!route || route.submodule) return null;

  const rememberedSubmodule = window.localStorage.getItem(`${lastSubmodulePrefix}${route.module}`);
  if (!rememberedSubmodule) return null;

  url.pathname = `/app/${route.module}/${rememberedSubmodule}`;
  return `${url.pathname}${url.search}${url.hash}`;
}

function internalAppHref(href: string) {
  const url = new URL(href, window.location.href);
  if (url.origin !== window.location.origin || !url.pathname.startsWith("/app/")) return null;
  return `${url.pathname}${url.search}${url.hash}`;
}

function parseAppRoute(pathname: string) {
  const [, app, module, submodule] = pathname.split("/");
  if (app !== "app" || !module) return null;
  if (!["business", "ctox", "marketing", "operations", "sales"].includes(module)) return null;
  return { module, submodule };
}
