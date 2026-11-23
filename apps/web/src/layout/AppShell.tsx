// Shell mobile-first. Escritorio: barra superior con Inicio y menús por tema (rediseño premium "Ámbar"); el menú del
// perfil agrupa lo de Sistema. Móvil: barra inferior mínima (Hoy, Capturar, Menú).
// En Telegram usa el BackButton nativo (si la versión lo soporta) y respeta las safe areas.
import { useQuery } from "@tanstack/react-query";
import { Suspense, useEffect, useId, useRef, useState } from "react";
import { Link, NavLink, Outlet, useLocation, useNavigate } from "react-router";
import { GROUP_LABEL, MORE_NAV, type NavGroup, type NavItem, TOP_GROUPS, visibleItems } from "../app/nav";
import { LoadingState } from "../components/ui/States";
import { useToast } from "../components/ui/Toast";
import { useAuth } from "../features/auth/AuthProvider";
import { DemoBanner } from "../features/demo/Demo";
import { api } from "../lib/api";
import { unwrap } from "../lib/errors";
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

export function initials(name: string | undefined): string {
  const parts = (name ?? "").trim().split(/\s+/).filter(Boolean);
  const [first = "?", second] = parts;
  return (second ? `${first.charAt(0)}${second.charAt(0)}` : first.slice(0, 2)).toUpperCase();
}

/** ¿La ruta actual pertenece a este elemento? `/finanzas` no debe marcarse activo en `/finanzas/movimientos`. */
function matches(pathname: string, to: string, siblings: NavItem[]): boolean {
  if (pathname === to) return true;
  if (!pathname.startsWith(`${to}/`)) return false;
  // Si otro hermano es más específico (p. ej. /objetivos/planificar), gana él.
  return !siblings.some((s) => s.to !== to && s.to.startsWith(`${to}/`) && pathname.startsWith(s.to));
}

/**
 * Menú desplegable accesible (patrón "disclosure": botón con aria-expanded + lista de enlaces). Se cierra con Escape,
 * clic fuera o al navegar. No usamos role="menu" porque son enlaces de navegación, no comandos.
 */
function useDisclosure() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const loc = useLocation();
  // biome-ignore lint/correctness/useExhaustiveDependencies: cerrar al cambiar de ruta es el efecto buscado
  useEffect(() => setOpen(false), [loc.pathname, loc.search]);
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        ref.current?.querySelector<HTMLButtonElement>("button")?.focus();
      }
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);
  return { open, setOpen, ref };
}

const popover =
  "absolute top-[calc(100%+8px)] z-40 min-w-60 rounded-2xl border border-line bg-surface p-1.5 shadow-float";
const tab =
  "flex h-10 cursor-pointer items-center gap-2 rounded-full border px-4 text-sm no-underline transition-colors";

function ItemIcon({ item }: { item: NavItem }) {
  return hasNavIcon(item.to) ? (
    <NavIcon name={item.to} className="size-[17px]" />
  ) : (
    <span aria-hidden className="w-[17px] text-center text-sm">
      {item.icon}
    </span>
  );
}

function GroupMenu({ group, items }: { group: NavGroup; items: NavItem[] }) {
  const { open, setOpen, ref } = useDisclosure();
  const loc = useLocation();
  const id = useId();
  const active = items.some((i) => matches(loc.pathname, i.to, items));
  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen((o) => !o)}
        className={`${tab} ${active ? "border-line bg-surface-2 text-fg" : "border-transparent text-muted hover:text-fg"}`}
      >
        {GROUP_LABEL[group]}
        <NavIcon name="chevron" className={`size-3.5 transition-transform ${open ? "rotate-180" : ""}`} />
      </button>
      {open && (
        <ul id={id} className={`${popover} left-0`}>
          {items.map((i) => (
            <li key={i.to}>
              <NavLink
                to={i.to}
                end
                className={({ isActive }) =>
                  `flex items-start gap-3 rounded-xl px-3 py-2.5 no-underline ${isActive ? "bg-accent-soft text-link" : "text-fg hover:bg-surface-2"}`
                }
              >
                <span className="mt-0.5 text-muted">
                  <ItemIcon item={i} />
                </span>
                <span className="min-w-0">
                  <span className="block text-sm font-medium">{i.label}</span>
                  {i.hint && <span className="block text-xs text-muted">{i.hint}</span>}
                </span>
              </NavLink>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

const REMINDER_LABEL: Record<string, string> = {
  morning: "Mensaje de la mañana",
  daily: "Cierre del día",
  weekly: "Revisión semanal",
  monthly: "Objetivos del mes",
};

/** Campana: próximos mensajes del bot. Es lo que "te va a llegar", útil para confiar en que el asistente funciona. */
function Notifications() {
  const { open, setOpen, ref } = useDisclosure();
  const { me } = useAuth();
  const id = useId();
  const tz = me?.settings.timezone ?? "America/Monterrey";
  const q = useQuery({
    queryKey: ["reminders"],
    queryFn: async () => unwrap(await api.GET("/reminders")),
    enabled: open,
  });
  const fmt = (iso: string) =>
    new Intl.DateTimeFormat("es-MX", {
      weekday: "short",
      hour: "2-digit",
      minute: "2-digit",
      timeZone: tz,
    }).format(new Date(iso));
  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        aria-label="Notificaciones del asistente"
        onClick={() => setOpen((o) => !o)}
        className="grid size-10 cursor-pointer place-items-center rounded-full border border-line text-muted hover:text-fg"
      >
        <NavIcon name="bell" />
      </button>
      {open && (
        <div id={id} className={`${popover} right-0 w-80 p-3`}>
          <p className="text-sm font-semibold">Próximos mensajes en Telegram</p>
          {q.isPending && <p className="mt-2 text-sm text-muted">Cargando…</p>}
          {q.data && !q.data.chat_known && (
            <p className="mt-2 text-sm text-muted">
              Aún no conectas el bot: escríbele <strong>/start</strong> en Telegram.
            </p>
          )}
          {q.data?.paused && <p className="mt-2 text-sm text-warn">Recordatorios en pausa.</p>}
          {q.data && q.data.upcoming.length > 0 && (
            <ul className="mt-2 space-y-1.5">
              {q.data.upcoming.slice(0, 5).map((u) => (
                <li key={`${u.kind}-${u.occurrence_key}`} className="flex justify-between gap-3 text-sm">
                  <span>{REMINDER_LABEL[u.kind] ?? u.kind}</span>
                  <span className="shrink-0 tabular-nums text-muted">{fmt(u.run_at)}</span>
                </li>
              ))}
            </ul>
          )}
          <Link to="/ajustes" className="mt-3 inline-block text-sm">
            Cambiar horarios →
          </Link>
        </div>
      )}
    </div>
  );
}

function ProfileMenu({ system }: { system: NavItem[] }) {
  const { open, setOpen, ref } = useDisclosure();
  const { me, logout } = useAuth();
  const id = useId();
  const name = me?.user.display_name?.trim() || undefined;
  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        aria-label="Perfil y ajustes"
        onClick={() => setOpen((o) => !o)}
        className="grid size-10 cursor-pointer place-items-center rounded-full border border-line bg-linear-to-br from-[#5b4636] to-[#2a211c] text-[13px] font-semibold text-fg"
      >
        {name ? initials(name) : <NavIcon name="/personas" className="size-4" />}
      </button>
      {open && (
        <div id={id} className={`${popover} right-0`}>
          <p className="px-3 pb-2 pt-1.5 text-sm font-semibold">{name ?? "Tu cuenta"}</p>
          <ul className="border-t border-line pt-1">
            {system
              .filter((i) => i.to !== "/buscar" && i.to !== "/captura/nueva")
              .map((i) => (
                <li key={i.to}>
                  <Link
                    to={i.to}
                    className="flex items-center gap-3 rounded-xl px-3 py-2 text-sm text-fg no-underline hover:bg-surface-2"
                  >
                    <span className="text-muted">
                      <ItemIcon item={i} />
                    </span>
                    {i.label}
                  </Link>
                </li>
              ))}
          </ul>
          <button
            type="button"
            onClick={() => void logout()}
            className="mt-1 flex w-full cursor-pointer items-center gap-3 rounded-xl border-t border-line px-3 py-2 text-sm text-muted hover:bg-surface-2 hover:text-fg"
          >
            <NavIcon name="logout" className="size-[17px]" />
            Cerrar sesión
          </button>
        </div>
      )}
    </div>
  );
}

function TopBar({ items }: { items: NavItem[] }) {
  const iconBtn =
    "grid size-10 place-items-center rounded-full border border-line text-muted no-underline hover:text-fg";
  return (
    <header className="sticky top-0 z-30 hidden border-b border-line/60 bg-bg/80 backdrop-blur-md md:block">
      <div className="mx-auto flex max-w-7xl items-center gap-5 px-6 py-3.5">
        <Link to="/" className="text-[22px] font-semibold tracking-tight text-fg no-underline">
          Dane<span className="text-accent">.</span>
        </Link>
        <nav aria-label="Navegación principal" className="flex flex-wrap items-center gap-1">
          <NavLink
            to="/"
            end
            className={({ isActive }) =>
              `${tab} ${isActive ? "border-line bg-surface-2 text-fg" : "border-transparent text-muted hover:text-fg"}`
            }
          >
            <NavIcon name="/" className="size-4" />
            Inicio
          </NavLink>
          {TOP_GROUPS.map((g) => {
            const list = items.filter((i) => i.group === g);
            return list.length ? <GroupMenu key={g} group={g} items={list} /> : null;
          })}
        </nav>
        <div className="ml-auto flex items-center gap-2">
          <SyncIndicator />
          <Link to="/buscar" aria-label="Buscar" title="Buscar" className={iconBtn}>
            <NavIcon name="/buscar" />
          </Link>
          <Link
            to="/captura/nueva"
            aria-label="Captura rápida"
            title="Captura rápida: anota algo y Dane lo ordena"
            className={iconBtn}
          >
            <NavIcon name="/captura/nueva" />
          </Link>
          <Notifications />
          <ProfileMenu system={items.filter((i) => i.group === "sistema")} />
        </div>
      </div>
    </header>
  );
}

/**
 * Barra inferior del teléfono: solo Hoy, Capturar y Menú. Por qué tan corta: en el teléfono se usa Dane para el día
 * (check-in, casillas, capturar algo); todo lo demás, incluido el dashboard completo, vive en "Menú".
 */
function BottomNav() {
  const tab = ({ isActive }: { isActive: boolean }) =>
    `flex min-h-15 flex-col items-center justify-center gap-1 text-[11px] no-underline ${isActive ? "font-semibold text-link" : "text-muted"}`;
  return (
    <nav
      aria-label="Navegación inferior"
      className="fixed inset-x-0 bottom-0 z-40 border-t border-line bg-bg/95 backdrop-blur md:hidden"
      style={{ paddingBottom: "var(--safe-bottom)" }}
    >
      <ul className="mx-auto grid max-w-sm grid-cols-3 items-center">
        <li>
          <NavLink to="/" end className={tab}>
            <NavIcon name="/" className="size-5" />
            Hoy
          </NavLink>
        </li>
        <li className="flex justify-center">
          <NavLink
            to="/captura/nueva"
            aria-label="Capturar"
            className="grid size-12 place-items-center rounded-2xl bg-accent text-accent-fg no-underline shadow-glow"
          >
            <NavIcon name="/captura/nueva" className="size-6" />
          </NavLink>
        </li>
        <li>
          <NavLink to="/mas" className={tab}>
            <NavIcon name="/mas" className="size-5" />
            Menú
          </NavLink>
        </li>
      </ul>
    </nav>
  );
}

export function AppShell() {
  useTelegramBackButton();
  const { me, notice, isTelegram, offline } = useAuth();
  const toast = useToast();
  const modules = (me?.settings.modules ?? null) as Record<string, unknown> | null;
  const items = visibleItems(MORE_NAV, modules);
  const name = me?.user.display_name?.trim() || undefined;
  const loc = useLocation();
  const guided = loc.pathname === "/hoy" && new URLSearchParams(loc.search).has("guiado");

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
      <TopBar items={items} />
      {/* Encabezado móvil: marca + perfil. Dentro de Telegram sobra (Telegram ya muestra el nombre del bot). */}
      {!isTelegram && (
        <header className="flex items-center justify-between gap-2 px-4 pt-4 md:hidden">
          <Link to="/" className="text-xl font-semibold tracking-tight text-fg no-underline">
            Dane<span className="text-accent">.</span>
          </Link>
          <div className="flex items-center gap-2">
            <SyncIndicator />
            <Link
              to="/ajustes"
              aria-label="Perfil y ajustes"
              className="grid size-9 place-items-center rounded-full border border-line bg-linear-to-br from-[#5b4636] to-[#2a211c] text-xs font-semibold text-fg no-underline"
            >
              {name ? initials(name) : <NavIcon name="/personas" className="size-4" />}
            </Link>
          </div>
        </header>
      )}
      <main id="contenido" className="mx-auto max-w-7xl px-4 pb-28 pt-4 md:px-6 md:pb-12 md:pt-6">
        {offline && (
          <p role="status" className="mb-3 rounded-xl border border-warn bg-surface p-3 text-sm">
            Sin conexión: trabajas con los datos guardados en este dispositivo. Se sincronizará al reconectar.
          </p>
        )}
        <Suspense fallback={<LoadingState />}>
          <Outlet />
        </Suspense>
      </main>
      {/* Durante el check-in guiado se oculta: sus botones Atrás/Siguiente van fijos abajo y competirían con ella. */}
      {!guided && <BottomNav />}
      <UpdatePrompt />
    </div>
  );
}
