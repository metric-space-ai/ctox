import Link from "next/link";
import { cookies } from "next/headers";
import { parseWebsiteSession } from "../lib/website-session";

type PublicPage = {
  id: string;
  title: string;
  path: string;
  intent: string;
  updated: string;
};

export default async function Home() {
  const session = parseWebsiteSession((await cookies()).get("ctox_website_session")?.value);
  const pages = await fetchPublishedPages();
  const businessOsUrl = process.env.NEXT_PUBLIC_BUSINESS_OS_URL ?? process.env.BUSINESS_OS_URL ?? "";
  const canOpenBusinessOs = Boolean(
    businessOsUrl
    && session
    && (
      session.roles.includes("business_os_user")
      || session.roles.includes("business_os_admin")
      || session.permissions.includes("business_os:access")
      || session.permissions.includes("business_os:admin")
    )
  );

  return (
    <main className="site-shell">
      <header className="site-header">
        <Link className="site-brand" href="/">Website</Link>
        <nav>
          <Link href="/login">Login</Link>
          {canOpenBusinessOs ? <Link href={`${businessOsUrl}/app`}>Business OS</Link> : null}
        </nav>
      </header>

      <section className="hero">
        <p>Managed in Marketing / Website</p>
        <h1>Public website, separate repository.</h1>
        <span>
          Content can be governed by CTOX Business OS while this website stays
          independently deployed and publicly accessible.
        </span>
      </section>

      <section className="page-list" aria-label="Published website pages">
        {pages.length ? pages.map((page) => (
          <article key={page.id}>
            <span>{page.path}</span>
            <h2>{page.title}</h2>
            <p>{page.intent}</p>
            <small>Updated {page.updated}</small>
          </article>
        )) : (
          <article>
            <span>/</span>
            <h2>No published pages yet</h2>
            <p>Publish pages in Marketing / Website to expose them here.</p>
          </article>
        )}
      </section>
    </main>
  );
}

async function fetchPublishedPages() {
  const baseUrl = process.env.BUSINESS_OS_URL;
  if (!baseUrl) return [] as PublicPage[];

  const response = await fetch(`${baseUrl.replace(/\/$/, "")}/api/public/website/pages?locale=de`, {
    next: { revalidate: 60 }
  }).catch(() => null);
  if (!response?.ok) return [] as PublicPage[];

  const payload = await response.json().catch(() => null) as { data?: PublicPage[] } | null;
  return payload?.data ?? [];
}
