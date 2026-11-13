// Mapa de navegación. Principal: Hoy, Objetivos, Dashboard, Más. El resto se alcanza desde "Más" (o la barra lateral
// en escritorio). `module` liga la entrada al interruptor de módulos de las preferencias.

export interface NavItem {
  to: string;
  label: string;
  icon: string;
  group: "principal" | "organizar" | "finanzas" | "vida" | "conocimiento" | "sistema";
  module?: "devotional" | "finance" | "content" | "school" | "dev" | "readings" | "people";
}

export const PRIMARY_NAV: NavItem[] = [
  { to: "/hoy", label: "Hoy", icon: "☀️", group: "principal" },
  { to: "/objetivos", label: "Objetivos", icon: "🎯", group: "principal" },
  { to: "/dashboard", label: "Dashboard", icon: "📊", group: "principal" },
];

export const MORE_NAV: NavItem[] = [
  { to: "/habitos", label: "Hábitos", icon: "✅", group: "organizar" },
  { to: "/tareas", label: "Tareas", icon: "☑️", group: "organizar" },
  { to: "/inbox", label: "Inbox", icon: "📥", group: "organizar" },
  { to: "/semanal", label: "Revisión semanal", icon: "🗓️", group: "organizar" },
  { to: "/proyectos", label: "Áreas y proyectos", icon: "🗂️", group: "organizar" },
  { to: "/finanzas", label: "Finanzas", icon: "💰", group: "finanzas", module: "finance" },
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
  { to: "/captura/nueva", label: "Captura libre", icon: "✍️", group: "sistema" },
  { to: "/ajustes/integraciones", label: "Integraciones", icon: "🔌", group: "sistema" },
  { to: "/importar", label: "Importar / exportar", icon: "⇅", group: "sistema" },
  { to: "/sincronizacion", label: "Sincronización", icon: "⟳", group: "sistema" },
  { to: "/instalar", label: "Instalar app", icon: "📲", group: "sistema" },
  { to: "/ajustes", label: "Ajustes", icon: "⚙️", group: "sistema" },
];

export const GROUP_LABEL: Record<NavItem["group"], string> = {
  principal: "Principal",
  organizar: "Organizar",
  finanzas: "Finanzas",
  vida: "Vida",
  conocimiento: "Conocimiento",
  sistema: "Sistema",
};

/** Filtra módulos desactivados en las preferencias (`modules`). Ausente = activo. */
export function visibleItems(
  items: NavItem[],
  modules: Record<string, unknown> | null | undefined,
): NavItem[] {
  return items.filter((i) => !i.module || modules?.[i.module] !== false);
}
