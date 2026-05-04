import { redirect } from "next/navigation";

export default async function CtoxPage({
  searchParams
}: {
  searchParams: Promise<{ locale?: string; theme?: string }>;
}) {
  const { locale, theme } = await searchParams;
  redirect(withQuery("/app/ctox/runs", { locale, theme }));
}

function withQuery(path: string, query: Record<string, string | undefined>) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  const queryString = params.toString();
  return queryString ? `${path}?${queryString}` : path;
}
