import { createHmac } from "node:crypto";
import { type Page, expect } from "@playwright/test";

export const BOT_TOKEN = "123456:E2E-TOKEN";

/** Fecha local de hoy en America/Monterrey (zona por defecto del usuario). */
export function todayLocal(): string {
  return new Intl.DateTimeFormat("en-CA", { timeZone: "America/Monterrey" }).format(new Date());
}

export async function devLogin(page: Page, telegramId: number, redirect = "/hoy") {
  await page.goto(`/login?redirect_to=${encodeURIComponent(redirect)}`);
  await page.getByLabel("ID de Telegram (solo desarrollo)").fill(String(telegramId));
  await page.getByRole("button", { name: "Entrar en modo desarrollo" }).click();
  await expect(page).not.toHaveURL(/\/login/);
}

/** initData firmado como lo hace Telegram (HMAC-SHA256 con clave derivada de "WebAppData"). */
export function signedInitData(userId: number, startParam?: string): string {
  const fields: [string, string][] = [
    ["auth_date", String(Math.floor(Date.now() / 1000))],
    ["query_id", "AAE2E"],
    ["user", JSON.stringify({ id: userId, first_name: "Neil", language_code: "es" })],
  ];
  if (startParam) fields.push(["start_param", startParam]);
  const check = [...fields].sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => `${k}=${v}`).join("\n");
  const secret = createHmac("sha256", "WebAppData").update(BOT_TOKEN).digest();
  const hash = createHmac("sha256", secret).update(check).digest("hex");
  const p = new URLSearchParams(fields);
  p.append("hash", hash);
  return p.toString();
}

/** Stub del script oficial: expone Telegram.WebApp con el initData del hash de la URL. */
export const TELEGRAM_STUB = `
(() => {
  const h = new URLSearchParams(location.hash.slice(1));
  const initData = h.get("tgWebAppData") || "";
  const noop = () => {};
  const btn = { show: noop, hide: noop, onClick: noop, offClick: noop, setText: noop, enable: noop, disable: noop, showProgress: noop, hideProgress: noop, isVisible: false };
  window.Telegram = { WebApp: {
    initData, initDataUnsafe: Object.fromEntries(new URLSearchParams(initData)), version: "8.0", platform: "android",
    colorScheme: "dark", themeParams: { bg_color: "#17212b", text_color: "#f5f5f5", button_color: "#5288c1", button_text_color: "#ffffff", hint_color: "#708499", secondary_bg_color: "#232e3c" },
    isVersionAtLeast: (v) => parseFloat(v) <= 8.0, ready: noop, expand: noop, close: noop, onEvent: noop, offEvent: noop,
    BackButton: { ...btn }, MainButton: { ...btn }, SecondaryButton: { ...btn }, HapticFeedback: { impactOccurred: noop, notificationOccurred: noop, selectionChanged: noop },
    safeAreaInset: { top: 0, bottom: 24, left: 0, right: 0 }, contentSafeAreaInset: { top: 48, bottom: 0, left: 0, right: 0 },
    viewportHeight: 700, viewportStableHeight: 700, isExpanded: true, setHeaderColor: noop, setBackgroundColor: noop, enableClosingConfirmation: noop, disableClosingConfirmation: noop,
  } };
})();`;

/** La página no debe tener scroll horizontal accidental. */
export async function expectNoHorizontalOverflow(page: Page, route: string) {
  const { sw, iw } = await page.evaluate(() => ({ sw: document.documentElement.scrollWidth, iw: window.innerWidth }));
  expect(sw, `scroll horizontal en ${route} (${sw}px > ${iw}px)`).toBeLessThanOrEqual(iw + 1);
}
