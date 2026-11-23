# CLAUDE.md — reglas para trabajar en Dane

Guía para cualquier agente (Claude, Codex, etc.) o persona que haga cambios en este repo. Léela antes de empezar.
Contexto técnico: [`README.md`](README.md), [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md),
[`docs/CONVENTIONS.md`](docs/CONVENTIONS.md) y [`docs/DECISIONS.md`](docs/DECISIONS.md).

## 1. Cada cambio actualiza la documentación (obligatorio)
Un cambio no está terminado hasta que la documentación dice lo mismo que el código. En **el mismo PR**:

| Si cambias… | Actualiza |
|---|---|
| Una función visible (pantalla, comando del bot, mensaje automático, ajuste) | `README.md` (qué hace la app) y, si el usuario debe configurar algo, `docs/USER_SETUP.md` |
| Arquitectura, flujos, servicios o dependencias entre piezas | `docs/ARCHITECTURE.md` |
| Tablas, columnas o migraciones | `docs/DATA_MODEL.md` |
| Una decisión con alternativas (librería, enfoque, proveedor) | Nueva fila `D-0NN` en `docs/DECISIONS.md` |
| Despliegue, variables de entorno, Dockerfiles, Railway | `docs/DEPLOY.md` y `.env.example` (solo el **nombre** de la variable y un valor de ejemplo falso) |
| Pantallas o estructura de `apps/web` | `apps/web/README.md` |
| Pruebas nuevas o resultados de `pnpm verify` | `docs/TEST_REPORT.md` |
| Trabajo pendiente o terminado | `docs/TASKS.md` |
| Estas reglas | este `CLAUDE.md` |

Si un documento queda obsoleto y nadie lo usa, bórralo en vez de dejarlo desactualizado.

## 2. Sin datos personales en el repo
El repo lo trabajan varias personas y puede volverse público. **Nunca** subas:
* Nombres, correos, teléfonos ni usuarios reales (tampoco en pruebas, mocks, capturas ni comentarios).
  En pruebas usa `Usuario` / `usuario` / `usuario@example.com` y los IDs de Telegram ficticios `111` y `222`.
* IDs reales de Telegram, IDs de chat, usuario o ID del bot de producción.
* Rutas de una computadora concreta (`/Users/<alguien>/…`, `C:\Users\…`); usa rutas relativas al repo.
* URLs privadas de producción, paneles o enlaces de invitación.

Los autores de los commits (metadatos de git) son la única excepción.

## 3. Nunca subir secretos ni variables de entorno
* `.env`, `.env.*` (salvo `.env.example`), `*.pem`, `*.key` y credenciales están en `.gitignore`. No lo debilites.
* `.env.example` lleva **solo valores de desarrollo falsos o vacíos**. Si agregas una variable, agrégala ahí sin su valor real.
* Los valores reales viven únicamente en **Railway → servicio → Variables** (producción) y en tu `.env` local.
* No pegues tokens, claves ni contraseñas en código, pruebas, docs, mensajes de commit, PRs ni logs.
* Antes de cada commit revisa `git diff --cached`. CI corre un escaneo de secretos (gitleaks) en cada PR y push a
  `main`; si falla, **no** lo silencies: quita el secreto, rótalo (BotFather, Railway…) y reescribe la rama.
* No subas archivos generados localmente que no sean del proyecto (por ejemplo, cambios de `pnpm-lock.yaml`
  que no vengan de un cambio real de dependencias).

## 4. Commits y PRs
* **Sin atribución de IA**: nada de `Co-Authored-By: Claude`, "Generated with …" ni similares en commits o PRs.
* Mensajes en español, cortos y en imperativo o descriptivos ("Inicio: …", "Bot: …").
* Nunca commits directos a `main`: rama → PR → merge. **Cada merge a `main` publica en producción** (ver §5).

## 5. Despliegue (Railway)
* Producción corre en Railway con tres servicios: `server` (API + web), `bot` (Telegram en modo polling) y `Postgres`.
* `server` y `bot` están conectados a GitHub, rama `main`: **cada merge a `main` reconstruye y publica** solo, sin
  caída (la versión anterior sigue hasta que la nueva arranca). Las migraciones corren solas al arrancar el server.
* Recetas: `infra/Dockerfile.railway` (server + web) e `infra/Dockerfile.railway.bot` (bot), elegidas con la variable
  `RAILWAY_DOCKERFILE_PATH` de cada servicio.
* Detalle completo y cómo agregar variables: [`docs/DEPLOY.md`](docs/DEPLOY.md).

## 6. Desarrollo local: cuidado con el bot compartido
* Producción usa el bot en modo **polling**. Telegram solo permite **un** proceso leyendo updates por bot: si corres
  el bot local con el **mismo token** que producción, los dos se pelean los mensajes.
* Para desarrollo usa **otro bot** (crea uno de pruebas en @BotFather) o deja `TELEGRAM_BOT_TOKEN` vacío en tu
  `.env`. `pnpm dev` arranca el bot si hay token.
* Pregunta siempre en qué ambiente (producción / desarrollo) antes de ejecutar algo que modifique datos.

## 7. Antes de abrir un PR
```bash
pnpm verify        # fmt, clippy, pruebas Rust con PostgreSQL, contrato OpenAPI, Biome, typecheck, pruebas web, E2E
pnpm gen:api       # si cambiaste la API del server (regenera packages/api-client)
```
Si algo no se pudo correr (por ejemplo, falta una cuenta externa), dilo en el PR.
Código e identificadores en inglés; textos visibles y documentación en español. Los comentarios explican el
**por qué**, no solo el qué.
