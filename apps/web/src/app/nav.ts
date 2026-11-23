// Mapa de navegación. En escritorio: barra superior con Inicio y menús por tema (Objetivos, Organizar, Finanzas, Vida,
// Conocimiento); "Sistema" vive en el menú del perfil. En móvil: barra inferior (Inicio, Objetivos, Capturar, Finanzas,
// Más) y el resto en "Más". `module` liga la entrada al interruptor de módulos de las preferencias.

export type NavGroup =
  | "principal"
  | "objetivos"
  | "organizar"
  | "finanzas"
  | "vida"
  | "conocimiento"
  | "sistema";

export interface NavItem {
  to: string;
  label: string;
  icon: string;
  group: NavGroup;
  module?: "devotional" | "finance" | "content" | "school" | "dev" | "readings" | "people";
  /** Texto corto bajo la etiqueta en los menús desplegables: explica para qué sirve la pantalla. */
  hint?: string;
}

/** Inicio ES el dashboard: por eso ya no hay una pestaña "Dashboard" aparte (sus métricas completas están en Objetivos). */
export const PRIMARY_NAV: NavItem[] = [{ to: "/", label: "Inicio", icon: "⌂", group: "principal" }];

export const MORE_NAV: NavItem[] = [
  { to: "/objetivos", label: "Objetivos", icon: "🎯", group: "objetivos", hint: "Del mes y de la semana" },
  {
    to: "/objetivos/planificar",
    label: "Planificar",
    icon: "🧭",
    group: "objetivos",
    hint: "Define tus metas del periodo",
  },
  {
    to: "/dashboard",
    label: "Estadísticas",
    icon: "📊",
    group: "objetivos",
    hint: "Tendencias y comparaciones",
  },
  {
    to: "/hoy",
    label: "Check-in del día",
    icon: "☀️",
    group: "organizar",
    hint: "Formulario completo de hoy",
  },
  { to: "/habitos", label: "Hábitos", icon: "✅", group: "organizar" },
  { to: "/tareas", label: "Tareas", icon: "☑️", group: "organizar" },
  { to: "/inbox", label: "Inbox", icon: "📥", group: "organizar", hint: "Lo que capturaste sin ordenar" },
  { to: "/agenda", label: "Agenda", icon: "📅", group: "organizar" },
  { to: "/semanal", label: "Revisión semanal", icon: "🗓️", group: "organizar" },
  { to: "/proyectos", label: "Áreas y proyectos", icon: "🗂️", group: "organizar" },
  { to: "/finanzas", label: "Resumen", icon: "💰", group: "finanzas", module: "finance" },
  { to: "/finanzas/movimientos", label: "Movimientos", icon: "🧾", group: "finanzas", module: "finance" },
  { to: "/personas", label: "Personas", icon: "👥", group: "vida", module: "people" },
  { to: "/devocionales", label: "Devocionales", icon: "🙏", group: "vida", module: "devotional" },
  { to: "/lecturas", label: "Lecturas", icon: "📚", group: "vida", module: "readings" },
  { to: "/contenido", label: "Contenido", icon: "🎬", group: "conocimiento", module: "content" },
  { to: "/swipe", label: "Swipe", icon: "🔖", group: "conocimiento", module: "content" },
  { to: "/escuela", label: "Escuela", icon: "🎓", group: "conocimiento", module: "school" },
  { to: "/dev", label: "Desarrollo", icon: "💻", group: "conocimiento", module: "dev" },
  { to: "/notas", label: "Notas", icon: "📝", group: "conocimiento" },
  { to: "/dibujos", label: "Dibujos", icon: "✏️", group: "conocimiento" },
  { to: "/buscar", label: "Buscar", icon: "🔎", group: "sistema" },
  { to: "/vistas", label: "Vistas", icon: "🗃️", group: "sistema" },
  { to: "/captura/nueva", label: "Captura libre", icon: "✍️", group: "sistema" },
  { to: "/ajustes/integraciones", label: "Integraciones", icon: "🔌", group: "sistema" },
  { to: "/importar", label: "Importar / exportar", icon: "⇅", group: "sistema" },
  { to: "/sincronizacion", label: "Sincronización", icon: "⟳", group: "sistema" },
  { to: "/instalar", label: "Instalar app", icon: "📲", group: "sistema" },
  { to: "/ajustes", label: "Ajustes", icon: "⚙️", group: "sistema" },
];

export const GROUP_LABEL: Record<NavGroup, string> = {
  principal: "Principal",
  objetivos: "Objetivos",
  organizar: "Organizar",
  finanzas: "Finanzas",
  vida: "Vida",
  conocimiento: "Conocimiento",
  sistema: "Sistema",
};

/** Menús de la barra superior, en orden. "Sistema" no aparece: va en el menú del perfil para no saturar la barra. */
export const TOP_GROUPS: NavGroup[] = ["objetivos", "organizar", "finanzas", "vida", "conocimiento"];

/** Filtra módulos desactivados en las preferencias (`modules`). Ausente = activo. */
export function visibleItems(
  items: NavItem[],
  modules: Record<string, unknown> | null | undefined,
): NavItem[] {
  return items.filter((i) => !i.module || modules?.[i.module] !== false);
}
