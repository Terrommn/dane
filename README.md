# Dane

Aplicación personal de hábitos, objetivos y organización de vida, derivada del vault de Obsidian
"segundo cerebro" (`fixtures/vault-skeleton/`). Telegram es la entrada cotidiana (recordatorios y check-in en la Mini App)
y la misma web funciona en el navegador y como PWA instalable.

> **Antes de cambiar algo lee [`CLAUDE.md`](CLAUDE.md)**: cada cambio actualiza la documentación, el repo no lleva
> datos personales ni secretos, y cada merge a `main` se publica solo en producción.

## Qué hace hoy
* **Inicio** (`/`): saludo según la hora, resumen del día ("Día N de «objetivo». Hoy tienes…"), botón para empezar
  el check-in guiado, tarjetas de objetivo del mes, hábitos de hoy, racha y peso, e interruptores para marcar los
  hábitos del día. En escritorio, fondo animado de planos dibujado en código (SVG, sin video); en el teléfono, vista
  mínima con barra inferior **Hoy · ＋ · Menú**.
* **Check-in guiado** (`/hoy?guiado=manana|noche`): paso a paso. Mañana: energía, peso, gasto y resumen del día.
  Noche: hábitos, bitácora, reflexión y cierre. Usa el mismo borrador, cola offline y control de conflictos que el
  formulario completo de `/hoy`.
* **Navegación**: en escritorio, barra lateral con Inicio y secciones colapsables (Objetivos, Organizar, Finanzas,
  Vida, Conocimiento, Sistema) que se puede reducir a un riel de iconos; arriba, búsqueda, captura rápida, avisos y
  menú de perfil. Las estadísticas completas están en `/dashboard`.
* **Asistente diario en Telegram (sin IA)**: mensaje de la mañana (07:00 por defecto) con tareas y agenda del día y
  botón al check-in guiado; cierre del día (21:00); aviso N minutos antes de cada evento; `hola`, `menú` o `/menu`
  responden con un menú de botones. Horarios y anticipación se configuran en **Ajustes → Recordatorios en Telegram**.
* Módulos: objetivos, hábitos, tareas, inbox, revisión semanal, finanzas, personas, lecturas, contenido, escuela,
  dev, notas, dibujos, importación del vault e integraciones (Google, ClickUp, captura con IA opcional).

| Pieza | Tecnología |
|---|---|
| `apps/server` | Rust · Axum · SQLx · PostgreSQL 17 — API `/api/v1`, reglas de negocio, auth, scheduler, integraciones |
| `apps/telegram-bot` | Rust · teloxide — transporta updates y entrega mensajes vía API interna (sin acceso a la BD) |
| `apps/web` | React · Vite · TypeScript — un solo build para Mini App, web y PWA |
| `crates/domain` | Reglas puras (fechas/zonas, dinero, hábitos, métricas, finanzas, recordatorios, parser del vault) |
| `crates/persistence` | Migraciones y repositorios |
| `crates/contracts` | DTOs de la API interna server ↔ bot |
| `packages/api-client` | Cliente TypeScript generado del OpenAPI del server |
| `infra/` | Dockerfiles de Railway (`Dockerfile.railway`, `Dockerfile.railway.bot`), Dockerfiles y Compose para VPS, Caddy |

## Requisitos
* Rust ≥ 1.94 (con rustfmt y clippy), Node ≥ 24, pnpm 12.
* PostgreSQL 17: nativo (`brew install postgresql@17`) **o** Docker.

## Arranque local
```bash
cp .env.example .env          # ya trae valores de desarrollo (dev-login con el ID 111)
pnpm install
pnpm dev                      # BD local (puerto 5433) + server :8080 + bot (si hay token) + web :5180
```
⚠️ Si tu `.env` tiene el **mismo** `TELEGRAM_BOT_TOKEN` que producción, `pnpm dev` arranca un segundo lector del
bot y se pelea los mensajes con Railway. Usa un bot de pruebas propio o deja el token vacío (ver `CLAUDE.md` §6).
Abre http://localhost:5180 → "Entrar en modo desarrollo" con el ID `111` (está en `ALLOWED_TELEGRAM_IDS`).
Para probar el bot y la Mini App reales sigue [`docs/USER_SETUP.md`](docs/USER_SETUP.md).

Comandos útiles:
| Comando | Qué hace |
|---|---|
| `pnpm verify` | fmt, clippy, pruebas Rust (con PostgreSQL real), contrato OpenAPI, Biome, typecheck, pruebas y build web, E2E, backup/restore. Tabla de resultados y código de salida |
| `pnpm gen:api` | Regenera `packages/api-client` desde el server |
| `pnpm db start\|stop\|reset\|psql` | PostgreSQL local de desarrollo |
| `scripts/backup.sh` / `scripts/restore.sh` | Respaldo y restauración |

## Producción
Railway: servicios `server` (API + web), `bot` (polling) y `Postgres`, conectados a la rama `main`. Cada merge a
`main` se publica solo. Detalle, variables y respaldos en [`docs/DEPLOY.md`](docs/DEPLOY.md).

## Documentación
* [`CLAUDE.md`](CLAUDE.md) — reglas para cambiar el repo (documentación, datos personales, secretos, commits, deploy).
* [`docs/VAULT_REQUIREMENTS.md`](docs/VAULT_REQUIREMENTS.md) — matriz vault → app y correcciones.
* [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md), [`docs/DATA_MODEL.md`](docs/DATA_MODEL.md), [`docs/DECISIONS.md`](docs/DECISIONS.md).
* [`docs/TASKS.md`](docs/TASKS.md) — tareas con evidencia · [`docs/TEST_REPORT.md`](docs/TEST_REPORT.md) — resultados.
* [`docs/USER_SETUP.md`](docs/USER_SETUP.md) — lo que necesitas configurar tú · [`docs/DEPLOY.md`](docs/DEPLOY.md) — despliegue.
