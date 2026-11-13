import { QueryClientProvider } from "@tanstack/react-query";
import { lazy, useEffect } from "react";
import { BrowserRouter, Navigate, Route, Routes, useLocation, useNavigate } from "react-router";
import { Button } from "../components/ui/Button";
import { ErrorState, LoadingState } from "../components/ui/States";
import { ToastProvider } from "../components/ui/Toast";
import { AuthProvider, useAuth } from "../features/auth/AuthProvider";
import { LoginPage } from "../features/auth/LoginPage";
import { ModulePending } from "../features/more/ModulePending";
import { MorePage } from "../features/more/MorePage";
import { TodayPage } from "../features/today/TodayPage";
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
const FinanceRoutes = lazy(() =>
  import("../features/finance/FinanceRoutes").then((m) => ({ default: m.FinanceRoutes })),
);
const InstallPage = lazy(() =>
  import("../features/install/InstallPage").then((m) => ({ default: m.InstallPage })),
);
// Fase 2 · módulos (F2-modules).
const TasksPage = lazy(() => import("../features/tasks/TasksPage").then((m) => ({ default: m.TasksPage })));
const InboxPage = lazy(() => import("../features/inbox/InboxPage").then((m) => ({ default: m.InboxPage })));
const PeoplePage = lazy(() =>
  import("../features/people/PeoplePage").then((m) => ({ default: m.PeoplePage })),
);
const PersonDetailPage = lazy(() =>
  import("../features/people/PersonDetailPage").then((m) => ({ default: m.PersonDetailPage })),
);
const ContentPage = lazy(() =>
  import("../features/content/ContentPage").then((m) => ({ default: m.ContentPage })),
);
const ContentDetailPage = lazy(() =>
  import("../features/content/ContentDetailPage").then((m) => ({ default: m.ContentDetailPage })),
);
const SwipePage = lazy(() => import("../features/swipe/SwipePage").then((m) => ({ default: m.SwipePage })));
const SchoolPage = lazy(() =>
  import("../features/school/SchoolPage").then((m) => ({ default: m.SchoolPage })),
);
const ClassNotePage = lazy(() =>
  import("../features/school/SchoolPage").then((m) => ({ default: m.ClassNotePage })),
);
const DevPage = lazy(() => import("../features/dev/DevPage").then((m) => ({ default: m.DevPage })));
const AdrPage = lazy(() => import("../features/dev/AdrPage").then((m) => ({ default: m.AdrPage })));
const NotesPage = lazy(() => import("../features/notes/NotesPage").then((m) => ({ default: m.NotesPage })));
const NoteDetailPage = lazy(() =>
  import("../features/notes/NoteDetailPage").then((m) => ({ default: m.NoteDetailPage })),
);
const SearchPage = lazy(() =>
  import("../features/search/SearchPage").then((m) => ({ default: m.SearchPage })),
);
const DrawingsPage = lazy(() =>
  import("../features/drawings/DrawingsPage").then((m) => ({ default: m.DrawingsPage })),
);
const DrawingPage = lazy(() =>
  import("../features/drawings/DrawingsPage").then((m) => ({ default: m.DrawingPage })),
);
const AgendaPage = lazy(() =>
  import("../features/agenda/AgendaPage").then((m) => ({ default: m.AgendaPage })),
);
const ViewsPage = lazy(() => import("../features/views/ViewsPage").then((m) => ({ default: m.ViewsPage })));

// Núcleo (fase 2): objetivos, hábitos, dashboard, semanal, devocionales y lecturas.
const GoalsPage = lazy(() => import("../features/goals/GoalsPage").then((m) => ({ default: m.GoalsPage })));
const GoalDetailPage = lazy(() =>
  import("../features/goals/GoalDetailPage").then((m) => ({ default: m.GoalDetailPage })),
);
const PlanningPage = lazy(() =>
  import("../features/goals/PlanningPage").then((m) => ({ default: m.PlanningPage })),
);
const HabitsPage = lazy(() =>
  import("../features/habits/HabitsPage").then((m) => ({ default: m.HabitsPage })),
);
const HabitDetailPage = lazy(() =>
  import("../features/habits/HabitDetailPage").then((m) => ({ default: m.HabitDetailPage })),
);
const DashboardPage = lazy(() =>
  import("../features/dashboard/DashboardPage").then((m) => ({ default: m.DashboardPage })),
);
const WeeklyPage = lazy(() =>
  import("../features/weekly/WeeklyPage").then((m) => ({ default: m.WeeklyPage })),
);
const DevotionalsPage = lazy(() =>
  import("../features/devotionals/DevotionalsPage").then((m) => ({ default: m.DevotionalsPage })),
);
const DevotionalEditorPage = lazy(() =>
  import("../features/devotionals/DevotionalsPage").then((m) => ({ default: m.DevotionalEditorPage })),
);
const ReadingsPage = lazy(() =>
  import("../features/readings/ReadingsPage").then((m) => ({ default: m.ReadingsPage })),
);
const ReadingEditorPage = lazy(() =>
  import("../features/readings/ReadingsPage").then((m) => ({ default: m.ReadingEditorPage })),
);

/** Pantallas registradas cuyo módulo se completa en la fase 2 (ver ModulePending). */
const PENDING: { path: string; title: string; includes: string[] }[] = [
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
                <Route path="/objetivos" element={<GoalsPage />} />
                <Route path="/objetivos/planificar" element={<PlanningPage />} />
                <Route path="/objetivos/:id" element={<GoalDetailPage />} />
                <Route path="/habitos" element={<HabitsPage />} />
                <Route path="/habitos/:id" element={<HabitDetailPage />} />
                <Route path="/dashboard" element={<DashboardPage />} />
                <Route path="/semanal" element={<WeeklyPage />} />
                <Route path="/devocionales" element={<DevotionalsPage />} />
                <Route path="/devocionales/:date" element={<DevotionalEditorPage />} />
                <Route path="/lecturas" element={<ReadingsPage />} />
                <Route path="/lecturas/:id" element={<ReadingEditorPage />} />
                <Route path="/mas" element={<MorePage />} />
                <Route path="/proyectos" element={<ProjectsPage />} />
                <Route path="/proyectos/:id" element={<ProjectDetailPage />} />
                <Route path="/ajustes" element={<SettingsPage />} />
                <Route path="/onboarding" element={<OnboardingPage />} />
                <Route path="/sincronizacion" element={<SyncPage />} />
                <Route path="/instalar" element={<InstallPage />} />
                <Route path="/finanzas/*" element={<FinanceRoutes />} />
                <Route path="/tareas" element={<TasksPage />} />
                <Route path="/inbox" element={<InboxPage />} />
                <Route path="/personas" element={<PeoplePage />} />
                <Route path="/personas/:id" element={<PersonDetailPage />} />
                <Route path="/contenido" element={<ContentPage />} />
                <Route path="/contenido/:id" element={<ContentDetailPage />} />
                <Route path="/swipe" element={<SwipePage />} />
                <Route path="/escuela" element={<SchoolPage />} />
                <Route path="/escuela/clase/:id" element={<ClassNotePage />} />
                <Route path="/dev" element={<DevPage />} />
                <Route path="/dev/adr/:id" element={<AdrPage />} />
                <Route path="/notas" element={<NotesPage />} />
                <Route path="/notas/:id" element={<NoteDetailPage />} />
                <Route path="/buscar" element={<SearchPage />} />
                <Route path="/dibujos" element={<DrawingsPage />} />
                <Route path="/dibujos/:id" element={<DrawingPage />} />
                <Route path="/agenda" element={<AgendaPage />} />
                <Route path="/vistas" element={<ViewsPage />} />
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
