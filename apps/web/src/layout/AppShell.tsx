// Shell mobile-first: barra inferior (Hoy, Objetivos, Dashboard, Más) en móvil; barra lateral en escritorio.
// En Telegram usa el BackButton nativo (si la versión lo soporta) y respeta las safe areas.
import { Suspense, useEffect } from "react";
import { Link, NavLink, Outlet, useLocation, useNavigate } from "react-router";
import { GROUP_LABEL, MORE_NAV, type NavItem, PRIMARY_NAV, visibleItems } from "../app/nav";
import { LoadingState } from "../components/ui/States";
import { useToast } from "../components/ui/Toast";
import { useAuth } from "../features/auth/AuthProvider";
import { DemoBanner } from "../features/demo/Demo";
import { useSyncQueue } from "../lib/offline/hooks";
import { supports } from "../lib/telegram";
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

function SideNav({ items }: { items: NavItem[] }) {
  const groups = [...new Set(items.map((i) => i.group))];
  return (
    <nav aria-label="Navegación principal" className="space-y-4">
      {groups.map((g) => (
        <div key={g}>
          <p className="px-3 pb-1 text-xs font-semibold uppercase tracking-wide text-muted">
            {GROUP_LABEL[g]}
          </p>
          <ul>
            {items
              .filter((i) => i.group === g)
              .map((i) => (
                <li key={i.to}>
                  <NavLink
                    to={i.to}
                    end={i.to === "/finanzas"}
                    className={({ isActive }) =>
                      `flex items-center gap-2 rounded-lg px-3 py-2 text-fg no-underline ${isActive ? "bg-surface-2 font-semibold" : "hover:bg-surface-2"}`
                    }
                  >
                    <span aria-hidden>{i.icon}</span>
                    {i.label}
                  </NavLink>
                </li>
              ))}
          </ul>
        </div>
      ))}
    </nav>
  );
}

export function AppShell() {
  useTelegramBackButton();
  const { me, notice, isTelegram } = useAuth();
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
      <div className="mx-auto flex max-w-7xl">
        <aside className="sticky top-0 hidden h-screen w-60 shrink-0 overflow-y-auto border-r border-line p-3 md:block">
          <Link to="/hoy" className="mb-4 block px-3 text-xl font-bold text-fg no-underline">
            Vida
          </Link>
          <SideNav items={all} />
        </aside>
        <div className="min-w-0 flex-1">
          <header className="flex items-center justify-between gap-2 px-4 pt-3 md:px-6">
            {!isTelegram && (
              <Link to="/hoy" className="text-lg font-bold text-fg no-underline md:hidden">
                Vida
              </Link>
            )}
            <div className="ml-auto">
              <SyncIndicator />
            </div>
          </header>
          <main id="contenido" className="px-4 pb-28 pt-3 md:px-6 md:pb-10">
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
