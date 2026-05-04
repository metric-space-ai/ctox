type WebsiteIdentity = {
  sub: string;
  roles: string[];
  permissions: string[];
};

type AuthUser = WebsiteIdentity & {
  email?: string;
  name?: string;
  password: string;
};

export function resolveWebsiteIdentity(user: string, password: string) {
  const normalizedUser = normalizeUser(user);
  if (!normalizedUser || !password) return null;

  return authUsers().find((candidate) => (
    normalizeUser(candidate.sub) === normalizedUser
    || normalizeUser(candidate.email) === normalizedUser
  ) && candidate.password === password) ?? null;
}

function authUsers(): AuthUser[] {
  const configured = parseConfiguredUsers(process.env.CTOX_AUTH_USERS ?? process.env.WEBSITE_AUTH_USERS);
  if (configured.length > 0) return configured;

  return [
    {
      sub: process.env.WEBSITE_USER ?? "customer",
      password: process.env.WEBSITE_PASSWORD ?? "customer",
      roles: splitList(process.env.WEBSITE_USER_ROLES ?? "customer"),
      permissions: splitList(process.env.WEBSITE_USER_PERMISSIONS ?? "")
    },
    {
      sub: process.env.WEBSITE_PARTNER_USER ?? "partner",
      password: process.env.WEBSITE_PARTNER_PASSWORD ?? "partner",
      roles: splitList(process.env.WEBSITE_PARTNER_ROLES ?? "partner"),
      permissions: splitList(process.env.WEBSITE_PARTNER_PERMISSIONS ?? "")
    },
    {
      sub: process.env.WEBSITE_TEAM_USER ?? "michael.welsch@metric-space.ai",
      email: process.env.WEBSITE_TEAM_USER ?? "michael.welsch@metric-space.ai",
      name: "Michael Welsch",
      password: process.env.WEBSITE_TEAM_PASSWORD ?? process.env.CTOX_BUSINESS_PASSWORD ?? "ctox-business",
      roles: splitList(process.env.WEBSITE_TEAM_ROLES ?? "owner,business_os_admin,business_os_user"),
      permissions: splitList(process.env.WEBSITE_TEAM_PERMISSIONS ?? "business_os:admin,business_os:access")
    }
  ];
}

function parseConfiguredUsers(value?: string) {
  if (!value?.trim()) return [];
  return value.split(";").map((entry) => {
    const separator = entry.includes("|") ? "|" : ":";
    const [subject = "", password = "", roles = "", permissions = "", name = ""] = entry.split(separator).map((part) => part.trim());
    if (!subject || !password) return null;
    return {
      sub: subject,
      email: subject.includes("@") ? subject : undefined,
      name: name || subject,
      password,
      roles: splitList(roles || "customer"),
      permissions: splitList(permissions)
    } satisfies AuthUser;
  }).filter((entry) => entry !== null);
}

function normalizeUser(value?: string) {
  return String(value ?? "").trim().toLowerCase();
}

function splitList(value?: string) {
  return String(value ?? "").split(",").map((item) => item.trim()).filter(Boolean);
}
