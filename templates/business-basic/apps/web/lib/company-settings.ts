export const companyNameCookieName = "ctox_company_name";
export const sessionCookieName = "ctox_business_session";

const defaultCompanyName = "CTOX";

export function normalizeCompanyName(value?: string | null) {
  const normalized = String(value ?? "")
    .replace(/[\u0000-\u001f\u007f]/g, "")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 80);
  return normalized || defaultCompanyName;
}

export function businessOsName(companyName?: string | null) {
  return `${normalizeCompanyName(companyName)} Business OS`;
}
