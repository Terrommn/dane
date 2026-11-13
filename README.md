# Vida

Aplicación personal de hábitos, objetivos y organización de vida, derivada del vault de Obsidian
"segundo cerebro" (`fixtures/vault-skeleton/`). Telegram es la entrada cotidiana (recordatorios y check-in en la Mini App)
y la misma web funciona en el navegador y como PWA instalable.

| Pieza | Tecnología |
|---|---|
| `apps/server` | Rust · Axum · SQLx · PostgreSQL 17 — API `/api/v1`, reglas de negocio, auth, scheduler, integraciones |
| `apps/telegram-bot` | Rust · teloxide — transporta updates y entrega mensajes vía API interna (sin acceso a la BD) |
| `apps/web` | React · Vite · TypeScript — un solo build para Mini App, web y PWA |
| `crates/domain` | Reglas puras (fechas/zonas, dinero, hábitos, métricas, finanzas, recordatorios, parser del vault) |
| `crates/persistence` | Migraciones y repositorios |
| `crates/contracts` | DTOs de la API interna server ↔ bot |
| `packages/api-client` | Cliente TypeScript generado del OpenAPI del server |
| `infra/` | Dockerfiles, Compose dev/prod, Caddy |

## Requisitos
* Rust ≥ 1.94 (con rustfmt y clippy), Node ≥ 24, pnpm 12.
* PostgreSQL 17: nativo (`brew install postgresql@17`) **o** Docker.

## Arranque local
```bash
cp .env.example .env          # ya trae valores de desarrollo (dev-login con el ID 111)
pnpm install
pnpm dev                      # BD local (puerto 5433) + server :8080 + bot (si hay token) + web :5180
```
Abre http://localhost:5180 → "Entrar en modo desarrollo" con el ID `111` (está en `ALLOWED_TELEGRAM_IDS`).
Para probar el bot y la Mini App reales sigue [`docs/USER_SETUP.md`](docs/USER_SETUP.md).

Comandos útiles:
| Comando | Qué hace |
|---|---|
| `pnpm verify` | fmt, clippy, pruebas Rust (con PostgreSQL real), contrato OpenAPI, Biome, typecheck, pruebas y build web, E2E, backup/restore. Tabla de resultados y código de salida |
| `pnpm gen:api` | Regenera `packages/api-client` desde el server |
| `pnpm db start\|stop\|reset\|psql` | PostgreSQL local de desarrollo |
| `scripts/backup.sh` / `scripts/restore.sh` | Respaldo y restauración |

## Documentación
* [`docs/VAULT_REQUIREMENTS.md`](docs/VAULT_REQUIREMENTS.md) — matriz vault → app y correcciones.
* [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md), [`docs/DATA_MODEL.md`](docs/DATA_MODEL.md), [`docs/DECISIONS.md`](docs/DECISIONS.md).
* [`docs/TASKS.md`](docs/TASKS.md) — tareas con evidencia · [`docs/TEST_REPORT.md`](docs/TEST_REPORT.md) — resultados.
* [`docs/USER_SETUP.md`](docs/USER_SETUP.md) — lo que necesitas configurar tú · [`docs/DEPLOY.md`](docs/DEPLOY.md) — despliegue.
