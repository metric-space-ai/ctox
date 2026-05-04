import { redirect } from "next/navigation";

export default async function AppHome({
  searchParams
}: {
  searchParams: Promise<{ locale?: string; theme?: string }>;
}) {
  const { locale, theme } = await searchParams;
  redirect(withQuery("/app/operations/projects", { locale, theme }));
}

function withQuery(path: string, params: Record<string, string | undefined>) {
  const query = new URLSearchParams();
  Object.entries(params).forEach(([key, value]) => {
    if (value) query.set(key, value);
  });
  const serialized = query.toString();
  return serialized ? `${path}?${serialized}` : path;
}
