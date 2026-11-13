# apps/web — Mini App, web y PWA (un solo build)

React 19 + Vite 8 + TypeScript 7 (estricto) + React Router 8 + TanStack Query 5 + React Hook Form + Zod 4 +
Tailwind 4 + Recharts + vite-plugin-pwa (Workbox) + idb. Lint/formato con Biome 2. Pruebas con Vitest 5 + jsdom +
fake-indexeddb + Testing Library.

## Ejecutar

```bash
scripts/dev-db.sh start                         # PostgreSQL local (raíz del repo)
set -a; source .env; set +a; cargo run -p server  # API en 127.0.0.1:8080
pnpm -C apps/web dev                             # http://localhost:5173 (proxy /api → 8080)
```
`VIDA_API_URL=http://127.0.0.1:8099 pnpm -C apps/web dev -- --port 5174` para usar otro puerto de API/web.
`PUBLIC_BASE_URL` del server debe ser el origen de la web (dev-login exige origen local).

Comandos: `pnpm -C apps/web typecheck | lint | test | build | preview`. `pnpm -C apps/web icons` regenera los PNG
de la PWA (Node puro, sin dependencias).

## Estructura

```
src/
  app/          App (rutas), nav, queryClient
  layout/       AppShell (barra inferior móvil / lateral escritorio, BackButton de Telegram), aviso de actualización
  components/ui Button, Field (Text/TextArea/Select/Checkbox/Radio), Dialog/ConfirmDialog, States (vacío/carga/error/QueryState),
                DataTable (filtro sin acentos, orden, paginación; tarjetas en móvil), Display (ProgressBar, KpiCard, Card,
                PageHeader, Badge), DatePicker, Toast, SaveStatus, ConflictDialog
  lib/          api (openapi-fetch + Bearer/CSRF/Idempotency-Key), session (solo memoria), errors, telegram, format
  lib/offline/  db (IndexedDB por usuario), store (borradores + cola), hooks (useDraft, useSyncQueue)
  features/     auth, daily, goals, habits, tasks, reviews, finance, people, content, readings, projects, notes,
                settings, install, more, dashboard
```

## Decisiones

* **Sesión**: Mini App → `POST /auth/telegram/miniapp` con `Telegram.WebApp.initData` sin modificar; el Bearer vive
  solo en memoria y se renueva con initData ante un 401. Web/PWA → cookie HttpOnly + `X-CSRF-Token` leído de
  `vida_csrf` en mutaciones. Nunca se guardan tokens en localStorage (solo el `user_id`, para detectar cambio de cuenta).
* **Telegram**: el script oficial se carga solo si la URL trae `tgWebAppData`/`tgWebAppStartParam`; sin initData
  firmado no hay modo Mini App. Tema (`themeParams` → `--tg-*`), safe areas (`safeAreaInset` + `contentSafeAreaInset`,
  eventos ≥ 8.0), BackButton (≥ 6.1). Fecha objetivo: `?date=` o `start_param` `checkin_YYYY-MM-DD` (validada).
* **Idempotency-Key** automática en cada mutación; la cola offline reutiliza la misma clave al reintentar y genera
  una nueva solo cuando cambia el cuerpo (resolución de conflicto).
* **Offline**: base IndexedDB `vida-offline-<user_id>` (borradores + cola). Estados: `draft` (solo dispositivo),
  `pending` (en cola), `saved` (confirmado por el servidor), `conflict` (409 con `current`), `error` (4xx de
  validación). Sincroniza al abrir, al reconectar y al volver a primer plano; no usa Background Sync. Orden estricto
  de la cola (marca de tiempo monotónica). Logout: avisa si hay pendientes, permite sincronizar antes y borra la base
  del usuario y la caché de consultas. Cambio de cuenta: se descartan los datos locales de la cuenta anterior con aviso.
* **Service worker**: `registerType: "prompt"`; precache solo del app shell; `NavigationRoute` con denylist de
  `/api`, `/internal`, `/healthz`, `/readyz`; sin `runtimeCaching` → ninguna respuesta privada en caché. La
  actualización espera a que el usuario la acepte; los borradores sobreviven porque viven en IndexedDB.
* **Compatibilidad verificada**: TypeScript 7.0.2 compila el proyecto (`tsc --noEmit`); `openapi-typescript` declara
  peer `typescript ^5` pero solo se usa en `packages/api-client`, no aquí. `biome migrate` convertía
  `recommended: true` en `preset: "none"`; se fijó `preset: "recommended"` (verificado con una sonda de lint).

## API para la fase 2

```ts
const { draft, status, saveDebounced, submit, clear } = useDraft<CheckinForm>("checkin", date);
saveDebounced(form, serverVersion);                       // borrador local
await submit("PUT", `/checkins/${date}`, body, version);  // cola + sincronización
<SaveStatus status={status} />
const q = useSyncQueue();  // items, pending, conflicts, errors, flush, resolve(id, r), drop(id), online
```
