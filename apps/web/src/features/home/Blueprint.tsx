// Fondo de Inicio en escritorio: planos de un edificio de 4 pisos dibujados en código (líneas ámbar sobre negro).
// Reemplaza al video de IA: el usuario lo sentía "muy AI" y muy protagonista; unas líneas finas sí pueden ir a pantalla
// completa detrás del contenido sin competir con él, y pesan casi nada.
// Ciclo en bucle: se dibuja → pausa → se abre por pisos (vista "explotada") → se cierra → se desvanece → vuelve a empezar.
import { useEffect, useMemo, useRef } from "react";

type Pt = [number, number];

// Proyección isométrica (30°) a coordenadas del viewBox 1440×900. Origen y escala elegidos para que el edificio quede a
// la derecha, completo, y el saludo a la izquierda tenga espacio.
const O = { x: 1100, y: 600 };
const K = 0.68;
const COS = Math.cos(Math.PI / 6);
const SIN = Math.sin(Math.PI / 6);
const P = (x: number, y: number, z: number): Pt => [O.x + (x - y) * COS * K, O.y + ((x + y) * SIN - z) * K];

const W = 300;
const D = 220;
const H = 120;
/** Separación entre pisos al "abrirse" (unidades del viewBox). */
const GAP = 40;

interface Line {
  d: string;
  soft?: boolean;
}

const toD = (pts: Pt[], closed = false) =>
  pts.map((p, i) => `${i ? "L" : "M"}${p[0].toFixed(1)} ${p[1].toFixed(1)}`).join(" ") + (closed ? " Z" : "");

/** Caja (mueble) con solo sus aristas visibles. */
function box(x: number, y: number, z: number, w: number, d: number, h: number): Line[] {
  const a = P(x, y, z + h);
  const b = P(x + w, y, z + h);
  const c = P(x + w, y + d, z + h);
  const e = P(x, y + d, z + h);
  return [
    { d: toD([P(x, y + d, z), P(x + w, y + d, z), P(x + w, y, z)]), soft: true },
    { d: toD([a, b, c, e], true), soft: true },
    { d: toD([P(x + w, y, z), b]), soft: true },
    { d: toD([P(x + w, y + d, z), c]), soft: true },
    { d: toD([P(x, y + d, z), e]), soft: true },
  ];
}

// Mobiliario por piso (de abajo hacia arriba): jardín/meditación, oficina, biblioteca, gimnasio. Sin etiquetas: el
// usuario prefirió que el edificio hable solo.
const FURNITURE: ((z: number) => Line[])[] = [
  (z) => [...box(30, 30, z, 70, 40, 8), ...box(180, 120, z, 60, 60, 30), ...box(40, 150, z, 30, 30, 50)],
  (z) => [
    ...box(60, 60, z, 160, 50, 36),
    ...box(90, 70, z + 36, 100, 6, 28),
    ...box(120, 130, z, 34, 34, 40),
  ],
  (z) => [...box(10, 10, z, 260, 18, 100), ...box(100, 110, z, 50, 50, 40), ...box(170, 120, z, 44, 44, 40)],
  (z) => [...box(40, 50, z, 70, 70, 95), ...box(170, 40, z, 100, 26, 30), ...box(150, 140, z, 80, 40, 4)],
];

function buildFloors(): Line[][] {
  const floors = FURNITURE.map((furnish, i) => {
    const z = i * H;
    const slab: Line = { d: toD([P(0, 0, z), P(W, 0, z), P(W, D, z), P(0, D, z)], true) };
    const cols: Line[] = (
      [
        [0, 0],
        [W, 0],
        [W, D],
        [0, D],
      ] as const
    ).map(([x, y]) => ({ d: toD([P(x, y, z), P(x, y, z + H)]) }));
    return [slab, ...cols, ...furnish(z)];
  });
  // Techo: se mueve como un "piso" más al abrirse.
  floors.push([{ d: toD([P(0, 0, 4 * H), P(W, 0, 4 * H), P(W, D, 4 * H), P(0, D, 4 * H)], true) }]);
  return floors;
}

const EASE = "cubic-bezier(.6,.1,.2,1)";
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export function Blueprint() {
  const floors = useMemo(buildFloors, []);
  const root = useRef<SVGGElement>(null);

  useEffect(() => {
    const g = root.current;
    if (!g) return;
    const reduced =
      typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
    const paths = [...g.querySelectorAll<SVGPathElement>("path")];
    const groups = [...g.querySelectorAll<SVGGElement>("g[data-floor]")];
    if (reduced || typeof g.animate !== "function") {
      // Sin animación (accesibilidad o navegador viejo/pruebas): el plano completo y quieto.
      for (const p of paths) p.style.strokeDashoffset = "0";
      return;
    }
    let alive = true;
    const run = async () => {
      while (alive) {
        g.style.opacity = "1";
        // 1) Dibujar: cada línea con un pequeño retraso, piso por piso de abajo hacia arriba.
        const draws = paths.map((p, i) =>
          p.animate([{ strokeDashoffset: 1 }, { strokeDashoffset: 0 }], {
            duration: 1400,
            delay: i * 28,
            easing: EASE,
            fill: "forwards",
          }),
        );
        await Promise.all(draws.map((a) => a.finished)).catch(() => {});
        if (!alive) return;
        await sleep(2000);
        // 2) Abrir: cada piso sube en proporción a su altura (vista explotada).
        const open = groups.map((fl, i) =>
          fl.animate([{ transform: "translate(0,0)" }, { transform: `translate(0,${-i * GAP}px)` }], {
            duration: 1800,
            easing: "cubic-bezier(.45,0,.2,1)",
            fill: "forwards",
          }),
        );
        await Promise.all(open.map((a) => a.finished)).catch(() => {});
        if (!alive) return;
        await sleep(2600);
        // 3) Cerrar y desvanecer ("sale de la pantalla"), luego reiniciar el trazo para el siguiente ciclo.
        const close = groups.map((fl, i) =>
          fl.animate([{ transform: `translate(0,${-i * GAP}px)` }, { transform: "translate(0,0)" }], {
            duration: 1400,
            easing: "cubic-bezier(.45,0,.2,1)",
            fill: "forwards",
          }),
        );
        await Promise.all(close.map((a) => a.finished)).catch(() => {});
        const fade = g.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 900, fill: "forwards" });
        await fade.finished.catch(() => {});
        if (!alive) return;
        for (const a of [...draws, ...open, ...close, fade]) a.cancel();
        g.style.opacity = "0";
        await sleep(500);
      }
    };
    void run();
    return () => {
      alive = false;
      for (const a of g.getAnimations({ subtree: true })) a.cancel();
    };
  }, []);

  return (
    <div aria-hidden className="pointer-events-none fixed inset-0 z-0 hidden md:block">
      {/* Cuadrícula de papel de plano, más visible hacia el edificio. */}
      <div className="absolute inset-0 [background-image:linear-gradient(rgb(240_122_46/0.05)_1px,transparent_1px),linear-gradient(90deg,rgb(240_122_46/0.05)_1px,transparent_1px),linear-gradient(rgb(240_122_46/0.025)_1px,transparent_1px),linear-gradient(90deg,rgb(240_122_46/0.025)_1px,transparent_1px)] [background-size:120px_120px,120px_120px,24px_24px,24px_24px] [mask-image:radial-gradient(ellipse_at_70%_45%,#000_20%,transparent_75%)]" />
      <svg
        aria-hidden="true"
        viewBox="0 0 1440 900"
        preserveAspectRatio="xMidYMid slice"
        className="absolute inset-0 size-full [mask-image:linear-gradient(90deg,transparent_0%,#000_45%)]"
      >
        <g ref={root}>
          {floors.map((lines, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: geometría fija; el índice ES la identidad del piso
            <g key={i} data-floor={i}>
              {lines.map((l, j) => (
                <path
                  // biome-ignore lint/suspicious/noArrayIndexKey: geometría fija generada una sola vez
                  key={j}
                  d={l.d}
                  pathLength={1}
                  fill="none"
                  stroke={l.soft ? "rgb(246 163 90 / 0.5)" : "var(--c-accent)"}
                  strokeWidth={l.soft ? 0.8 : 1.1}
                  strokeLinecap="round"
                  strokeDasharray="1"
                  strokeDashoffset="1"
                />
              ))}
            </g>
          ))}
          {/* Cota general, como en un plano de arquitectura. */}
          <path
            d={toD([P(0, D + 50, 0), P(W, D + 50, 0)])}
            pathLength={1}
            fill="none"
            stroke="rgb(160 149 140 / 0.45)"
            strokeWidth={0.7}
            strokeDasharray="1"
            strokeDashoffset="1"
          />
        </g>
      </svg>
    </div>
  );
}
