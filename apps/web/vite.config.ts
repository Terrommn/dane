import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { VitePWA } from "vite-plugin-pwa";
import { defineConfig } from "vitest/config";

// Destino del proxy de la API en desarrollo (VIDA_API_URL para usar otro puerto).
const apiTarget =
  (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env?.VIDA_API_URL ??
  "http://127.0.0.1:8080";

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
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
    css: false,
  },
});
