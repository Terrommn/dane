# Runbook de despliegue (portable)

No depende de ningún proveedor pagado específico: cualquier VPS Linux con Docker (≥1 vCPU, 1 GB RAM, 10 GB disco)
y un dominio con DNS. API, bot y PostgreSQL deben quedar **siempre encendidos** para cumplir los horarios de
recordatorios (no uses planes que duermen los servicios).

## 1. Preparar el servidor
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

## 2. Levantar
```bash
docker compose -f infra/compose.prod.yml --env-file .env.prod up -d --build
docker compose -f infra/compose.prod.yml --env-file .env.prod ps        # server y bot "healthy"
curl -fsS https://$DOMAIN/healthz                                        # → ok
```
El server aplica migraciones al arrancar (`dane-server migrate` también existe para hacerlo explícitamente). El bot en
modo `webhook` registra `https://$DOMAIN/telegram/webhook` con el secreto y elimina cualquier polling.

## 3. Comprobaciones posteriores
| Qué | Cómo | Esperado |
|---|---|---|
| API viva | `curl https://$DOMAIN/readyz` | `ready` |
| Interno no expuesto | `curl -i https://$DOMAIN/internal/v1/jobs/claim` | `404` |
| Webhook | `curl https://api.telegram.org/bot$TOKEN/getWebhookInfo` | `url` = `https://$DOMAIN/telegram/webhook`, sin `last_error_message` |
| Bot | En Telegram envía `/start` | Respuesta con botón "Abrir check-in" |
| Mini App | Toca el botón | Abre `/hoy` autenticado |
| Web | Abre `https://$DOMAIN` → Iniciar sesión con Telegram | Vuelve autenticado |

## 4. Actualizar
```bash
git pull && docker compose -f infra/compose.prod.yml --env-file .env.prod up -d --build
```
Apagado ordenado: los contenedores reciben SIGTERM; server y bot terminan peticiones y trabajos en curso. Un job
interrumpido conserva su lease y se reintenta al vencer.

## 5. Respaldos
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

## 6. Rotación y revocación
* `INTERNAL_API_TOKEN`: cambia en `.env.prod` y reinicia server y bot juntos.
* Sesiones: "Cerrar sesión en todos los dispositivos" en Ajustes, o `UPDATE sessions SET revoked_at = now();`.
* Quitar acceso: elimina el ID de `ALLOWED_TELEGRAM_IDS` y revoca sesiones.
* Token del bot comprometido: `/revoke` en @BotFather, actualiza `TELEGRAM_BOT_TOKEN`, reinicia.

## 7. Logs
JSON estructurado (`LOG_FORMAT=json`) sin secretos ni query strings: `docker compose … logs -f server bot`.
