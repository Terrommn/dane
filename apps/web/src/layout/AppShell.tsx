// Shell mobile-first. Escritorio: barra lateral con Inicio y una sección colapsable por tema (se puede reducir a un
// riel de iconos) y un encabezado con acciones rápidas. Móvil: barra inferior mínima (Hoy, Capturar, Menú).
// En Telegram usa el BackButton nativo (si la versión lo soporta) y respeta las safe areas.
import { useQuery } from "@tanstack/react-query";
import { Suspense, useEffect, useId, useRef, useState } from "react";
import { Link, NavLink, Outlet, useLocation, useNavigate } from "react-router";
import { GROUP_LABEL, MORE_NAV, type NavGroup, type NavItem, SIDEBAR_GROUPS, visibleItems } from "../app/nav";
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

function ItemIcon({ item }: { item: NavItem }) {
  return hasNavIcon(item.to) ? (
    <NavIcon name={item.to} className="size-[17px]" />
  ) : (
    <span aria-hidden className="w-[17px] text-center text-sm">
      {item.icon}
    </span>
  );
}

// Preferencias de la barra lateral: solo comodidad de este navegador. Si localStorage falla (modo privado, bloqueado)
// se usan los valores por defecto y la barra funciona igual.
const SB_RAIL_KEY = "dane.sidebar.rail";
const SB_CLOSED_KEY = "dane.sidebar.closed";
/** Por defecto "Sistema" arranca cerrado: son pantallas de configuración que se visitan poco. */
const DEFAULT_CLOSED: NavGroup[] = ["sistema"];

function readPref<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

function writePref(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Sin almacenamiento: la preferencia dura solo esta visita.
  }
}

function sidebarLink(rail: boolean) {
  return ({ isActive }: { isActive: boolean }) =>
    `flex items-center gap-3 rounded-xl text-sm no-underline transition-colors ${rail ? "size-11 justify-center" : "px-3 py-2"} ${isActive ? "bg-sb-active font-medium text-sb-active-fg" : "text-sb-fg hover:bg-sb-hover"}`;
}

/**
 * Sección colapsable de la barra lateral (patrón "disclosure": botón con aria-expanded que muestra u oculta la lista).
 * En modo riel no hay encabezados: los iconos de cada grupo se separan con una línea y el nombre sale en el tooltip.
 */
function SidebarGroup({
  group,
  items,
  open,
  rail,
  onToggle,
}: {
  group: NavGroup;
  items: NavItem[];
  open: boolean;
  rail: boolean;
  onToggle: () => void;
}) {
  const loc = useLocation();
  const id = useId();
  const active = items.some((i) => matches(loc.pathname, i.to, items));
  const link = sidebarLink(rail);
  const list = (
    <ul id={id} className={rail ? "flex flex-col items-center gap-1" : "mt-0.5 space-y-0.5"}>
      {items.map((i) => (
        <li key={i.to}>
          <NavLink
            to={i.to}
            end
            className={link}
            title={rail ? `${GROUP_LABEL[group]} · ${i.label}` : i.hint}
            aria-label={rail ? i.label : undefined}
          >
            <span className="shrink-0 text-sb-muted">
              <ItemIcon item={i} />
            </span>
            {!rail && <span className="truncate">{i.label}</span>}
          </NavLink>
        </li>
      ))}
    </ul>
  );
  if (rail) return <li className="border-t border-sb-line pt-2">{list}</li>;
  return (
    <li>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        onClick={onToggle}
        className={`flex w-full cursor-pointer items-center justify-between rounded-lg px-3 py-1.5 text-xs font-semibold uppercase tracking-wider hover:text-sb-fg ${active ? "text-sb-active-fg" : "text-sb-muted"}`}
      >
        {GROUP_LABEL[group]}
        <NavIcon name="chevron" className={`size-3.5 transition-transform ${open ? "rotate-180" : ""}`} />
      </button>
      {open && list}
    </li>
  );
}

/**
 * Barra lateral de escritorio: Inicio y una sección colapsable por tema. Se puede reducir a un riel de iconos para
 * ganar espacio en pantallas medianas; las dos preferencias se recuerdan en este navegador.
 */
function Sidebar({
  items,
  rail,
  onRail,
}: {
  items: NavItem[];
  rail: boolean;
  onRail: (rail: boolean) => void;
}) {
  const loc = useLocation();
  const [closed, setClosed] = useState<NavGroup[]>(() => readPref(SB_CLOSED_KEY, DEFAULT_CLOSED));
  const groups = SIDEBAR_GROUPS.map((g) => ({ group: g, list: items.filter((i) => i.group === g) })).filter(
    (g) => g.list.length > 0,
  );
  const activeGroup = groups.find((g) => g.list.some((i) => matches(loc.pathname, i.to, g.list)))?.group;

  // Si llegas a una pantalla de un grupo cerrado (enlace, búsqueda…), se abre para que veas dónde estás.
  useEffect(() => {
    if (activeGroup && closed.includes(activeGroup)) {
      const next = closed.filter((g) => g !== activeGroup);
      setClosed(next);
      writePref(SB_CLOSED_KEY, next);
    }
  }, [activeGroup, closed]);

  const toggle = (g: NavGroup) => {
    const next = closed.includes(g) ? closed.filter((x) => x !== g) : [...closed, g];
    setClosed(next);
    writePref(SB_CLOSED_KEY, next);
  };

  return (
    <aside
      className={`fixed inset-y-0 left-0 z-30 hidden flex-col border-r border-sb-line bg-sb transition-[width] duration-200 md:flex ${rail ? "w-[72px]" : "w-64"}`}
      style={{ paddingTop: "var(--safe-top)", paddingLeft: "var(--safe-left)" }}
    >
      <div
        className={`flex h-[68px] shrink-0 items-center ${rail ? "justify-center" : "justify-between px-5"}`}
      >
        {!rail && (
          <Link to="/" className="text-[22px] font-semibold tracking-tight text-fg no-underline">
            Dane<span className="text-accent">.</span>
          </Link>
        )}
        <button
          type="button"
          onClick={() => onRail(!rail)}
          aria-label={rail ? "Expandir barra lateral" : "Contraer barra lateral"}
          title={rail ? "Expandir barra lateral" : "Contraer barra lateral"}
          className="grid size-9 cursor-pointer place-items-center rounded-lg text-sb-muted hover:bg-sb-hover hover:text-sb-fg"
        >
          <NavIcon name="panel" className="size-[18px]" />
        </button>
      </div>
      <nav
        aria-label="Navegación principal"
        className={`flex-1 overflow-y-auto pb-6 ${rail ? "px-2" : "px-3"}`}
      >
        <ul className={rail ? "flex flex-col items-center gap-2" : "space-y-3"}>
          <li className={rail ? "" : "pb-1"}>
            <NavLink
              to="/"
              end
              className={sidebarLink(rail)}
              title={rail ? "Inicio" : undefined}
              aria-label={rail ? "Inicio" : undefined}
            >
              <span className="shrink-0 text-sb-muted">
                <NavIcon name="/" className="size-[17px]" />
              </span>
              {!rail && "Inicio"}
            </NavLink>
          </li>
          {groups.map(({ group, list }) => (
            <SidebarGroup
              key={group}
              group={group}
              items={list}
              rail={rail}
              open={!closed.includes(group)}
              onToggle={() => toggle(group)}
            />
          ))}
        </ul>
      </nav>
    </aside>
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

function ProfileMenu() {
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
          <Link
            to="/ajustes"
            className="flex items-center gap-3 rounded-xl border-t border-line px-3 py-2 text-sm text-fg no-underline hover:bg-surface-2"
          >
            <NavIcon name="/ajustes" className="size-[17px]" />
            Ajustes
          </Link>
          <button
            type="button"
            onClick={() => void logout()}
            className="flex w-full cursor-pointer items-center gap-3 rounded-xl px-3 py-2 text-sm text-muted hover:bg-surface-2 hover:text-fg"
          >
            <NavIcon name="logout" className="size-[17px]" />
            Cerrar sesión
          </button>
        </div>
      )}
    </div>
  );
}

/** Encabezado de escritorio: solo acciones rápidas. La navegación vive en la barra lateral. */
function TopBar() {
  const iconBtn =
    "grid size-10 place-items-center rounded-full border border-line text-muted no-underline hover:text-fg";
  return (
    <header className="sticky top-0 z-20 hidden border-b border-line/60 bg-bg/80 backdrop-blur-md md:block">
      <div className="mx-auto flex h-[68px] max-w-7xl items-center justify-end gap-2 px-6">
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
        <ProfileMenu />
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
  const [rail, setRail] = useState(() => readPref(SB_RAIL_KEY, false));
  const onRail = (next: boolean) => {
    setRail(next);
    writePref(SB_RAIL_KEY, next);
  };

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
      <Sidebar items={items} rail={rail} onRail={onRail} />
      {/* Columna de contenido: se desplaza a la derecha lo que mida la barra lateral (solo en escritorio). El aviso de
          demo va aquí dentro para no tapar la parte superior de la barra lateral, que es fija. */}
      <div className={`transition-[padding] duration-200 ${rail ? "md:pl-[72px]" : "md:pl-64"}`}>
        <DemoBanner />
        <TopBar />
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
              Sin conexión: trabajas con los datos guardados en este dispositivo. Se sincronizará al
              reconectar.
            </p>
          )}
          <Suspense fallback={<LoadingState />}>
            <Outlet />
          </Suspense>
        </main>
      </div>
      {/* Durante el check-in guiado se oculta: sus botones Atrás/Siguiente van fijos abajo y competirían con ella. */}
      {!guided && <BottomNav />}
      <UpdatePrompt />
    </div>
  );
}
