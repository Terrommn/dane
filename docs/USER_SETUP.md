# Configuración que te toca hacer a ti

Todo lo de esta página requiere tus cuentas, dispositivos o decisiones de gasto, por eso no está automatizado.
El resto (código, migraciones, pruebas) ya está hecho y verificado con `pnpm verify`.
Convención: `DOMAIN` es tu dominio público (ej. `vida.tudominio.com`); `PUBLIC_BASE_URL=https://DOMAIN`.

## 0. Probar todo localmente (sin cuentas externas)
```bash
cd ~/Developer/Coding/vida
pnpm install
pnpm dev            # PostgreSQL local :5433 + API :8080 + web :5180 (+ bot si pones token)
```
* Abre http://localhost:5180 → **Entrar en modo desarrollo** con el ID `111` (tu cuenta de pruebas), o
  **Explorar con datos demo** para ver todos los módulos poblados con datos ficticios marcados `[DEMO]`.
* Para usar tu ID real de Telegram en local, ponlo en `.env` → `ALLOWED_TELEGRAM_IDS=111,<tu_id>`.
* Verificación completa: `pnpm verify` (Rust, contrato, web, E2E con Chrome, backup/restore).
* Sin Docker: `brew install postgresql@17` (ya hecho en esta Mac). Con Docker: `USE_DOCKER_DB=1 pnpm dev`.

Comprobación: la pantalla **Hoy** guarda un check-in ("✓ Guardado"), **Dashboard** lo refleja y `/ajustes` muestra
"Telegram: chat no conectado" hasta completar el paso 2.

## 1. Tu ID de Telegram (allowlist)
1. En Telegram, escribe a `@userinfobot` (o `@getmyid_bot`) y copia tu **Id** numérico.
2. `ALLOWED_TELEGRAM_IDS=<tu_id>` (separa varios con comas). En producción no puede quedar vacío.

Comprobación: con un ID fuera de la lista, la Mini App y el login web responden "Esta cuenta de Telegram no tiene acceso".

## 2. Bot de Telegram
1. `@BotFather` → `/newbot` → nombre y usuario (termina en `bot`). Copia el token.
2. Variables: `TELEGRAM_BOT_TOKEN=<token>`, `TELEGRAM_BOT_USERNAME=<usuario_sin_@>`.
3. Los comandos (`/start`, `/daily`, `/checkin`, `/objetivos`, `/dump`, `/gasto`, `/ingreso`, `/weekly_review`,
   `/log_session`, `/dashboard`, `/settings`, `/help`, `/pause`, `/resume`) los registra el bot solo al arrancar
   (`setMyCommands`).
4. **Local (polling):** `TELEGRAM_MODE=polling` en `.env` y `pnpm dev`. Escribe `/start` al bot.
   Los botones serán enlaces normales a `http://localhost:5180/...` (Telegram solo abre Mini Apps con **https**).
5. **Producción (webhook):** lo configura `infra/compose.prod.yml` (`TELEGRAM_MODE=webhook`,
   `WEBHOOK_PUBLIC_URL=https://DOMAIN/telegram/webhook`, `WEBHOOK_SECRET=$(openssl rand -hex 24)`). El bot borra el
   webhook en modo polling y lo registra en modo webhook; nunca ambos.

Comprobación: `/start` responde con un botón **Abrir check-in**; en `/ajustes` aparece "chat conectado".
Webhook: `curl https://api.telegram.org/bot<TOKEN>/getWebhookInfo` → `url` correcta y sin `last_error_message`.

## 3. Mini App (dentro de Telegram)
Requiere HTTPS público (producción o túnel).
1. `@BotFather` → tu bot → **Bot Settings → Configure Mini App** → URL `https://DOMAIN/hoy`.
2. Opcional: **Menu Button** → `https://DOMAIN/hoy` con texto "Check-in".
3. Para probar desde tu Mac sin servidor: túnel HTTPS (p. ej. `cloudflared tunnel --url http://localhost:5180`),
   pon `PUBLIC_BASE_URL=<url del túnel>` y `COOKIE_SECURE=true`, desactiva `DEV_LOGIN_ENABLED` (el arranque lo exige
   con un origen no local) y reinicia `pnpm dev`.

Comprobación: abre el bot en el celular → botón de menú → la app abre en **Hoy** ya autenticada, con el tema de
Telegram. Un botón de un recordatorio viejo abre ese día y muestra "Estás editando el check-in del …".

## 4. Inicio de sesión web/PWA (Telegram Login · OIDC)
1. `@BotFather` → tu bot → **Login Widget** (Bot Settings).
2. Allowed origins: `https://DOMAIN`. Redirect URI exacto: `https://DOMAIN/api/v1/auth/oidc/callback`.
3. Copia **Client ID** y **Client Secret**: `OIDC_CLIENT_ID=…`, `OIDC_CLIENT_SECRET=…` (issuer por defecto
   `https://oauth.telegram.org`).

Comprobación: en el navegador, `https://DOMAIN/login` → **Iniciar sesión con Telegram** → vuelves autenticado y
`/ajustes` muestra las dos identidades (Mini App y web) en el **mismo** usuario.

## 5. PWA instalada
* iPhone (Safari): Compartir → **Agregar a inicio**. Android (Chrome): menú → **Instalar app**. Escritorio (Chrome/Edge):
  ícono de instalar en la barra de direcciones. Instrucciones también en `/instalar`.

Comprobación: abre la app instalada, activa modo avión, edita el check-in → "Pendiente de sincronizar"; cierra y abre
la app sin red → el borrador sigue; quita modo avión → "✓ Guardado".

## 6. Google Calendar y Gmail (opcional, solo lectura)
1. https://console.cloud.google.com → proyecto nuevo → **APIs y servicios**: habilita *Google Calendar API* y *Gmail API*.
2. **Pantalla de consentimiento OAuth**: tipo *Externo*, en modo *Testing*, agrega tu correo como usuario de prueba.
   Scopes: `openid`, `email`, `.../auth/calendar.readonly`, `.../auth/gmail.readonly`.
3. **Credenciales → ID de cliente OAuth → Aplicación web**. Redirect URI exacto:
   `https://DOMAIN/api/v1/integrations/google/callback` (en local: `http://localhost:5180/api/v1/integrations/google/callback`).
4. `GOOGLE_CLIENT_ID=…`, `GOOGLE_CLIENT_SECRET=…`.
5. En la app: **Ajustes → Integraciones → Conectar Google**, elige calendarios.

Nota: con la app en modo *Testing*, Google vence el refresh token cada 7 días; reconecta en Integraciones cuando la
app marque "error" (Gmail es un scope restringido: publicarla exigiría verificación de Google).
Comprobación: "Sincronizar ahora" muestra eventos en la Agenda de Hoy (sin festivos) y sugerencias de personas.

## 7. ClickUp (opcional, solo lectura)
1. ClickUp → avatar → **Settings → Apps → API Token** → genera un token personal (`pk_…`).
2. Abre la vista de tareas que quieres usar; el `view_id` es la parte final de la URL (`…/v/l/<VIEW_ID>`).
3. En la app: **Ajustes → Integraciones → ClickUp**: token, `view_id`, tu ID de usuario y mapa de estados
   (`new` → abierta, `need info` → bloqueada, cerradas → hecha).

Nota: desconectar borra el token de la app, pero ClickUp no permite revocarlo por API: elimínalo también en ClickUp.
Comprobación: "Sincronizar" lista primero 3–5 tareas fáciles con su motivo, luego el resto y al final "need info".

## 8. Asistente de captura con IA (opcional)
1. https://console.anthropic.com → API Keys → crea una clave.
2. `ANTHROPIC_API_KEY=…` (modelo por defecto `LLM_MODEL=claude-haiku-4-5-20251001`).

Sin clave todo funciona en modo manual. Comprobación: `/gasto 250 gasolina ayer` en el bot → botón **Revisar** →
propuesta extraída con monto y fecha; nada se guarda hasta **Aplicar**.

## 9. Producción
Sigue [`docs/DEPLOY.md`](DEPLOY.md): VPS con Docker, DNS `A` de `DOMAIN`, puertos 80/443, `.env.prod` con
`DOMAIN`, `ACME_EMAIL`, `POSTGRES_PASSWORD`, `ALLOWED_TELEGRAM_IDS`, `INTERNAL_API_TOKEN` (`openssl rand -hex 32`),
`ENCRYPTION_KEY` (`openssl rand -base64 32`, respáldala en tu gestor de contraseñas), `TELEGRAM_BOT_TOKEN`,
`TELEGRAM_BOT_USERNAME`, `WEBHOOK_SECRET` y las opcionales de los pasos 4, 6 y 8. Luego:
```bash
docker compose -f infra/compose.prod.yml --env-file .env.prod up -d --build
```
Coste: necesitas un VPS siempre encendido (los recordatorios dependen de que API y bot estén activos) y un dominio.

## 10. Importar tu vault real (opcional)
**Importar** → sube un ZIP de `~/Obsidian` → revisa la simulación (qué se crea, advertencias, conflictos) → **Aplicar**.
Plantillas, comandos, scripts y valores de ejemplo no se importan. Puedes repetirla sin duplicar.

## Verificaciones pendientes que requieren tus cuentas o dispositivos
| ID | Qué falta | Cómo hacerlo |
|---|---|---|
| X1 | Bot real: `setMyCommands`, `/start`, recordatorio a las 21:00 en tu chat | Pasos 1–2 y esperar el horario (o cambiar la hora en Ajustes) |
| X2 | Mini App en cliente real (iOS/Android/Desktop) con initData real, tema y safe areas | Paso 3 |
| X3 | Webhook en producción (`getWebhookInfo` sin errores) y 429/403 reales | Pasos 2 y 9 |
| X4 | Telegram Login OIDC real (consentimiento, redirect, misma identidad) | Paso 4 |
| X5 | PWA instalada en tu iPhone/Android: abrir sin red y sincronizar | Paso 5 |
| X6 | Google Calendar/Gmail con tu cuenta | Paso 6 |
| X7 | ClickUp con tu vista | Paso 7 |
| X8 | Extracción con la API real de Anthropic | Paso 8 |
| X9 | Construir y levantar las imágenes Docker (`compose.prod.yml`) y HTTPS de Caddy | Paso 9 (esta Mac no tiene Docker) |
| X10 | Importar tu vault real | Paso 10 |
