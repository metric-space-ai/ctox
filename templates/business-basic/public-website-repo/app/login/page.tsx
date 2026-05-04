export default function LoginPage() {
  return (
    <main className="login-shell">
      <form action="/api/auth/login" method="post">
        <h1>Website Login</h1>
        <p>Customer users stay on the website. Team users with Business OS roles can open the internal app.</p>
        <input name="next" type="hidden" value="/"/>
        <label>
          User
          <input autoComplete="username" name="user" type="text" />
        </label>
        <label>
          Password
          <input autoComplete="current-password" name="password" type="password" />
        </label>
        <button type="submit">Continue</button>
      </form>
    </main>
  );
}
