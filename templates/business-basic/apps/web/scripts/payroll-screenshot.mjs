/**
 * Take a real screenshot of the Payroll workbench so I can look at the rendered page myself,
 * not just assert DOM substrings. Saves to /tmp/payroll-workbench.png.
 */
import puppeteer from "puppeteer-core";
import { writeFile } from "node:fs/promises";

const baseUrl = process.env.CTOX_BUSINESS_BASE_URL ?? "http://localhost:3001";
const chromePath = process.env.CTOX_BUSINESS_CHROME_PATH ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const sessionCookie = await loginCookie();
const cookieParts = sessionCookie.split("=");
const cookieName = cookieParts.shift();
const cookieValue = cookieParts.join("=");

const browser = await puppeteer.launch({
  executablePath: chromePath,
  headless: true,
  args: ["--no-sandbox", "--disable-dev-shm-usage"]
});
try {
  const page = await browser.newPage();
  await page.setViewport({ width: 1600, height: 1100, deviceScaleFactor: 2 });
  await page.setCookie({ name: cookieName, value: cookieValue, url: baseUrl, httpOnly: true });
  await page.goto(`${baseUrl}/app/operations/payroll?locale=de&theme=light`, { waitUntil: "domcontentloaded" });
  await page.waitForSelector(`[data-context-module="operations"][data-context-submodule="payroll"]`, { timeout: 15000 });
  // Give the workbench a moment to hydrate.
  await new Promise((r) => setTimeout(r, 400));
  const buf = await page.screenshot({ fullPage: true });
  await writeFile("/tmp/payroll-workbench.png", buf);
  console.log("/tmp/payroll-workbench.png");
} finally {
  await browser.close();
}

async function loginCookie() {
  const user = process.env.CTOX_BUSINESS_USER ?? "admin";
  const password = process.env.CTOX_BUSINESS_PASSWORD ?? "ctox-business";
  const response = await fetch(
    `${baseUrl}/api/auth/login?user=${encodeURIComponent(user)}&password=${encodeURIComponent(password)}&next=/app`,
    { redirect: "manual" }
  );
  const cookie = response.headers.get("set-cookie");
  return cookie?.split(";")[0] ?? "";
}
