// Adaptador de Telegram Mini App. Solo se activa con contexto válido (initData no vacío); en navegador/PWA no hace nada.
// El script oficial se carga bajo demanda únicamente si la URL trae parámetros de lanzamiento de Telegram.

export interface TgButton {
  show(): void;
  hide(): void;
  onClick(cb: () => void): void;
  offClick(cb: () => void): void;
  setText?(text: string): void;
  enable?(): void;
  disable?(): void;
  isVisible?: boolean;
}

export interface TgInsets {
  top: number;
  bottom: number;
  left: number;
  right: number;
}

export interface TgWebApp {
  initData: string;
  version: string;
  platform: string;
  colorScheme?: "light" | "dark";
  themeParams: Record<string, string | undefined>;
  safeAreaInset?: TgInsets;
  contentSafeAreaInset?: TgInsets;
  initDataUnsafe?: { start_param?: string };
  BackButton?: TgButton;
  MainButton?: TgButton;
  ready(): void;
  expand(): void;
  isVersionAtLeast?(v: string): boolean;
  onEvent?(event: string, cb: () => void): void;
  offEvent?(event: string, cb: () => void): void;
  openTelegramLink?(url: string): void;
}

declare global {
  interface Window {
    Telegram?: { WebApp?: TgWebApp };
  }
}

const SCRIPT_URL = "https://telegram.org/js/telegram-web-app.js";

/** Telegram abre la Mini App con `#tgWebAppData=…` (y a veces `?tgWebAppStartParam=`). */
export function isTelegramLaunch(loc: Pick<Location, "hash" | "search"> = location): boolean {
  return loc.hash.includes("tgWebAppData") || loc.search.includes("tgWebAppStartParam");
}

export function loadTelegramScript(timeoutMs = 5000): Promise<TgWebApp | null> {
  if (window.Telegram?.WebApp) return Promise.resolve(validWebApp());
  return new Promise((resolve) => {
    const s = document.createElement("script");
    s.src = SCRIPT_URL;
    s.async = true;
    const timer = setTimeout(() => resolve(validWebApp()), timeoutMs);
    s.onload = () => {
      clearTimeout(timer);
      resolve(validWebApp());
    };
    s.onerror = () => {
      clearTimeout(timer);
      resolve(null);
    };
    document.head.appendChild(s);
  });
}

/** WebApp solo si hay initData firmado (no se usa initDataUnsafe para autenticar). */
export function validWebApp(): TgWebApp | null {
  const wa = window.Telegram?.WebApp;
  return wa && typeof wa.initData === "string" && wa.initData.length > 0 ? wa : null;
}

export function supports(wa: TgWebApp, version: string): boolean {
  return typeof wa.isVersionAtLeast === "function" ? wa.isVersionAtLeast(version) : false;
}

const DATE_RE = /^(\d{4})-(\d{2})-(\d{2})$/;

export function isValidIsoDate(s: string | null | undefined): s is string {
  if (!s) return false;
  const m = DATE_RE.exec(s);
  if (!m) return false;
  const d = new Date(Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3])));
  return (
    d.getUTCFullYear() === Number(m[1]) &&
    d.getUTCMonth() === Number(m[2]) - 1 &&
    d.getUTCDate() === Number(m[3])
  );
}

export interface StartTarget {
  screen: "checkin" | "objetivos" | "dashboard" | "semanal" | "finanzas" | "captura" | "ajustes" | "hoy";
  date: string | null;
  /** id o semana según la pantalla. */
  ref: string | null;
}

/**
 * `start_param` de enlaces t.me (`checkin_2026-11-12`, `semanal_2026-W46`, `captura_<uuid>`, `dashboard`).
 * Telegram solo admite `[A-Za-z0-9_-]` en start_param.
 */
export function parseStartParam(sp: string | null | undefined): StartTarget | null {
  if (!sp) return null;
  const [screen, ...rest] = sp.split("_");
  const arg = rest.join("_") || null;
  switch (screen) {
    case "checkin":
    case "hoy":
      return { screen: "checkin", date: isValidIsoDate(arg) ? arg : null, ref: null };
    case "semanal":
      return { screen: "semanal", date: null, ref: arg && /^\d{4}-W\d{2}$/.test(arg) ? arg : null };
    case "captura":
      return { screen: "captura", date: null, ref: arg && /^[0-9a-f-]{36}$/i.test(arg) ? arg : null };
    case "objetivos":
    case "dashboard":
    case "finanzas":
    case "ajustes":
      return { screen, date: null, ref: null };
    default:
      return null;
  }
}

/** Ruta interna correspondiente al start_param (o null si no aplica). */
export function startTargetPath(t: StartTarget | null): string | null {
  if (!t) return null;
  switch (t.screen) {
    case "checkin":
    case "hoy":
      return t.date ? `/hoy?date=${t.date}` : "/hoy";
    case "semanal":
      return t.ref ? `/semanal?week=${t.ref}` : "/semanal";
    case "captura":
      return t.ref ? `/captura/${t.ref}` : "/inbox";
    default:
      return `/${t.screen}`;
  }
}

/** Fecha objetivo: `?date=` de la URL tiene prioridad; luego el start_param. Valida el formato. */
export function resolveTargetDate(search: string, startParam: string | null | undefined): string | null {
  const q = new URLSearchParams(search).get("date");
  if (isValidIsoDate(q)) return q;
  const t = parseStartParam(startParam);
  return t?.screen === "checkin" ? t.date : null;
}

const THEME_MAP: Record<string, string> = {
  bg_color: "--tg-bg",
  secondary_bg_color: "--tg-bg-2",
  text_color: "--tg-text",
  hint_color: "--tg-hint",
  link_color: "--tg-link",
  button_color: "--tg-button",
  button_text_color: "--tg-button-text",
  destructive_text_color: "--tg-danger",
  section_bg_color: "--tg-section",
};

export function applyTelegramTheme(wa: TgWebApp, root: HTMLElement = document.documentElement) {
  for (const [k, v] of Object.entries(THEME_MAP)) {
    const color = wa.themeParams?.[k];
    if (color) root.style.setProperty(v, color);
  }
  root.dataset.telegram = "true";
  root.dataset.theme = wa.colorScheme ?? "light";
}

export function applySafeAreas(wa: TgWebApp, root: HTMLElement = document.documentElement) {
  const s = wa.safeAreaInset;
  const c = wa.contentSafeAreaInset;
  const top = (s?.top ?? 0) + (c?.top ?? 0);
  const bottom = (s?.bottom ?? 0) + (c?.bottom ?? 0);
  root.style.setProperty("--safe-top", `${top}px`);
  root.style.setProperty("--safe-bottom", `${bottom}px`);
  root.style.setProperty("--safe-left", `${(s?.left ?? 0) + (c?.left ?? 0)}px`);
  root.style.setProperty("--safe-right", `${(s?.right ?? 0) + (c?.right ?? 0)}px`);
}

/** Inicializa la Mini App: ready/expand, tema y safe areas (con suscripción a cambios si la versión lo soporta). */
export function initTelegramUi(wa: TgWebApp) {
  wa.ready();
  wa.expand();
  applyTelegramTheme(wa);
  applySafeAreas(wa);
  if (wa.onEvent) {
    wa.onEvent("themeChanged", () => applyTelegramTheme(wa));
    if (supports(wa, "8.0")) {
      wa.onEvent("safeAreaChanged", () => applySafeAreas(wa));
      wa.onEvent("contentSafeAreaChanged", () => applySafeAreas(wa));
    }
  }
}

let current: TgWebApp | null = null;
export function setTelegram(wa: TgWebApp | null) {
  current = wa;
}
export function getTelegram(): TgWebApp | null {
  return current;
}
