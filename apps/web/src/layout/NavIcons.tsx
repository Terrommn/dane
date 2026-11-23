// Iconos de línea para la barra lateral de escritorio. Se usan en lugar de los emojis de nav.ts porque los emojis
// cambian de aspecto según el sistema operativo y no toman el color del tema (el activo debe verse violeta).
// Los emojis se conservan en nav.ts para la barra inferior móvil y la pantalla "Más", que no cambian.
import type { ReactNode } from "react";

const PATHS: Record<string, ReactNode> = {
  "/hoy": (
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
    </>
  ),
  "/objetivos": (
    <>
      <circle cx="12" cy="12" r="9" />
      <circle cx="12" cy="12" r="5" />
      <circle cx="12" cy="12" r="1" />
    </>
  ),
  "/dashboard": <path d="M3 3v18h18M7 15l4-4 3 3 5-6" />,
  "/habitos": (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M8.5 12.5l2.5 2.5 4.5-5" />
    </>
  ),
  "/tareas": <path d="M10 6h10M10 12h10M10 18h10M3.5 6l1.5 1.5L7.5 5M3.5 12l1.5 1.5L7.5 11M4 18h2" />,
  "/inbox": <path d="M3 13h5l1.5 3h5L16 13h5M5.5 5h13L21 13v6H3v-6z" />,
  "/agenda": (
    <>
      <rect x="3" y="5" width="18" height="16" rx="2" />
      <path d="M8 3v4M16 3v4M3 10h18" />
    </>
  ),
  "/semanal": (
    <>
      <rect x="3" y="5" width="18" height="16" rx="2" />
      <path d="M8 3v4M16 3v4M3 10h18M7 14h10M7 17h6" />
    </>
  ),
  "/proyectos": <path d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2z" />,
  "/finanzas": (
    <path d="M19 7V5a2 2 0 00-2-2H5a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-3M21 10h-5a2 2 0 000 4h5z" />
  ),
  "/finanzas/movimientos": (
    <path d="M5 3v18l2.5-1.5L10 21l2-1.5 2 1.5 2.5-1.5L19 21V3l-2.5 1.5L14 3l-2 1.5L10 3 7.5 4.5zM9 9h6M9 13h6" />
  ),
  "/personas": (
    <>
      <circle cx="9" cy="8" r="3.5" />
      <path d="M2.5 20a6.5 6.5 0 0113 0M16 4.5a3.5 3.5 0 010 7M18 14a6 6 0 013.5 6" />
    </>
  ),
  "/devocionales": <path d="M12 20s-7-4.4-7-10a4 4 0 017-2.6A4 4 0 0119 10c0 5.6-7 10-7 10z" />,
  "/lecturas": <path d="M4 5a2 2 0 012-2h13v16H6a2 2 0 00-2 2zM4 21V5M19 19v2H6" />,
  "/contenido": (
    <>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="M7 4v16M17 4v16M3 9h4M3 15h4M17 9h4M17 15h4" />
    </>
  ),
  "/swipe": <path d="M6 3h12v18l-6-4-6 4z" />,
  "/escuela": <path d="M2 9l10-5 10 5-10 5zM6 11v5c3 2.5 9 2.5 12 0v-5M22 9v6" />,
  "/dev": <path d="M8 7l-5 5 5 5M16 7l5 5-5 5M14 4l-4 16" />,
  "/notas": <path d="M14 3H6a2 2 0 00-2 2v14a2 2 0 002 2h12a2 2 0 002-2V9zM14 3v6h6M8 13h8M8 17h5" />,
  "/dibujos": <path d="M16.5 3.5a2.1 2.1 0 013 3L7 19l-4 1 1-4z" />,
  "/buscar": (
    <>
      <circle cx="11" cy="11" r="7" />
      <path d="M20 20l-3.5-3.5" />
    </>
  ),
  "/vistas": <path d="M12 3l9 5-9 5-9-5zM3 13l9 5 9-5" />,
  "/captura/nueva": <path d="M12 5v14M5 12h14" />,
  "/ajustes/integraciones": <path d="M9 2v5M15 2v5M6 7h12v4a6 6 0 01-12 0zM12 17v5" />,
  "/importar": <path d="M7 4L3 8l4 4M3 8h14M17 20l4-4-4-4M21 16H7" />,
  "/sincronizacion": (
    <path d="M21 12a9 9 0 01-15.5 6.2L3 16M3 12a9 9 0 0115.5-6.2L21 8M21 3v5h-5M3 21v-5h5" />
  ),
  "/instalar": <path d="M12 3v12M7 10l5 5 5-5M4 21h16" />,
  "/ajustes": (
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M12 2v3M12 19v3M4.2 4.2l2.1 2.1M17.7 17.7l2.1 2.1M2 12h3M19 12h3M4.2 19.8l2.1-2.1M17.7 6.3l2.1-2.1" />
    </>
  ),
  // Controles propios de la barra (no son rutas).
  panel: (
    <>
      <rect x="3" y="4" width="18" height="16" rx="3" />
      <path d="M9 4v16" />
    </>
  ),
  chevron: <path d="M6 9l6 6 6-6" />,
};

/** Icono de línea por ruta (o control). Devuelve null si no hay uno, para que el llamador use el emoji. */
export function NavIcon({ name, className = "size-[18px]" }: { name: string; className?: string }) {
  const p = PATHS[name];
  if (!p) return null;
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`shrink-0 ${className}`}
    >
      {p}
    </svg>
  );
}

export function hasNavIcon(name: string): boolean {
  return name in PATHS;
}
