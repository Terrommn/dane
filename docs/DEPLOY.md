# Runbook de despliegue

Producción corre en **Railway** (§A). La alternativa portable con Docker Compose en cualquier VPS sigue
documentada en §B. En los dos casos API, bot y PostgreSQL deben quedar **siempre encendidos**: los recordatorios y
mensajes automáticos dependen de ello (no uses planes que duermen los servicios).

## A. Railway (producción actual)

### Servicios
| Servicio | Origen | Receta | Qué hace |
|---|---|---|---|
| `server` | GitHub, rama `main` | `infra/Dockerfile.railway` | API `/api/v1` + web compilada (`WEB_DIST_DIR=/srv/web`) en un solo dominio `*.up.railway.app`; scheduler; migraciones al arrancar |
| `bot` | GitHub, rama `main` | `infra/Dockerfile.railway.bot` | Bot de Telegram en modo **polling**; habla con el server por la red privada (`http://server.railway.internal:8080`); sin `DATABASE_URL` |
| `Postgres` | Plantilla de Railway | — | Base de datos con volumen persistente. No se redepliega con el código |

La receta de cada servicio se elige con la variable `RAILWAY_DOCKERFILE_PATH`. Por qué un solo contenedor para web +
API y no Caddy: ver comentario en `infra/Dockerfile.railway` y D-033.

### Cómo se publica un cambio
1. Rama → PR → merge a `main`. **No hay paso manual**: Railway detecta el push a `main`, reconstruye `server` y
   `bot` (3–5 min, Rust en release) y cambia a la versión nueva solo cuando arranca. Si falla, sigue la anterior.
2. Las migraciones de `crates/persistence/migrations/` se aplican solas al arrancar el server. Deben ser compatibles
   hacia atrás durante unos minutos (la versión anterior sigue atendiendo mientras arranca la nueva).
3. Comprueba: Railway → Deployments en `SUCCESS` y `GET /readyz` → `ready`.

Opcional: en el servicio `bot` → Settings → **Watch Paths**, limitar los redespliegues a lo que usa el bot
(`/apps/telegram-bot/**`, `/crates/contracts/**`, `/Cargo.toml`, `/Cargo.lock`, `/rust-toolchain.toml`,
`/infra/Dockerfile.railway.bot`). Si el bot empieza a depender de otro crate, agrégalo a la lista.

Despliegue manual de emergencia (sube el código local sin pasar por GitHub; el siguiente merge a `main` lo
reemplaza): `railway up --service server --detach` (o `--service bot`).

### Variables (solo nombres; los valores viven en Railway → servicio → Variables)
**server**: `APP_ENV=production`, `DATABASE_URL` (referencia a `Postgres`), `DATABASE_MAX_CONNECTIONS`,
`PUBLIC_BASE_URL` (el dominio público del servicio, con `https://`), `ALLOWED_TELEGRAM_IDS`, `COOKIE_SECURE=true`,
`DEV_LOGIN_ENABLED=false`, `DEMO_ENABLED=false`, `SCHEDULER_ENABLED=true`, `INTERNAL_API_TOKEN`, `ENCRYPTION_KEY`,
`TELEGRAM_BOT_TOKEN`, `TELEGRAM_BOT_USERNAME`, `OIDC_CLIENT_ID`, `OIDC_CLIENT_SECRET`, `OIDC_ISSUER`, `LOG_FORMAT`,
`RUST_LOG`, `RAILWAY_DOCKERFILE_PATH`. Opcionales: `GOOGLE_CLIENT_ID`/`GOOGLE_CLIENT_SECRET`, `ANTHROPIC_API_KEY`.

**bot**: `TELEGRAM_BOT_TOKEN`, `TELEGRAM_MODE=polling`, `SERVER_INTERNAL_URL=http://server.railway.internal:8080`,
`INTERNAL_API_TOKEN` (el mismo que el server), `BOT_BIND_ADDR`, `LOG_FORMAT`, `RAILWAY_DOCKERFILE_PATH`.

Reglas:
* `ALLOWED_TELEGRAM_IDS`: IDs numéricos separados por comas, sin espacios ni llaves (`111111111,222222222`).
  Cada persona de la lista tiene su propia cuenta y sus datos separados; se crea sola al escribirle `/start` al bot.
  En producción no puede quedar vacía (el server no arranca).
* Guardar una variable redepliega ese servicio (3–5 min, sin caída).
* Variables nuevas: agrégalas también a `.env.example` (sin valor real) y a esta lista.

### Por qué polling y no webhook
Un solo servicio `bot` siempre encendido, sin exponer `/telegram/webhook` ni administrar `WEBHOOK_SECRET`. Telegram
permite **un** lector por bot: no corras otro proceso con el mismo token (en local usa un bot de pruebas). Ver D-034.

### Comprobaciones posteriores
| Qué | Cómo | Esperado |
|---|---|---|
| API viva | `curl https://<dominio>/readyz` | `ready` |
| Interno protegido | `curl -i -X POST https://<dominio>/internal/v1/jobs/claim` | `401` (sin Caddy la ruta es pública; la protege `INTERNAL_API_TOKEN`) |
| Bot | En Telegram: `hola` o `/menu` | Menú con botones |
| Mini App | Botón del menú del bot | Abre la app autenticada |
| Web | `https://<dominio>/login` → Iniciar sesión con Telegram | Vuelve autenticado |

### Respaldos y rotación en Railway
* Respaldo puntual: copia `DATABASE_PUBLIC_URL` de Postgres → Variables y corre
  `DATABASE_URL='<esa url>' scripts/backup.sh` (requiere `pg_dump` 17+). El archivo queda en `backups/`, que está en
  `.gitignore`; guárdalo también fuera de tu equipo. Para psql interactivo: `railway connect Postgres`.
* `INTERNAL_API_TOKEN`: cámbialo en **server y bot** a la vez.
* Token del bot comprometido: `/revoke` en @BotFather y actualiza `TELEGRAM_BOT_TOKEN` en server y bot.
* Quitar acceso: elimina el ID de `ALLOWED_TELEGRAM_IDS` y revoca sesiones (Ajustes → cerrar sesión en todos lados).
* `ENCRYPTION_KEY` se respalda aparte (gestor de contraseñas): sin ella las credenciales OAuth guardadas no se pueden
  descifrar.

## B. Alternativa portable: VPS con Docker Compose

Cualquier VPS Linux con Docker (≥1 vCPU, 1 GB RAM, 10 GB disco) y un dominio con DNS. Usa `infra/compose.prod.yml`
(Caddy con HTTPS, bot en modo webhook).

No depende de ningún proveedor pagado específico: cualquier VPS Linux con Docker (≥1 vCPU, 1 GB RAM, 10 GB disco)
y un dominio con DNS. API, bot y PostgreSQL deben quedar **siempre encendidos** para cumplir los horarios de
recordatorios (no uses planes que duermen los servicios).

### 1. Preparar el servidor
```bash
# En el VPS (Debian/Ubuntu)
curl -fsSL https://get.docker.com | sh
git clone <tu-repo> dane && cd dane
cp .env.example .env.prod   # completa según docs/USER_SETUP.md
```
DNS: registro `A` (y `AAAA` si aplica) de `DOMAIN` → IP del VPS. Puertos 80 y 443 abiertos (Caddy obtiene HTTPS solo).

Variables mínimas en `.env.prod`: `DOMAIN`, `ACME_EMAIL`, `POSTGRES_PASSWORD`, `ALLOWED_TELEGRAM_IDS`,
`INTERNAL_API_TOKEN` (`openssl rand -hex 32`), `ENCRYPTION_KEY` (`openssl rand -base64 32`), `TELEGRAM_BOT_TOKEN`,
`TELEGRAM_BOT_USERNAME`, `WEBHOOK_SECRET` (`openssl rand -hex 24`). Opcionales: OIDC, Google, Anthropic.

### 2. Levantar
```bash
docker compose -f infra/compose.prod.yml --env-file .env.prod up -d --build
docker compose -f infra/compose.prod.yml --env-file .env.prod ps        # server y bot "healthy"
curl -fsS https://$DOMAIN/healthz                                        # → ok
```
El server aplica migraciones al arrancar (`dane-server migrate` también existe para hacerlo explícitamente). El bot en
modo `webhook` registra `https://$DOMAIN/telegram/webhook` con el secreto y elimina cualquier polling.

### 3. Comprobaciones posteriores
| Qué | Cómo | Esperado |
|---|---|---|
| API viva | `curl https://$DOMAIN/readyz` | `ready` |
| Interno no expuesto | `curl -i https://$DOMAIN/internal/v1/jobs/claim` | `404` |
| Webhook | `curl https://api.telegram.org/bot$TOKEN/getWebhookInfo` | `url` = `https://$DOMAIN/telegram/webhook`, sin `last_error_message` |
| Bot | En Telegram envía `/start` | Respuesta con botón "Abrir check-in" |
| Mini App | Toca el botón | Abre `/hoy` autenticado |
| Web | Abre `https://$DOMAIN` → Iniciar sesión con Telegram | Vuelve autenticado |

### 4. Actualizar
```bash
git pull && docker compose -f infra/compose.prod.yml --env-file .env.prod up -d --build
```
Apagado ordenado: los contenedores reciben SIGTERM; server y bot terminan peticiones y trabajos en curso. Un job
interrumpido conserva su lease y se reintenta al vencer.

### 5. Respaldos
```bash
# Respaldo diario (cron del host):
0 3 * * * cd /ruta/dane && docker compose -f infra/compose.prod.yml --env-file .env.prod exec -T db pg_dump -U dane -Fc dane > backups/dane-$(date +\%F).dump
# Restaurar en una BD nueva (nunca sobre datos sin --force):
docker compose -f infra/compose.prod.yml --env-file .env.prod exec -T db createdb -U dane dane_restore
docker compose -f infra/compose.prod.yml --env-file .env.prod exec -T db pg_restore -U dane --no-owner -d dane_restore < backups/dane-AAAA-MM-DD.dump
```
Copia los respaldos fuera del VPS. La prueba automatizada del procedimiento es `scripts/test-backup-restore.sh`
(parte de `pnpm verify`). `ENCRYPTION_KEY` debe respaldarse aparte (gestor de contraseñas): sin ella las
credenciales OAuth guardadas no se pueden descifrar (basta con reconectar las integraciones).

### 6. Rotación y revocación
* `INTERNAL_API_TOKEN`: cambia en `.env.prod` y reinicia server y bot juntos.
* Sesiones: "Cerrar sesión en todos los dispositivos" en Ajustes, o `UPDATE sessions SET revoked_at = now();`.
* Quitar acceso: elimina el ID de `ALLOWED_TELEGRAM_IDS` y revoca sesiones.
* Token del bot comprometido: `/revoke` en @BotFather, actualiza `TELEGRAM_BOT_TOKEN`, reinicia.

### 7. Logs
JSON estructurado (`LOG_FORMAT=json`) sin secretos ni query strings: `docker compose … logs -f server bot`.
