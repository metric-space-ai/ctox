export type UnifiedIdentity = {
  subject: string;
  email?: string;
  name?: string;
  roles: string[];
  permissions: string[];
};

type AuthUser = UnifiedIdentity & {
  password: string;
};

const defaultPassword = process.env.CTOX_BUSINESS_PASSWORD ?? "ctox-business";

export function resolveUnifiedIdentity(user: string, password: string) {
  const normalizedUser = normalizeUser(user);
  if (!normalizedUser || !password) return null;

  return authUsers().find((candidate) => (
    normalizeUser(candidate.subject) === normalizedUser
    || normalizeUser(candidate.email) === normalizedUser
  ) && candidate.password === password) ?? null;
}

export function hasBusinessOsAccess(identity: Pick<UnifiedIdentity, "roles" | "permissions">) {
  const roles = new Set(identity.roles);
  const permissions = new Set(identity.permissions);
  return (
    roles.has("owner")
    || roles.has("business_os_user")
    || roles.has("business_os_admin")
    || permissions.has("business_os:access")
    || permissions.has("business_os:admin")
  );
}

export function encodeBusinessSession(identity: UnifiedIdentity) {
  return encodeURIComponent(JSON.stringify({
    sub: identity.subject,
    email: identity.email,
    name: identity.name,
    roles: identity.roles,
    permissions: identity.permissions
  }));
}

export function decodeBusinessSession(value: string) {
  try {
    const parsed = JSON.parse(decodeSessionValue(value)) as Partial<UnifiedIdentity> & { sub?: string };
    const subject = typeof parsed.sub === "string" ? parsed.sub : typeof parsed.subject === "string" ? parsed.subject : "";
    if (!subject) return null;
    return {
      subject,
      email: typeof parsed.email === "string" ? parsed.email : undefined,
      name: typeof parsed.name === "string" ? parsed.name : undefined,
      roles: Array.isArray(parsed.roles) ? parsed.roles.filter(isString) : [],
      permissions: Array.isArray(parsed.permissions) ? parsed.permissions.filter(isString) : []
    } satisfies UnifiedIdentity;
  } catch {
    return {
      subject: decodeSessionValue(value),
      roles: ["business_os_admin"],
      permissions: ["business_os:admin"]
    } satisfies UnifiedIdentity;
  }
}

function decodeSessionValue(value: string) {
  let current = value;
  for (let index = 0; index < 3; index += 1) {
    try {
      const decoded = decodeURIComponent(current);
      if (decoded === current) return decoded;
      current = decoded;
    } catch {
      return current;
    }
  }
  return current;
}

function authUsers(): AuthUser[] {
  const configured = parseConfiguredUsers(process.env.CTOX_AUTH_USERS);
  if (configured.length > 0) return configured;

  const legacyUser = process.env.CTOX_BUSINESS_USER ?? "admin";
  return [
    {
      subject: legacyUser,
      name: legacyUser,
      password: defaultPassword,
      roles: ["owner", "business_os_admin", "business_os_user"],
      permissions: ["business_os:admin", "business_os:access"]
    },
    {
      subject: "michael.welsch@metric-space.ai",
      email: "michael.welsch@metric-space.ai",
      name: "Michael Welsch",
      password: defaultPassword,
      roles: ["owner", "business_os_admin", "business_os_user"],
      permissions: ["business_os:admin", "business_os:access"]
    }
  ];
}

function parseConfiguredUsers(value?: string) {
  if (!value?.trim()) return [];
  const jsonUsers = parseJsonUsers(value);
  if (jsonUsers.length > 0) return jsonUsers;

  return value.split(";").map((entry) => {
    const separator = entry.includes("|") ? "|" : ":";
    const [subject = "", password = "", roles = "", permissions = "", name = ""] = entry.split(separator).map((part) => part.trim());
    if (!subject || !password) return null;
    return {
      subject,
      email: subject.includes("@") ? subject : undefined,
      name: name || subject,
      password,
      roles: splitList(roles || "customer"),
      permissions: splitList(permissions)
    } satisfies AuthUser;
  }).filter((entry) => entry !== null);
}

function parseJsonUsers(value: string) {
  try {
    const parsed = JSON.parse(value) as Array<Record<string, unknown>>;
    if (!Array.isArray(parsed)) return [];
    return parsed.map((entry) => {
      const subject = stringValue(entry.subject) ?? stringValue(entry.email) ?? stringValue(entry.user);
      const password = stringValue(entry.password);
      if (!subject || !password) return null;
      return {
        subject,
        email: stringValue(entry.email) ?? (subject.includes("@") ? subject : undefined),
        name: stringValue(entry.name) ?? subject,
        password,
        roles: arrayValue(entry.roles),
        permissions: arrayValue(entry.permissions)
      } satisfies AuthUser;
    }).filter((entry) => entry !== null);
  } catch {
    return [];
  }
}

function normalizeUser(value?: string) {
  return String(value ?? "").trim().toLowerCase();
}

function splitList(value?: string) {
  return String(value ?? "").split(",").map((item) => item.trim()).filter(Boolean);
}

function arrayValue(value: unknown) {
  if (Array.isArray(value)) return value.filter(isString);
  if (typeof value === "string") return splitList(value);
  return [];
}

function stringValue(value: unknown) {
  return typeof value === "string" ? value : undefined;
}

function isString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}
