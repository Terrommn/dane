// Genera los iconos PNG de la PWA (sin dependencias): fondo teal y una "V" clara.
// Uso: node scripts/make-icons.mjs
import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";

const OUT = path.join(path.dirname(new URL(import.meta.url).pathname), "..", "public", "icons");
fs.mkdirSync(OUT, { recursive: true });

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
};

function distToSegment(px, py, ax, ay, bx, by) {
  const dx = bx - ax;
  const dy = by - ay;
  const t = Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)));
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
}

function icon(size, { maskable }) {
  const bg = [15, 118, 110];
  const fg = [240, 253, 250];
  const radius = maskable ? 0 : size * 0.22;
  const scale = maskable ? 0.62 : 0.8; // zona segura del 80% en maskable
  const c = size / 2;
  const s = (size * scale) / 2;
  const a = [c - s * 0.6, c - s * 0.42];
  const m = [c, c + s * 0.48];
  const b = [c + s * 0.6, c - s * 0.42];
  const stroke = size * 0.085 * scale;
  const raw = Buffer.alloc(size * (size * 4 + 1));
  for (let y = 0; y < size; y++) {
    raw[y * (size * 4 + 1)] = 0;
    for (let x = 0; x < size; x++) {
      const px = x + 0.5;
      const py = y + 0.5;
      // Esquinas redondeadas (solo icono "any").
      let alpha = 255;
      if (radius > 0) {
        const cx = Math.min(Math.max(px, radius), size - radius);
        const cy = Math.min(Math.max(py, radius), size - radius);
        const d = Math.hypot(px - cx, py - cy);
        alpha = d > radius ? 0 : d > radius - 1 ? Math.round((radius - d) * 255) : 255;
      }
      const d = Math.min(distToSegment(px, py, ...a, ...m), distToSegment(px, py, ...m, ...b));
      const t = Math.max(0, Math.min(1, stroke - d + 0.5));
      const o = y * (size * 4 + 1) + 1 + x * 4;
      for (let i = 0; i < 3; i++) raw[o + i] = Math.round(bg[i] * (1 - t) + fg[i] * t);
      raw[o + 3] = alpha;
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", zlib.deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

for (const [name, size, maskable] of [
  ["icon-192.png", 192, false],
  ["icon-512.png", 512, false],
  ["maskable-512.png", 512, true],
]) {
  fs.writeFileSync(path.join(OUT, name), icon(size, { maskable }));
  console.log("✓", name);
}
