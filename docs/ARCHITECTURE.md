# Arquitectura

## Vista general

```
                 Telegram (Bot API)
                   ▲        │ updates (polling dev / webhook prod)
   sendMessage     │        ▼
            ┌──────────────────────┐   HTTP interno (Bearer INTERNAL_API_TOKEN)
            │  apps/telegram-bot   │ ───────────────────────────────┐
            │  (Rust, teloxide)    │  POST /internal/v1/telegram/updates  (aceptación durable)
            └──────────────────────┘  POST /internal/v1/jobs/claim | /{id}/ack
                                                                   ▼
 Navegador / PWA / Mini App ──HTTPS──► Caddy (mismo origen) ──► apps/server (Rust, Axum)
   apps/web (React, un build)          /        → web estática      │  dominio (crates/domain)
                                       /api/*   → server            │  persistencia (crates/persistence, SQLx)
                                       /internal/* NO expuesto      │  scheduler + procesador de updates
                                                                    ▼
                                                              PostgreSQL 17 (fuente de verdad)
```

**Producción (Railway)**: no hay Caddy. El servicio `server` sirve la web compilada (`WEB_DIST_DIR`) y `/api` en el
mismo origen `*.up.railway.app`. El servicio `bot` (polling) llama a `/internal/*` por la red privada
(`server.railway.internal`). Ojo: sin Caddy, `/internal/*` **también es alcanzable desde el dominio público**; lo
protege únicamente `INTERNAL_API_TOKEN` (sin él responde 401), así que ese token debe ser largo y aleatorio. El diagrama de arriba con Caddy corresponde al despliegue en VPS
(`infra/compose.prod.yml`). Ver `DEPLOY.md`.

* **apps/server** concentra reglas de negocio y acceso a datos: API `/api/v1`, autenticación (Mini App, OIDC, sesiones),
  scheduler de recordatorios/recurrencias, procesamiento de updates de Telegram, integraciones (Google, ClickUp, LLM),
  importación/exportación. Expone OpenAPI en `/api/v1/openapi.json`.
* **apps/telegram-bot** es delgado: recibe updates y los entrega al servidor; reclama mensajes salientes, los envía a
  Telegram y confirma (ack). **No recibe `DATABASE_URL`** ni conoce reglas de negocio.
* **apps/web** es un único build Vite: funciona como Mini App (cuando hay `initData` válido), web y PWA instalable.
* **crates/domain**: reglas puras sin IO (fechas/zonas, dinero, hábitos, rachas, progreso, finanzas, recurrencias,
  sugerencia de tarjeta, horarios de recordatorio, parseo del vault). Probado con reloj inyectable.
* **crates/persistence**: pool, migraciones SQLx embebidas y repositorios. Consultas con `sqlx::query_as` en tiempo de
  ejecución (ver D-007) y pruebas de integración con PostgreSQL real (`#[sqlx::test]`).
* **crates/contracts**: DTOs compartidos entre server y bot (API interna).
* **packages/api-client**: tipos TypeScript generados de `openapi.json` (`openapi-typescript`) + cliente `openapi-fetch`.

## Flujos clave

### Autenticación
1. **Mini App**: la web detecta `Telegram.WebApp.initData` → `POST /api/v1/auth/telegram/miniapp {init_data}` → el server
   valida HMAC-SHA256 (clave `HMAC("WebAppData", BOT_TOKEN)`), `auth_date` ≤ `MINIAPP_MAX_AGE_SECS`, allowlist →
   sesión `bearer` (token opaco, hash SHA-256 en BD, expira, revocable). El token vive **solo en memoria**; al recargar
   se re-autentica con `initData`. No se usa `initDataUnsafe`.
2. **Web/PWA**: `GET /api/v1/auth/oidc/start` → server crea `state`, `nonce`, PKCE (S256) en `oauth_flows` →
   `oauth.telegram.org/auth` → `GET /api/v1/auth/oidc/callback` → intercambio con Basic auth → validación del ID token
   con `openidconnect` (firma JWKS, `iss`, `aud`, `exp`, `nonce`; Telegram sirve el JWKS con gzip aunque no se pida,
   por eso `reqwest` 0.12 lleva la feature `gzip`, D-035) → claim `id` (ID Telegram, distinto de `sub`) →
   allowlist → vínculo con la misma `users.id` de la Mini App → cookie `dane_session` HttpOnly Secure SameSite=Lax +
   cookie `dane_csrf` (double submit; header `X-CSRF-Token` obligatorio en mutaciones con cookie).
3. **Desarrollo**: `POST /api/v1/auth/dev-login` solo si `APP_ENV=development` y `DEV_LOGIN_ENABLED=true`; el arranque
   falla si se habilita con `APP_ENV=production`.

Cada handler obtiene `AuthUser` del extractor; todos los repositorios reciben `user_id` y filtran por él. Las FKs entre
entidades propias son compuestas `(user_id, id)` para impedir relaciones entre dueños distintos.

### Check-in diario
`PUT /api/v1/checkins/{date}` con `If-Match: <version>` (o `version` en el cuerpo) y `Idempotency-Key`. Merge parcial:
campos ausentes no se tocan; `null` explícito borra. 409 con el estado actual si la versión no coincide. Los efectos
(entradas de hábitos, vínculos explícitos) ocurren en la misma transacción.

### Recordatorios (outbox)
Scheduler (cada 60 s, varios procesos seguros): por usuario calcula la ocurrencia local de hoy en su zona IANA
(`chrono-tz`, DST: hora inexistente → siguiente válida; ambigua → la primera) e inserta `jobs` con
`UNIQUE(user_id, kind, occurrence_key)` (`ON CONFLICT DO NOTHING`). No crea ocurrencias pasadas fuera de gracia.
El bot reclama con `FOR UPDATE SKIP LOCKED` en una transacción corta que asigna `lease_token` y `lease_expires_at`, sin
mantener locks durante la llamada a Telegram. En el claim se cancelan recordatorios cuyo check-in ya está completo y se
expiran los vencidos. Ack con `lease_token`: `sent` (idempotente), `retry` (backoff exponencial o `retry_after` de 429),
`failed` (permanente, p. ej. 403 bot bloqueado → `telegram_chats.can_message=false`). Ack con token obsoleto → 409.
Entrega *al menos una vez*: un timeout tras el envío puede duplicar excepcionalmente.

### Asistente diario en Telegram (sin IA)
Todo es texto armado con plantillas en `apps/server/src/telegram/scheduler.rs`; no usa LLM.
* **Mañana** (`reminder_morning`, `user_settings.morning_enabled/morning_time`, 07:00 por defecto, gracia 3 h):
  resumen con tareas abiertas cuya fecha (`COALESCE(planned_on, due_on)`) es hoy o anterior y eventos con hora del
  día; botón a `/hoy?date=…&guiado=manana`. Se omite/cancela si `daily_checkins.morning_done_at` ya está puesto.
* **Cierre del día** (recordatorio diario existente, `daily_time`, 21:00): botón a `/hoy?date=…&guiado=noche`; se
  omite si `evening_done_at` está puesto.
* **Avisos de eventos**: cada ciclo busca eventos que empiezan en `now..now+event_reminder_minutes(+5 min)` y encola
  un job con `occurrence_key = "<id>@<starts_at>"` (idempotente; si el evento cambia de hora hay un aviso nuevo).
  Respeta `/pause`.
* **Menú**: `hola`, `menú` o `/menu` responden con botones (check-in, mi día, gasto…), en `telegram/commands.rs`.
Todos usan el mismo outbox de jobs descrito arriba.

### Updates de Telegram
Bot (polling en Railway o webhook en VPS, nunca ambos; un solo proceso por token) → `POST /internal/v1/telegram/updates` → `INSERT … ON CONFLICT (update_id) DO NOTHING`
(estado `pending`) → 200 solo tras commit. El webhook responde a Telegram solo después de ese 200. Un procesador en el
server toma updates `pending` (`SKIP LOCKED`), aplica efectos y encola respuestas **en la misma transacción** en que marca
`processed`. Un update repetido no reemplaza ni descarta uno pendiente.

### Finanzas
Montos en centavos `i64` (`BIGINT`) con moneda ISO; tasas en `NUMERIC`. Clases de movimiento: `expense`, `income`,
`transfer`, `card_payment`, `debt_payment`, `loan_received`, `adjustment`. Estados: `confirmed` (manual),
`pending_reconciliation` (generado por regla recurrente), `void`. Solo `confirmed` afecta saldos y flujo; las pendientes
alimentan proyecciones etiquetadas. Ver `DATA_MODEL.md` §Finanzas.

## Fases y dependencias

| Fase | Tareas | Depende de |
|---|---|---|
| 0 Especificación | T01–T04 | — |
| 1 Fundaciones | T05–T10, T21, T62 (parcial) | 0 |
| 2 Identidad | T11–T13 | 1 |
| 3 Dominio núcleo | T16–T20, T22, T24–T34 | 2 |
| 4 Finanzas | T36–T43 | 2 |
| 5 Telegram y jobs | T44–T48 | 2 |
| 6 Integraciones e import/export | T49–T54 | 3, 4 |
| 7 Frontend y PWA | T14, T15, T23, T35, T55–T58 | 2 (shell), 3–4 (módulos) |
| 8 Verificación y operación | T59–T66 | todo |

Fases 3, 4, 5 y 7 se ejecutan en paralelo con agentes con propiedad de archivos (ver `TASKS.md`).

## Definición de terminado
Una tarea está **completa** solo si: (1) está implementada de punta a punta (migración, repositorio, API, pantalla
cuando aplica); (2) tiene pruebas significativas que pasan en `pnpm verify`; (3) la evidencia (archivo de prueba, comando
o captura) está en `TASKS.md`; (4) no hay "próximamente", mocks en producción ni datos inventados. Lo que requiere cuentas o
dispositivos reales queda `blocked_external` con el paso exacto en `USER_SETUP.md`.
