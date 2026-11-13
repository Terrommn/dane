import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { VitePWA } from "vite-plugin-pwa";
import { defineConfig } from "vitest/config";

// Destino del proxy de la API en desarrollo (VIDA_API_URL para usar otro puerto).
const apiTarget =
  (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env?.VIDA_API_URL ??
  "http://127.0.0.1:8080";

/** Chunk que pertenece solo al editor de dibujos: el lienzo o dependencias exclusivas de Excalidraw. */
function isDrawingChunk(chunk: { name: string; moduleIds: string[] }): boolean {
  if (chunk.name.includes("ExcalidrawCanvas")) return true;
  const ids = chunk.moduleIds ?? [];
  return ids.length > 0 && ids.every((id) => id.includes("/node_modules/")) && ids.some((id) => id.includes("excalidraw") || /\/node_modules\/\.pnpm\/(mermaid|katex|cytoscape|@mermaid-js|roughjs|d3|dagre|elkjs|khroma|langium|chevrotain|vscode)/.test(id));
}

// Un solo build para Mini App de Telegram, navegador y PWA instalable.
export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    VitePWA({
      // "prompt": la actualización espera confirmación del usuario y nunca recarga a la fuerza (los borradores viven
      // en IndexedDB y sobreviven a la recarga).
      registerType: "prompt",
      injectRegister: false,
      includeAssets: ["icons/*.png", "favicon.svg"],
      manifest: {
        name: "Vida — hábitos, objetivos y organización",
        short_name: "Vida",
        description: "Check-in diario, hábitos, objetivos, finanzas y notas.",
        lang: "es-MX",
        start_url: "/hoy",
        scope: "/",
        display: "standalone",
        orientation: "portrait",
        background_color: "#0f172a",
        theme_color: "#0f766e",
        icons: [
          { src: "/icons/icon-192.png", sizes: "192x192", type: "image/png", purpose: "any" },
          { src: "/icons/icon-512.png", sizes: "512x512", type: "image/png", purpose: "any" },
          { src: "/icons/maskable-512.png", sizes: "512x512", type: "image/png", purpose: "maskable" },
        ],
      },
      workbox: {
        // Solo el app shell (assets estáticos del build). Nada de /api, /auth ni respuestas privadas.
        globPatterns: ["**/*.{js,css,html,svg,png,woff2}"],
        // El editor de dibujos (Excalidraw y sus dependencias, ~8 MB) se descarga solo al abrir un dibujo.
        globIgnores: ["assets/lazy/**"],
        navigateFallback: "/index.html",
        navigateFallbackDenylist: [/^\/api\//, /^\/internal\//, /^\/healthz/, /^\/readyz/],
        runtimeCaching: [],
        cleanupOutdatedCaches: true,
        clientsClaim: false,
        skipWaiting: false,
      },
      devOptions: { enabled: false },
    }),
  ],
  server: {
    port: 5173,
    proxy: {
      "/api": { target: apiTarget, changeOrigin: false },
    },
  },
  preview: {
    port: 4173,
    proxy: {
      "/api": { target: apiTarget, changeOrigin: false },
    },
  },
  build: {
    target: "es2022",
    sourcemap: true,
    rolldownOptions: {
      output: {
        // Chunks del editor de dibujos en assets/lazy/ (fuera del precache del service worker).
        chunkFileNames: (chunk) => (isDrawingChunk(chunk) ? "assets/lazy/[name]-[hash].js" : "assets/[name]-[hash].js"),
      },
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
    css: false,
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
