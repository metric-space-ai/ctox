import Link from "next/link";
import { resolveLocale, resolveThemeMode, withLocale, withThemeMode } from "@ctox-business/ui";

const basePath = process.env.NEXT_PUBLIC_BASE_PATH ?? "";

function appPath(path: string) {
  return `${basePath}${path}`;
}

export default async function LoginPage({
  searchParams
}: {
  searchParams: Promise<{ locale?: string; next?: string; theme?: string }>;
}) {
  const { locale, next, theme } = await searchParams;
  const activeLocale = resolveLocale(locale);
  const activeTheme = resolveThemeMode(theme);

  return (
    <main className="minimal-entry" data-theme={activeTheme}>
      <form className="module-card" action={appPath("/api/auth/login")} method="post">
        <h1>Login</h1>
        <p>Sign in to open Kunstmen Business OS.</p>
        <input name="next" type="hidden" value={next ?? appPath(withThemeMode(withLocale("/app", activeLocale), activeTheme))} />
        <label className="drawer-field">
          User
          <input autoComplete="username" name="user" type="text" />
        </label>
        <label className="drawer-field">
          Password
          <input autoComplete="current-password" name="password" type="password" />
        </label>
        <button className="login-link" type="submit">Continue</button>
        <p><Link href={appPath(withThemeMode(withLocale("/", activeLocale), activeTheme))}>Back</Link></p>
      </form>
    </main>
  );
}
