// Escena principal de Inicio: un video en bucle (generado con Higgsfield: corte isométrico de un edificio donde cada
// piso es un área de tu vida) que se funde con el fondo negro, con borde luminoso como la referencia.
// Por qué con respaldo en CSS: el video es opcional (puede faltar, tardar en red móvil o el usuario pedir menos
// movimiento); en esos casos se ve un degradado ámbar por área y la pantalla nunca queda vacía.
import { useEffect, useState } from "react";

export type PillarKey = "cuerpo" | "mente" | "dinero" | "espiritu";

export const PILLARS: { key: PillarKey; label: string; sub: string; route: string }[] = [
  { key: "cuerpo", label: "Cuerpo", sub: "Peso, energía y ejercicio", route: "/habitos" },
  { key: "mente", label: "Mente", sub: "Objetivos, lecturas y aprendizaje", route: "/objetivos" },
  { key: "dinero", label: "Dinero", sub: "Gasto, presupuesto e ingresos", route: "/finanzas" },
  { key: "espiritu", label: "Espíritu", sub: "Devocional, paz y personas", route: "/devocionales" },
];

/** Archivos en apps/web/public/media. Si no existen, el navegador falla al cargar y se usa el respaldo. */
export const HERO_VIDEO = "/media/dane-hero.mp4";
export const HERO_POSTER = "/media/dane-hero.jpg";

// Un brillo distinto por área: cambia el "tono" de la escena aunque el video sea el mismo.
const GLOW: Record<PillarKey, string> = {
  cuerpo: "radial-gradient(420px 300px at 70% 42%, rgb(240 122 46 / 0.5), transparent 70%)",
  mente: "radial-gradient(380px 280px at 62% 38%, rgb(246 163 90 / 0.42), transparent 70%)",
  dinero: "radial-gradient(440px 260px at 68% 48%, rgb(214 175 90 / 0.42), transparent 70%)",
  espiritu: "radial-gradient(300px 300px at 66% 36%, rgb(255 214 170 / 0.4), transparent 70%)",
};

function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(
    () => typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches,
  );
  useEffect(() => {
    if (typeof matchMedia !== "function") return;
    const mq = matchMedia("(prefers-reduced-motion: reduce)");
    const on = () => setReduced(mq.matches);
    mq.addEventListener?.("change", on);
    return () => mq.removeEventListener?.("change", on);
  }, []);
  return reduced;
}

export function HeroScene({ pillar, children }: { pillar: PillarKey; children: React.ReactNode }) {
  const reduced = useReducedMotion();
  const [videoOk, setVideoOk] = useState(true);
  const [posterOk, setPosterOk] = useState(true);

  return (
    <section
      aria-label="Resumen por área"
      className="relative isolate flex min-h-[340px] flex-col overflow-hidden rounded-[24px] border border-accent/40 bg-[#0d0a08] p-4 shadow-glow md:min-h-[420px] md:p-5"
    >
      {/* Respaldo siempre presente debajo: si el video carga, lo tapa; si no, es lo que se ve. */}
      <div
        aria-hidden
        className="absolute inset-0 -z-20 transition-[background] duration-700"
        style={{ background: GLOW[pillar] }}
      />
      {posterOk && (
        <img
          src={HERO_POSTER}
          alt=""
          aria-hidden
          onError={() => setPosterOk(false)}
          className="absolute inset-0 -z-10 size-full object-cover object-[50%_22%]"
        />
      )}
      {videoOk && !reduced && (
        <video
          src={HERO_VIDEO}
          poster={posterOk ? HERO_POSTER : undefined}
          autoPlay
          muted
          loop
          playsInline
          preload="metadata"
          aria-hidden
          onError={() => setVideoOk(false)}
          className="absolute inset-0 -z-10 size-full object-cover object-[50%_22%]"
        />
      )}
      {/* Fundido hacia el negro (abajo e izquierda) para que el texto se lea y la imagen "se derrita" en el fondo. */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-[5] bg-[linear-gradient(180deg,transparent_35%,var(--c-bg)_100%),linear-gradient(90deg,rgb(10_9_8/0.85)_0%,transparent_45%)]"
      />
      {/* Línea luminosa inferior, como el "horizonte" del banner de referencia. */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-x-8 bottom-0 -z-[4] h-px bg-linear-to-r from-transparent via-accent to-transparent opacity-70"
      />
      {children}
    </section>
  );
}
