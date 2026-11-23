// Fondo de Inicio en escritorio: planos de un edificio de 4 pisos dibujados en código (líneas ámbar sobre negro).
// Reemplaza al video de IA: el usuario lo sentía "muy AI" y muy protagonista; unas líneas finas sí pueden ir a pantalla
// completa detrás del contenido sin competir con él, y pesan casi nada.
//
// Ciclo en bucle (pedido por el usuario):
//   1. se dibuja piso por piso → 2. pausa → 3. se abre por pisos (vista "explotada") → 4. pausa →
//   5. cada línea sale hacia donde "apunta" su pared (la norte al norte, la este al este…) estirándose sobre su eje →
//   6. en el punto más abierto regresa por el mismo camino → 7. los pisos bajan y el edificio queda armado → repetir.
// Se dibuja una sola vez: el ciclo es reversible y continuo para que no haya tirones.
// Además: un clic en el fondo (no en tarjetas ni controles) lanza una onda y la cuadrícula "se abre" y regresa despacio.
// Cada arista es un <path> independiente: así cada pared puede salir en su propia dirección.
import { useEffect, useMemo, useRef } from "react";

type Pt = [number, number];

// Proyección isométrica (30°) a coordenadas del viewBox 1440×900. Origen y escala elegidos para que el edificio quede a
// la derecha, completo (también abierto), y el saludo a la izquierda tenga espacio.
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
/** Distancia máxima que recorre cada línea al salir antes de regresar (unidades del viewBox). */
const FLY = 420;

export interface Seg {
  a: Pt;
  b: Pt;
  soft?: boolean;
  /** Dirección de salida (unitaria, en pantalla): hacia donde apunta la pared respecto al centro de su piso. */
  out: Pt;
}

const sub = (p: Pt, q: Pt): Pt => [p[0] - q[0], p[1] - q[1]];
const norm = (v: Pt): Pt => {
  const l = Math.hypot(v[0], v[1]);
  return l < 1e-6 ? [0, 0] : [v[0] / l, v[1] / l];
};

/**
 * Dirección de salida de una arista: del centro de su piso hacia el punto medio de la arista. Para aristas que pasan por
 * el centro (casi cero) se usa la perpendicular de la propia línea, para que ninguna se quede quieta.
 */
export function outward(a: Pt, b: Pt, center: Pt): Pt {
  const mid: Pt = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
  const v = sub(mid, center);
  if (Math.hypot(v[0], v[1]) > 4) return norm(v);
  const d = norm(sub(b, a));
  return [-d[1], d[0]];
}

function seg3(p: [number, number, number], q: [number, number, number], center: Pt, soft = false): Seg {
  const a = P(...p);
  const b = P(...q);
  return { a, b, soft, out: outward(a, b, center) };
}

/** Caja (mueble): sus 9 aristas visibles como segmentos sueltos. */
function box(x: number, y: number, z: number, w: number, d: number, h: number, c: Pt): Seg[] {
  const t = z + h;
  return [
    seg3([x, y + d, z], [x + w, y + d, z], c, true),
    seg3([x + w, y + d, z], [x + w, y, z], c, true),
    seg3([x, y, t], [x + w, y, t], c, true),
    seg3([x + w, y, t], [x + w, y + d, t], c, true),
    seg3([x + w, y + d, t], [x, y + d, t], c, true),
    seg3([x, y + d, t], [x, y, t], c, true),
    seg3([x + w, y, z], [x + w, y, t], c, true),
    seg3([x + w, y + d, z], [x + w, y + d, t], c, true),
    seg3([x, y + d, z], [x, y + d, t], c, true),
  ];
}

// Mobiliario por piso (de abajo hacia arriba): jardín/meditación, oficina, biblioteca, gimnasio. Sin etiquetas: el
// usuario prefirió que el edificio hable solo.
const FURNITURE: ((z: number, c: Pt) => Seg[])[] = [
  (z, c) => [
    ...box(30, 30, z, 70, 40, 8, c),
    ...box(180, 120, z, 60, 60, 30, c),
    ...box(40, 150, z, 30, 30, 50, c),
  ],
  (z, c) => [
    ...box(60, 60, z, 160, 50, 36, c),
    ...box(90, 70, z + 36, 100, 6, 28, c),
    ...box(120, 130, z, 34, 34, 40, c),
  ],
  (z, c) => [
    ...box(10, 10, z, 260, 18, 100, c),
    ...box(100, 110, z, 50, 50, 40, c),
    ...box(170, 120, z, 44, 44, 40, c),
  ],
  (z, c) => [
    ...box(40, 50, z, 70, 70, 95, c),
    ...box(170, 40, z, 100, 26, 30, c),
    ...box(150, 140, z, 80, 40, 4, c),
  ],
];

function slab(z: number, c: Pt): Seg[] {
  const corners: [number, number][] = [
    [0, 0],
    [W, 0],
    [W, D],
    [0, D],
  ];
  return corners.map(([x, y], i) => {
    const [nx, ny] = corners[(i + 1) % 4] as [number, number];
    return seg3([x, y, z], [nx, ny, z], c);
  });
}

export function buildFloors(): Seg[][] {
  const floors = FURNITURE.map((furnish, i) => {
    const z = i * H;
    const c = P(W / 2, D / 2, z + H / 2);
    const cols = (
      [
        [0, 0],
        [W, 0],
        [W, D],
        [0, D],
      ] as const
    ).map(([x, y]) => seg3([x, y, z], [x, y, z + H], c));
    return [...slab(z, c), ...cols, ...furnish(z, c)];
  });
  // Techo: se mueve como un "piso" más al abrirse.
  floors.push(slab(4 * H, P(W / 2, D / 2, 4 * H - H / 2)));
  return floors;
}

const EASE = "cubic-bezier(.6,.1,.2,1)";
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const deg = (v: Pt) => (Math.atan2(v[1], v[0]) * 180) / Math.PI;

/** Elementos sobre los que un clic NO lanza la onda: tarjetas y cualquier control. */
const NO_SPLASH = "a,button,input,textarea,select,label,summary,[role=switch],[data-card],header,nav,dialog";

export function Blueprint() {
  const floors = useMemo(buildFloors, []);
  const root = useRef<SVGGElement>(null);
  const grid = useRef<HTMLDivElement>(null);
  const layer = useRef<HTMLDivElement>(null);

  // Ciclo de animación del edificio.
  useEffect(() => {
    const g = root.current;
    if (!g) return;
    const reduced =
      typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
    const paths = [...g.querySelectorAll<SVGPathElement>("path[data-seg]")];
    const groups = [...g.querySelectorAll<SVGGElement>("g[data-floor]")];
    if (reduced || typeof g.animate !== "function") {
      // Sin animación (accesibilidad o navegador viejo/pruebas): el plano completo y quieto.
      for (const p of paths) p.style.strokeDashoffset = "0";
      return;
    }
    let alive = true;
    const run = async () => {
      g.style.opacity = "1";
      // 1) Dibujar una sola vez al entrar: cada línea con un pequeño retraso, piso por piso de abajo hacia arriba.
      // Por qué solo una vez: borrar y redibujar en cada vuelta se sentía como un "tirón" en todo el dashboard.
      const draws = paths.map((p, i) =>
        p.animate([{ strokeDashoffset: 1 }, { strokeDashoffset: 0 }], {
          duration: 1200,
          delay: i * 18,
          easing: EASE,
          fill: "forwards",
        }),
      );
      await Promise.all(draws.map((a) => a.finished)).catch(() => {});
      // Ciclo continuo y reversible: nada desaparece de golpe, todo vuelve por el mismo camino.
      while (alive) {
        await sleep(2000);
        if (!alive) return;
        // 2) Abrir: cada piso sube en proporción a su altura (vista explotada).
        const lift = (i: number) => `translate(0,${-i * GAP}px)`;
        await Promise.all(
          groups.map(
            (fl, i) =>
              fl.animate([{ transform: "translate(0,0)" }, { transform: lift(i) }], {
                duration: 1800,
                easing: "cubic-bezier(.45,0,.2,1)",
                fill: "forwards",
              }).finished,
          ),
        ).catch(() => {});
        if (!alive) return;
        await sleep(1600);
        // 3) Dispersar y regresar en UNA sola animación por línea: sale en su dirección estirándose sobre su eje,
        //    se detiene suavemente en el punto más abierto y vuelve por el mismo camino a su lugar.
        await Promise.all(
          paths.map((p) => {
            const out = (p.dataset.out ?? "0,0").split(",").map(Number) as Pt;
            const ang = Number(p.dataset.ang ?? 0);
            const t = (k: number, sx: number) =>
              `translate(${(out[0] * FLY * k).toFixed(1)}px,${(out[1] * FLY * k).toFixed(1)}px) rotate(${ang}deg) scaleX(${sx}) rotate(${-ang}deg)`;
            return p.animate(
              [
                // Misma lista de funciones en todos los fotogramas para que se interpole cada una (sin giros raros).
                { transform: t(0, 1), opacity: 1, easing: "cubic-bezier(.3,0,.2,1)" },
                { transform: t(1, 2.6), opacity: 0.45, offset: 0.45, easing: "linear" },
                { transform: t(1, 2.6), opacity: 0.45, offset: 0.55, easing: "cubic-bezier(.6,0,.3,1)" },
                { transform: t(0, 1), opacity: 1 },
              ],
              { duration: 4200 },
            ).finished;
          }),
        ).catch(() => {});
        if (!alive) return;
        await sleep(600);
        // 4) Bajar: los pisos regresan a su lugar y el edificio queda armado como al inicio.
        await Promise.all(
          groups.map(
            (fl, i) =>
              fl.animate([{ transform: lift(i) }, { transform: "translate(0,0)" }], {
                duration: 1800,
                easing: "cubic-bezier(.45,0,.2,1)",
                fill: "forwards",
              }).finished,
          ),
        ).catch(() => {});
        // Limpia animaciones terminadas de los pisos para que no se acumulen vuelta tras vuelta.
        for (const fl of groups)
          for (const a of fl.getAnimations()) if (a.playState === "finished") a.cancel();
      }
    };
    void run();
    return () => {
      alive = false;
      for (const a of g.getAnimations({ subtree: true })) a.cancel();
    };
  }, []);

  // Onda al hacer clic en el fondo: la cuadrícula se abre desde el punto del clic y regresa poco a poco.
  useEffect(() => {
    const reduced =
      typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduced) return;
    const onDown = (e: PointerEvent) => {
      const box = layer.current;
      const gr = grid.current;
      if (!box || !gr || typeof gr.animate !== "function") return;
      if (getComputedStyle(box).display === "none") return; // solo escritorio
      const t = e.target as Element | null;
      if (t?.closest(NO_SPLASH)) return;
      const ring = document.createElement("span");
      ring.className = "pointer-events-none absolute rounded-full border border-accent/70";
      Object.assign(ring.style, {
        left: `${e.clientX - 20}px`,
        top: `${e.clientY - 20}px`,
        width: "40px",
        height: "40px",
      });
      box.appendChild(ring);
      ring
        .animate(
          [
            { transform: "scale(0.2)", opacity: 0.9, boxShadow: "0 0 30px rgb(240 122 46 / .6)" },
            { transform: "scale(14)", opacity: 0, boxShadow: "0 0 0 rgb(240 122 46 / 0)" },
          ],
          { duration: 1100, easing: "cubic-bezier(.2,.7,.2,1)" },
        )
        .finished.finally(() => ring.remove());
      gr.style.transformOrigin = `${e.clientX}px ${e.clientY}px`;
      gr.animate(
        [
          { transform: "scale(1)", filter: "brightness(1)" },
          { transform: "scale(1.14)", filter: "brightness(2.2)", offset: 0.18 },
          { transform: "scale(1)", filter: "brightness(1)" },
        ],
        { duration: 1800, easing: "cubic-bezier(.2,.8,.2,1)" },
      );
    };
    window.addEventListener("pointerdown", onDown);
    return () => window.removeEventListener("pointerdown", onDown);
  }, []);

  // contain + will-change: la capa animada se pinta aparte y no obliga a repintar el resto de la página en cada cuadro.
  return (
    <div
      ref={layer}
      aria-hidden
      className="pointer-events-none fixed inset-0 z-0 hidden overflow-hidden [contain:strict] md:block"
    >
      {/* Cuadrícula de papel de plano, más visible hacia el edificio. */}
      <div
        ref={grid}
        className="absolute inset-0 [background-image:linear-gradient(rgb(240_122_46/0.05)_1px,transparent_1px),linear-gradient(90deg,rgb(240_122_46/0.05)_1px,transparent_1px),linear-gradient(rgb(240_122_46/0.025)_1px,transparent_1px),linear-gradient(90deg,rgb(240_122_46/0.025)_1px,transparent_1px)] [background-size:120px_120px,120px_120px,24px_24px,24px_24px] [mask-image:radial-gradient(ellipse_at_70%_45%,#000_20%,transparent_75%)]"
      />
      <svg
        aria-hidden="true"
        viewBox="0 0 1440 900"
        preserveAspectRatio="xMidYMid slice"
        className="absolute inset-0 size-full [mask-image:linear-gradient(90deg,transparent_0%,#000_40%)] [will-change:transform]"
      >
        <g ref={root}>
          {floors.map((segs, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: geometría fija; el índice ES la identidad del piso
            <g key={i} data-floor={i}>
              {segs.map((s, j) => {
                const mx = (s.a[0] + s.b[0]) / 2;
                const my = (s.a[1] + s.b[1]) / 2;
                return (
                  <path
                    // biome-ignore lint/suspicious/noArrayIndexKey: geometría fija generada una sola vez
                    key={j}
                    data-seg=""
                    data-out={`${s.out[0].toFixed(4)},${s.out[1].toFixed(4)}`}
                    data-ang={deg(sub(s.b, s.a)).toFixed(2)}
                    d={`M${s.a[0].toFixed(1)} ${s.a[1].toFixed(1)}L${s.b[0].toFixed(1)} ${s.b[1].toFixed(1)}`}
                    // El estiramiento se hace alrededor del punto medio de cada línea, sobre su propio eje.
                    style={{ transformOrigin: `${mx.toFixed(1)}px ${my.toFixed(1)}px` }}
                    pathLength={1}
                    fill="none"
                    stroke={s.soft ? "rgb(246 163 90 / 0.5)" : "var(--c-accent)"}
                    strokeWidth={s.soft ? 0.8 : 1.1}
                    vectorEffect="non-scaling-stroke"
                    strokeLinecap="round"
                    strokeDasharray="1"
                    strokeDashoffset="1"
                  />
                );
              })}
            </g>
          ))}
        </g>
      </svg>
    </div>
  );
}
