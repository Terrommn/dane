# Reporte de pruebas

Fecha: 2026-11-12 · Máquina: macOS 13 (arm64), Rust 1.94.1, Node 24.14, pnpm 12.18, PostgreSQL 17 nativo,
Google Chrome 157 (Playwright 1.65 no trae Chromium para macOS 13; se usa `channel: "chrome"`).
Comando: `pnpm verify` → tabla de pasos con código de salida (logs en `.data/verify/`).

## Resultado de `pnpm verify`
| Paso | Resultado | Detalle |
|---|---|---|
| rust fmt | ✅ aprobado | `cargo fmt --all -- --check` |
| rust clippy | ✅ aprobado | `--workspace --all-targets -D warnings` |
| rust tests | ✅ aprobado | **215 pruebas, 0 fallos** (PostgreSQL real, una BD efímera por prueba) |
| contrato OpenAPI | ✅ aprobado | 160 rutas; sin schemas ni `operationId` duplicados; cliente TS al día |
| web biome | ✅ aprobado | 141 archivos |
| web typecheck | ✅ aprobado | TypeScript 7 estricto |
| web unit tests | ✅ aprobado | **137 aprobadas**, 1 omitida (contrato en vivo, requiere `DANE_LIVE_API`) |
| web build | ✅ aprobado | un solo build Mini App/web/PWA; SW con 90 entradas precacheadas, sin `/api` |
| web e2e (Playwright) | ✅ aprobado | **14 pruebas** en móvil (Pixel 7) y escritorio (1280×800) |
| backup/restore | ✅ aprobado | respaldo y restauración en BD temporal con verificación de datos |

## Rust por suite
| Suite | Pruebas | Cubre (requisito §11) |
|---|---|---|
| `domain` (unitarias) | 79 | semanas ISO W53/W01, DST hueco/ambiguo, 23 h, medianoche, cambio de zona; días 29–31/último/penúltimo; dinero exacto y FX; cumplimiento con día abierto, sin registro, no aplica, N/semana, pausas, cambio de frecuencia, rachas, ritmo y proyección solo acumulativa; recordatorios por fecha local; parser del vault (frontmatter, tablas con pipes escapados, mojibake); finanzas |
| `server` (unitarias) | 23 | initData (firma, antigüedad, alterado), cifrado AES-GCM, redirect seguro, config (dev-login imposible en producción) |
| `auth_isolation` | 9 | Mini App válida/vencida/firma ajena/fuera de allowlist; sesión revocada; CSRF; **acceso cruzado entre dos usuarios rechazado**; renombrar conserva ID; 409 por versión; Idempotency-Key |
| `auth_oidc` | 5 | OIDC con IdP simulado RS256/JWKS/PKCE: flujo válido, **misma identidad que la Mini App** (`sub`≠`id`), nonce/aud/iss/exp/firma/claim inválidos, state de un uso, allowlist, open redirect, rate limit |
| `core_goals_habits` | 9 | acción enlazada a dos objetivos aporta una vez a cada uno; libros vs minutos sin mezclar unidades; denominadores; 4/semana; pausas; aislamiento |
| `core_checkins` | 6 | guardado parcial; 409 entre dispositivos; **check-in repetido con Idempotency-Key sin duplicar**; vínculos explícitos peso/reflexión; devocional |
| `core_dashboard_weekly` | 3 | fallas devocionales vs máximo, peso sin interpolar, comparaciones mismo tramo/completo, resumen semanal |
| `finance_flows` | 12 | dinero exacto; lote idempotente por elemento; **compra con tarjeta + pago + transferencia sin doble conteo**; conciliación del mismo día y retroactivos; semántica legacy; FX; préstamo familiar; deuda saldada → hito; recurrentes 29–31 concurrentes; confirmar sin duplicar; sugerencia de tarjeta; presupuestos/rubros |
| `modules_tasks` | 6 | arrastre conserva ID; recurrentes feb bisiesto/no bisiesto, catch-up y concurrencia; ClickUp no se modifica |
| `modules_people_inbox` | 3 | alias sin acentos, cumpleaños 29-feb, sugerencias no crean personas; Inbox no mueve sin confirmar |
| `modules_knowledge` | 6 | búsqueda sin acentos aislada; enlaces ajenos rechazados; backlinks; favoritos; archivados |
| `modules_content_dev_school` | 5 | hooks ≤10, URLs peligrosas rechazadas, ADR numeración concurrente y reemplazo, devlog importado |
| `telegram_flow` | 16 | update duplicado/pendiente nunca perdido; efectos y respuesta en la misma transacción con rollback; **dos workers sin compartir job**; lease vencido; **ack obsoleto 409**; ack idempotente; **429**; bloqueo permanente; recordatorio local 21:00; cancelado si check-in cerrado; **sin avalancha tras caída**; cambio de zona; pausa |
| `integrations_google` | 4 | OAuth PKCE, calendarios, festivos excluidos, resync sin duplicar, Gmail sin automáticos, 401 → error |
| `integrations_clickup` | 3 | solo la vista, orden fáciles/resto/need info, cerradas, sin escritura |
| `integrations_captures` | 5 | salida LLM inválida rechazada, inyección de prompt sin efecto, preguntas de campos ambiguos, aplicar idempotente, modo manual |
| `integrations_import` | 3 | **importación idempotente**, plantillas/ejemplos ignorados, conflictos, **ZIP peligroso rechazado**, **export → reimport conserva vínculos** sin secretos |
| `hooks_contract` | 3 | vínculo acción→hábito de una sola ocurrencia; aporte único por fuente; FK de dueño |
| `demo_data` | 2 | demo aislado del usuario real; 404 si `DEMO_ENABLED=false` |
| `e2e_flow` | 1 | **crear meta → /start → recordatorio 21:00 con fecha → check-in (reintento offline) → mismo progreso en dashboard → recordatorio cancelado** |
| `telegram-bot` (unit + `bot.rs` + `e2e_server`) | 4 + 5 + 1 | polling no avanza offset sin aceptación; webhook con secreto, 503 si el server no confirma; envío/429/ack; server y bot reales por HTTP con Telegram simulado |
| `contracts`, `persistence` | 1 + 1 | serialización, backoff |

## Web
* **Vitest (137):** cola offline (orden, reintento con la misma clave, 409, sin red), particionado por usuario y limpieza al
  salir, middleware de API (Bearer/CSRF/Idempotency), start_param, dinero sin floats, formularios de finanzas, merge
  parcial del check-in, estados de hábitos, Markdown seguro (sin HTML ni `javascript:`), componentes clave.
* **Playwright (14, Chrome real contra el server Rust real sirviendo el build):**
  | Prueba | Móvil | Escritorio |
  |---|---|---|
  | check-in: guardar, recargar, mismo dato en server y dashboard | ✅ | ✅ |
  | sin conexión: pendiente → **reabrir sin red con el borrador** → reconectar → guardado | ✅ | ✅ |
  | datos locales no se filtran entre usuarios | ✅ | ✅ |
  | Mini App: initData firmado → Bearer en memoria, sin cookies ni tokens en storage, fecha objetivo | ✅ | ✅ |
  | Mini App: initData alterado no inicia sesión | ✅ | ✅ |
  | **revisión responsive de 29 pantallas**: sin errores, sin "pendiente", sin scroll horizontal (capturas en `apps/web/test-results/screens/`) | ✅ | ✅ |
  | espacio demo poblado y aislado en 12 módulos | ✅ | ✅ |

## Actualización 2026-11-23 (Railway, rediseño y asistente diario)
* Rust: pruebas nuevas del asistente (`telegram_flow.rs`: menú por saludo, resumen de la mañana, mañana omitida si
  ya hay check-in, aviso de evento; `scheduler.rs`: texto de la mañana; `reminders.rs`: tipo `Morning`). Las llaves
  RSA de `auth_oidc.rs` ya no viven solo en una máquina: `scripts/gen-test-keys.mjs` las genera (lo llama
  `pnpm verify`). Hasta este cambio CI fallaba al compilar `auth_oidc` porque esos archivos no estaban en el repo.
* Web: 140 pruebas unitarias pasan (incluye `home.test.ts`: saludo por zona horaria, racha, resumen del día).
* Verificado en producción: login web OIDC, Mini App, `/start` y menú, check-in guiado de la mañana (energía, peso,
  marcar como hecho) e interruptores de hábitos en Inicio.
* Animación de Inicio medida con `requestAnimationFrame` (9.5 s de ciclo): mediana 5.6 ms por cuadro, p99 11.2 ms,
  0 cuadros > 50 ms.

## Bugs encontrados por las pruebas y corregidos
* Limpieza de flujos OIDC con `now()` de la BD en vez del reloj de la app (borraba el flujo recién creado).
* Schemas OpenAPI homónimos y `operationId` duplicados que tipaban mal el cliente (ahora `pnpm verify` lo impide).
* La PWA no abría sin red (fallaba `/me`): ahora abre con el último usuario del dispositivo, usa la última vista del
  día guardada en su IndexedDB y revalida al reconectar.
* Rubros de presupuesto perdían el vínculo con gastos al editarse (IDs nuevos) — agente de finanzas.
* Orden de la cola offline con dos operaciones en el mismo milisegundo — agente web.
* Hoy mostraba "Borrador" aunque hubiera conflicto/error — agente web.
* Login OIDC en producción fallaba con "Failed to parse server response": Telegram envía el JWKS con gzip (D-035).
* Tirones en Inicio al reiniciar la animación y por `backdrop-filter` sobre el fondo animado (D-037).

## No ejecutado / requiere cuentas o dispositivos reales (ver `USER_SETUP.md`)
| Prueba | Motivo |
|---|---|
| Webhook y 429/403 reales | producción usa polling; el bot real (comandos, menú, recordatorios) sí se verificó en Railway |
| PWA instalada en iOS/Android | requiere dispositivo; se probó SW/offline en Chrome de escritorio y emulación móvil |
| Google, Gmail, ClickUp, Anthropic reales | requieren tus cuentas; probados con servidores HTTP simulados (wiremock) |
| Compose / Caddy (VPS) | No probado. Las imágenes de producción sí se construyen en Railway (`Dockerfile.railway`, `Dockerfile.railway.bot`); CI corre `pnpm verify`, no construye imágenes |
| WebKit/Firefox | Playwright 1.65 en macOS 13 solo con Chrome del sistema |
