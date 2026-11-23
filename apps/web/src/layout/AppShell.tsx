// Shell mobile-first: barra inferior (Hoy, Objetivos, Dashboard, Más) en móvil; barra lateral flotante y ocultable en escritorio.
// En Telegram usa el BackButton nativo (si la versión lo soporta) y respeta las safe areas.
import { Suspense, useCallback, useEffect, useState } from "react";
import { Link, NavLink, Outlet, useLocation, useNavigate } from "react-router";
import { GROUP_LABEL, MORE_NAV, type NavItem, PRIMARY_NAV, visibleItems } from "../app/nav";
import { LoadingState } from "../components/ui/States";
import { useToast } from "../components/ui/Toast";
import { useAuth } from "../features/auth/AuthProvider";
import { DemoBanner } from "../features/demo/Demo";
import { useSyncQueue } from "../lib/offline/hooks";
import { supports } from "../lib/telegram";
import { hasNavIcon, NavIcon } from "./NavIcons";
import { UpdatePrompt } from "./UpdatePrompt";

const ROOTS = new Set(["/", "/hoy", "/objetivos", "/dashboard", "/mas"]);

function useTelegramBackButton() {
  const { telegram } = useAuth();
  const loc = useLocation();
  const navigate = useNavigate();
  useEffect(() => {
    const bb = telegram?.BackButton;
    if (!telegram || !bb || !supports(telegram, "6.1")) return;
    const onBack = () => navigate(-1);
    if (ROOTS.has(loc.pathname)) bb.hide();
    else {
      bb.show();
      bb.onClick(onBack);
    }
    return () => bb.offClick(onBack);
  }, [telegram, loc.pathname, navigate]);
}

function SyncIndicator() {
  const q = useSyncQueue();
  const n = q.pending + q.conflicts.length + q.errors.length;
  if (!n && q.online) return null;
  const label = !q.online
    ? `Sin conexión${q.pending ? ` · ${q.pending} pendientes` : ""}`
    : q.conflicts.length
      ? `${q.conflicts.length} conflicto(s)`
      : q.errors.length
        ? `${q.errors.length} con error`
        : `${q.pending} pendiente(s) de sincronizar`;
  return (
    <Link
      to="/sincronizacion"
      className={`rounded-full border px-2.5 py-1 text-xs font-medium no-underline ${q.conflicts.length || q.errors.length ? "border-danger text-danger" : "border-warn text-warn"}`}
    >
      {label}
    </Link>
  );
}

// Claves de localStorage: la preferencia de barra es del dispositivo (no del usuario en el servidor), porque en un
// monitor grande conviene abierta y en una laptop pequeña quizá no.
const COLLAPSED_KEY = "dane.sidebar.collapsed";
const CLOSED_GROUPS_KEY = "dane.sidebar.closedGroups";
// Grupos menos usados empiezan cerrados para que la barra no sea una lista larga de 30 entradas.
const DEFAULT_CLOSED: NavItem["group"][] = ["conocimiento", "sistema"];
// Buscar y Captura tienen botones propios arriba de la barra; no se repiten en la lista.
const PINNED = new Set(["/buscar", "/captura/nueva"]);

function readStored<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    // Modo privado o JSON corrupto: se usa el valor por defecto en vez de romper la navegación.
    return fallback;
  }
}

function writeStored(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Sin almacenamiento disponible la preferencia solo dura la sesión; no es un error para el usuario.
  }
}

function isEditable(t: EventTarget | null): boolean {
  const el = t as HTMLElement | null;
  return !!el && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName));
}

/** Estado de la barra lateral (abierta/compacta y grupos cerrados), persistido por dispositivo. */
function useSidebarState() {
  const [collapsed, setCollapsed] = useState<boolean>(() => readStored(COLLAPSED_KEY, false));
  const [closed, setClosed] = useState<string[]>(() => readStored(CLOSED_GROUPS_KEY, DEFAULT_CLOSED));
  const toggle = useCallback(() => {
    setCollapsed((c) => {
      writeStored(COLLAPSED_KEY, !c);
      return !c;
    });
  }, []);
  const toggleGroup = (g: string) =>
    setClosed((cur) => {
      const next = cur.includes(g) ? cur.filter((x) => x !== g) : [...cur, g];
      writeStored(CLOSED_GROUPS_KEY, next);
      return next;
    });
  // ⌘B / Ctrl+B como en editores comunes. Se ignora dentro de campos de texto porque ahí Ctrl+B es "negritas"
  // (notas, devocionales) y robarlo sería peor que no tener atajo.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && !e.altKey && e.key.toLowerCase() === "b" && !isEditable(e.target)) {
        e.preventDefault();
        toggle();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggle]);
  return { collapsed, toggle, closed, toggleGroup };
}

function initials(name: string | undefined): string {
  const parts = (name ?? "").trim().split(/\s+/).filter(Boolean);
  const [first = "?", second] = parts;
  return (second ? `${first.charAt(0)}${second.charAt(0)}` : first.slice(0, 2)).toUpperCase();
}

/**
 * Barra lateral flotante de escritorio (tema Medianoche). Un ÚNICO botón la oculta/muestra: al compactarse el botón
 * baja debajo del logo en vez de duplicarse en el encabezado de la página (eso confundía en la maqueta).
 */
function Sidebar({ items }: { items: NavItem[] }) {
  const { me } = useAuth();
  const { collapsed, toggle, closed, toggleGroup } = useSidebarState();
  const list = items.filter((i) => !PINNED.has(i.to));
  const groups = [...new Set(list.map((i) => i.group))];
  // display_name puede venir vacío (p. ej. el usuario de dev-login): se trata igual que ausente.
  const name = me?.user.display_name?.trim() || undefined;
  const pill = "flex h-10 items-center gap-3 rounded-[10px] text-sm no-underline transition-colors";

  return (
    <aside
      aria-label="Barra lateral"
      data-collapsed={collapsed}
      className={`sticky top-3 my-3 ml-3 hidden h-[calc(100dvh-1.5rem)] shrink-0 flex-col overflow-hidden rounded-[18px] border border-sb-line bg-sb text-sb-fg shadow-float transition-[width] duration-300 ease-out md:flex ${collapsed ? "w-[76px]" : "w-64"}`}
    >
      <div
        className={`flex gap-2.5 px-3.5 pb-2 pt-4 ${collapsed ? "flex-col items-center" : "items-center"}`}
      >
        <Link
          to="/hoy"
          aria-label="Dane, ir a Hoy"
          className="grid size-9 shrink-0 place-items-center rounded-[11px] bg-linear-to-br from-accent to-accent-2 text-[17px] font-bold text-accent-fg no-underline"
        >
          D
        </Link>
        {!collapsed && (
          <div className="min-w-0 leading-tight">
            <p className="text-lg font-bold text-fg">Dane</p>
            <p className="truncate text-xs text-sb-muted">Tu segundo cerebro</p>
          </div>
        )}
        <button
          type="button"
          onClick={toggle}
          aria-expanded={!collapsed}
          aria-label={collapsed ? "Mostrar barra lateral" : "Ocultar barra lateral"}
          title={`${collapsed ? "Mostrar" : "Ocultar"} barra (Ctrl/⌘ + B)`}
          className={`grid size-8 shrink-0 cursor-pointer place-items-center rounded-[9px] text-sb-muted hover:bg-sb-hover hover:text-sb-fg ${collapsed ? "" : "ml-auto"}`}
        >
          <NavIcon name="panel" />
        </button>
      </div>

      <div className="space-y-1.5 px-3 pb-2">
        <Link
          to="/buscar"
          title={collapsed ? "Buscar" : undefined}
          className={`${pill} border border-sb-line bg-sb-hover px-3 text-sb-muted hover:text-sb-fg ${collapsed ? "justify-center px-0" : ""}`}
        >
          <NavIcon name="/buscar" />
          {!collapsed && "Buscar…"}
        </Link>
        <Link
          to="/captura/nueva"
          title={collapsed ? "Captura rápida" : undefined}
          className={`${pill} justify-center bg-linear-to-br from-accent to-accent-2 font-semibold text-accent-fg shadow-[0_4px_14px_rgb(139_124_246/0.3)] hover:opacity-95`}
        >
          <NavIcon name="/captura/nueva" />
          {!collapsed && "Captura rápida"}
        </Link>
      </div>

      <nav
        aria-label="Navegación principal"
        className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden px-3 pb-3"
      >
        {groups.map((g) => {
          // En modo compacto los grupos no se pueden abrir/cerrar (no hay etiqueta visible): se muestran todos.
          const isClosed = !collapsed && g !== "principal" && closed.includes(g);
          return (
            <div key={g}>
              {g !== "principal" &&
                (collapsed ? (
                  <hr className="mx-3 my-2.5 border-sb-line" />
                ) : (
                  <button
                    type="button"
                    onClick={() => toggleGroup(g)}
                    aria-expanded={!isClosed}
                    className="flex w-full cursor-pointer items-center px-2.5 pb-1.5 pt-3.5 text-[11px] font-semibold uppercase tracking-[0.08em] text-sb-muted hover:text-sb-fg"
                  >
                    {GROUP_LABEL[g]}
                    <span className={`ml-auto transition-transform ${isClosed ? "-rotate-90" : ""}`}>
                      <NavIcon name="chevron" className="size-3.5" />
                    </span>
                  </button>
                ))}
              {!isClosed && (
                <ul>
                  {list
                    .filter((i) => i.group === g)
                    .map((i) => (
                      <li key={i.to}>
                        <NavLink
                          to={i.to}
                          end={i.to === "/finanzas"}
                          title={collapsed ? i.label : undefined}
                          aria-label={collapsed ? i.label : undefined}
                          className={({ isActive }) =>
                            `relative my-px ${pill} h-[38px] whitespace-nowrap px-2.5 ${collapsed ? "justify-center px-0" : ""} ${
                              isActive
                                ? "bg-sb-active font-semibold text-sb-active-fg before:absolute before:-left-3 before:top-2 before:bottom-2 before:w-[3px] before:rounded-r before:bg-accent before:content-['']"
                                : "text-sb-fg hover:bg-sb-hover"
                            }`
                          }
                        >
                          {hasNavIcon(i.to) ? (
                            <NavIcon name={i.to} />
                          ) : (
                            <span aria-hidden className="w-[18px] text-center">
                              {i.icon}
                            </span>
                          )}
                          {!collapsed && <span className="truncate">{i.label}</span>}
                        </NavLink>
                      </li>
                    ))}
                </ul>
              )}
            </div>
          );
        })}
      </nav>

      <div className="border-t border-sb-line p-3">
        <Link
          to="/ajustes"
          title={collapsed ? "Ajustes y perfil" : undefined}
          className={`flex items-center gap-2.5 rounded-[10px] p-2 no-underline hover:bg-sb-hover ${collapsed ? "justify-center" : ""}`}
        >
          <span className="grid size-[34px] shrink-0 place-items-center rounded-full bg-accent-soft text-[13px] font-bold text-link">
            {name ? initials(name) : <NavIcon name="/personas" className="size-4" />}
          </span>
          {!collapsed && (
            <span className="min-w-0 leading-tight">
              <span className="block truncate text-sm font-semibold text-fg">{name ?? "Tu cuenta"}</span>
              <span className="block text-xs text-sb-muted">Ajustes y perfil</span>
            </span>
          )}
        </Link>
      </div>
    </aside>
  );
}

export function AppShell() {
  useTelegramBackButton();
  const { me, notice, isTelegram, offline } = useAuth();
  const toast = useToast();
  const modules = (me?.settings.modules ?? null) as Record<string, unknown> | null;
  const all = [...PRIMARY_NAV, ...visibleItems(MORE_NAV, modules)];

  useEffect(() => {
    if (notice) toast.show(notice, { sticky: true });
  }, [notice, toast]);

  return (
    <div
      className="min-h-screen"
      style={{
        paddingTop: "var(--safe-top)",
        paddingLeft: "var(--safe-left)",
        paddingRight: "var(--safe-right)",
      }}
    >
      <a href="#contenido" className="sr-only-focusable fixed left-2 top-2 z-50 rounded bg-surface p-2">
        Saltar al contenido
      </a>
      <DemoBanner />
      {/* La barra flota pegada al borde izquierdo de la ventana; solo el contenido se limita y centra (max-w-7xl),
          para que en monitores anchos no quede una franja vacía entre la barra y el borde. */}
      <div className="flex">
        <Sidebar items={all} />
        <div className="mx-auto min-w-0 max-w-7xl flex-1">
          <header className="flex items-center justify-between gap-2 px-4 pt-3 md:px-6">
            {!isTelegram && (
              <Link to="/hoy" className="text-lg font-bold text-fg no-underline md:hidden">
                Dane
              </Link>
            )}
            <div className="ml-auto">
              <SyncIndicator />
            </div>
          </header>
          <main id="contenido" className="px-4 pb-28 pt-3 md:px-6 md:pb-10">
            {offline && (
              <p role="status" className="mb-3 rounded-xl border border-warn bg-surface p-3 text-sm">
                Sin conexión: trabajas con los datos guardados en este dispositivo. Se sincronizará al
                reconectar.
              </p>
            )}
            <Suspense fallback={<LoadingState />}>
              <Outlet />
            </Suspense>
          </main>
        </div>
      </div>
      <nav
        aria-label="Navegación inferior"
        className="fixed inset-x-0 bottom-0 z-40 border-t border-line bg-surface md:hidden"
        style={{ paddingBottom: "var(--safe-bottom)" }}
      >
        <ul className="grid grid-cols-4">
          {[...PRIMARY_NAV, { to: "/mas", label: "Más", icon: "☰", group: "principal" as const }].map(
            (i) => (
              <li key={i.to}>
                <NavLink
                  to={i.to}
                  className={({ isActive }) =>
                    `flex min-h-14 flex-col items-center justify-center gap-0.5 text-xs no-underline ${isActive ? "font-semibold text-link" : "text-muted"}`
                  }
                >
                  <span aria-hidden className="text-lg">
                    {i.icon}
                  </span>
                  {i.label}
                </NavLink>
              </li>
            ),
          )}
        </ul>
      </nav>
      <UpdatePrompt />
    </div>
  );
}
