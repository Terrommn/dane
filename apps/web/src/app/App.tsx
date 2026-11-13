import { QueryClientProvider } from "@tanstack/react-query";
import { lazy, useEffect } from "react";
import { BrowserRouter, Navigate, Route, Routes, useLocation, useNavigate } from "react-router";
import { Button } from "../components/ui/Button";
import { ErrorState, LoadingState } from "../components/ui/States";
import { ToastProvider } from "../components/ui/Toast";
import { AuthProvider, useAuth } from "../features/auth/AuthProvider";
import { LoginPage } from "../features/auth/LoginPage";
import { TodayPage } from "../features/daily/TodayPage";
import { ModulePending } from "../features/more/ModulePending";
import { MorePage } from "../features/more/MorePage";
import { AppShell } from "../layout/AppShell";
import { startTargetPath } from "../lib/telegram";
import { queryClient } from "./queryClient";

// Pantallas secundarias cargadas bajo demanda (el app shell y Hoy quedan en el bundle inicial).
const ProjectsPage = lazy(() =>
  import("../features/projects/ProjectsPage").then((m) => ({ default: m.ProjectsPage })),
);
const ProjectDetailPage = lazy(() =>
  import("../features/projects/ProjectDetailPage").then((m) => ({ default: m.ProjectDetailPage })),
);
const SettingsPage = lazy(() =>
  import("../features/settings/SettingsPage").then((m) => ({ default: m.SettingsPage })),
);
const OnboardingPage = lazy(() =>
  import("../features/settings/OnboardingPage").then((m) => ({ default: m.OnboardingPage })),
);
const SyncPage = lazy(() => import("../features/settings/SyncPage").then((m) => ({ default: m.SyncPage })));
const InstallPage = lazy(() =>
  import("../features/install/InstallPage").then((m) => ({ default: m.InstallPage })),
);

/** Pantallas registradas cuyo módulo se completa en la fase 2 (ver ModulePending). */
const PENDING: { path: string; title: string; includes: string[] }[] = [
  {
    path: "/objetivos",
    title: "Objetivos",
    includes: [
      "Objetivos mensuales y semanales con relación semana → mes",
      "Progreso cuantitativo, ritmo y cierre",
    ],
  },
  {
    path: "/objetivos/planificar",
    title: "Planificar objetivos",
    includes: [
      "Revisión del periodo anterior",
      "Mes antes que semana; 1–3 hábitos por objetivo con avisos de carga",
    ],
  },
  {
    path: "/habitos",
    title: "Hábitos",
    includes: ["Fijos, del mes y de la semana", "Últimos 30 días, heatmap y rachas"],
  },
  {
    path: "/dashboard",
    title: "Dashboard",
    includes: [
      "Objetivos vs avance, hábitos, devocional, peso, energía, paz y finanzas",
      "Comparación con el mismo tramo anterior",
    ],
  },
  {
    path: "/tareas",
    title: "Tareas",
    includes: [
      "Tareas por área/proyecto con arrastre y recurrentes",
      "Tareas externas de ClickUp (solo lectura)",
    ],
  },
  {
    path: "/inbox",
    title: "Inbox",
    includes: ["Captura rápida (también sin conexión)", "Propuestas revisables antes de mover o descartar"],
  },
  {
    path: "/semanal",
    title: "Revisión semanal",
    includes: ["Resumen automático editable, paz 1–5 y tres prioridades", "Cierre de objetivos"],
  },
  {
    path: "/finanzas",
    title: "Finanzas",
    includes: [
      "Dashboard: consumo, deudas, ingresos, flujo, presupuesto y proyección",
      "Cuentas, tarjetas, deudas y plan de crédito",
    ],
  },
  {
    path: "/finanzas/movimientos",
    title: "Movimientos",
    includes: ["Gastos, ingresos, transferencias y pagos", "Filtros, totales e histórico"],
  },
  {
    path: "/personas",
    title: "Personas",
    includes: ["Contactos con alias, cumpleaños e interacciones", "Días sin hablar y sugerencias"],
  },
  {
    path: "/devocionales",
    title: "Devocionales",
    includes: ["Pasaje, versículo/acción, aprendizaje, oración y canción", "Historial y búsqueda"],
  },
  {
    path: "/lecturas",
    title: "Lecturas",
    includes: ["Libros y artículos por estado", "Aporte a objetivos de lectura"],
  },
  {
    path: "/contenido",
    title: "Contenido",
    includes: ["Pipeline idea → publicado, hooks, guion y shot list", "Métricas de publicados"],
  },
  { path: "/swipe", title: "Swipe", includes: ["Referencias con URL, explicación y hook"] },
  { path: "/escuela", title: "Escuela", includes: ["Materias, notas de clase y entregas"] },
  { path: "/dev", title: "Desarrollo", includes: ["Devlog con sesiones, ADRs, runbooks, TILs y snippets"] },
  {
    path: "/notas",
    title: "Notas",
    includes: ["Editor Markdown, tags, alias, enlaces y backlinks", "Favoritos y archivados"],
  },
  { path: "/dibujos", title: "Dibujos", includes: ["Notas visuales con Excalidraw"] },
  {
    path: "/buscar",
    title: "Buscar",
    includes: ["Búsqueda global sin acentos con filtros por módulo, fecha, área y estado"],
  },
  {
    path: "/captura/:id",
    title: "Revisar captura",
    includes: ["Vista previa de lo extraído y confirmación antes de aplicar"],
  },
  {
    path: "/importar",
    title: "Importar / exportar",
    includes: ["Importación del vault con simulación previa", "Exportación JSON/CSV/Markdown"],
  },
];

function RequireAuth({ children }: { children: React.ReactNode }) {
  const { status, error } = useAuth();
  const loc = useLocation();
  if (status === "loading")
    return (
      <div className="mx-auto max-w-md p-6">
        <LoadingState label="Iniciando sesión…" rows={2} />
      </div>
    );
  if (status === "error")
    return (
      <div className="mx-auto max-w-md p-6">
        <ErrorState error={new Error(error ?? "No se pudo iniciar")} title="No se pudo abrir la app" />
        <Button className="mt-3" onClick={() => location.reload()}>
          Reintentar
        </Button>
      </div>
    );
  if (status === "anonymous")
    return <Navigate to={`/login?redirect_to=${encodeURIComponent(loc.pathname + loc.search)}`} replace />;
  return <>{children}</>;
}

/** Al abrir desde un botón del bot con start_param (t.me/...?startapp=checkin_2026-11-11) navega a la pantalla. */
function StartParamRedirect() {
  const { startTarget } = useAuth();
  const navigate = useNavigate();
  const loc = useLocation();
  // biome-ignore lint/correctness/useExhaustiveDependencies: solo debe ejecutarse al resolver el start_param
  useEffect(() => {
    const target = startTargetPath(startTarget);
    if (target && (loc.pathname === "/" || loc.pathname === "/hoy") && !loc.search.includes("date="))
      navigate(target, { replace: true });
  }, [startTarget]);
  return null;
}

function NotFound() {
  return <ModulePending title="Página no encontrada" includes={["Revisa la dirección o vuelve a Hoy"]} />;
}

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <ToastProvider>
        <BrowserRouter>
          <AuthProvider>
            <StartParamRedirect />
            <Routes>
              <Route path="/login" element={<LoginPage />} />
              <Route
                element={
                  <RequireAuth>
                    <AppShell />
                  </RequireAuth>
                }
              >
                <Route index element={<Navigate to="/hoy" replace />} />
                <Route path="/hoy" element={<TodayPage />} />
                <Route path="/mas" element={<MorePage />} />
                <Route path="/proyectos" element={<ProjectsPage />} />
                <Route path="/proyectos/:id" element={<ProjectDetailPage />} />
                <Route path="/ajustes" element={<SettingsPage />} />
                <Route path="/onboarding" element={<OnboardingPage />} />
                <Route path="/sincronizacion" element={<SyncPage />} />
                <Route path="/instalar" element={<InstallPage />} />
                {PENDING.map((p) => (
                  <Route
                    key={p.path}
                    path={p.path}
                    element={<ModulePending title={p.title} includes={p.includes} />}
                  />
                ))}
                <Route path="*" element={<NotFound />} />
              </Route>
            </Routes>
          </AuthProvider>
        </BrowserRouter>
      </ToastProvider>
    </QueryClientProvider>
  );
}
