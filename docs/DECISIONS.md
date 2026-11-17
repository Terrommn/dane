# Decisiones y supuestos

Formato: ID · decisión · motivo · alternativas · estado. Se revisan al reanudar trabajo.

| ID | Decisión | Motivo | Alternativas | Estado |
|---|---|---|---|---|
| D-001 | Repositorio nuevo en `~/Developer/Coding/dane`, proyecto Obsidian **personal** | No había repositorio destino; la app es de vida personal | Otro nombre/ubicación | aceptada (reversible) |
| D-002 | Herramienta de tareas: `TaskCreate/TaskUpdate/TaskList` del entorno (equivalente a TodoWrite, que no está expuesto). Número de tarea `#N` = `TN`. `blocked_external` se mantiene `pending` allí y se documenta en `TASKS.md` | El entorno no expone TodoWrite | — | aceptada |
| D-003 | Versiones fijadas (nov 2026): Rust 1.94 (Homebrew), axum 0.8, sqlx 0.9, teloxide 0.17, reqwest 0.13, utoipa 6, openidconnect 4.1, chrono-tz 0.10, Node 24, pnpm 12, PostgreSQL 17 | Verificadas con `cargo search`/`npm view`; lockfiles versionados | — | aceptada |
| D-004 | Sin Docker en la máquina de desarrollo: `scripts/dev-db.sh` usa PostgreSQL nativo (Homebrew); Compose se entrega para quien tenga Docker y para producción | Docker no está instalado | Instalar Docker Desktop | aceptada |
| D-005 | Sesión Mini App por **Bearer en memoria** (no cookies) | Las cookies en WebViews de Telegram no son confiables; se re-autentica con `initData` al recargar | Cookie SameSite=None | aceptada |
| D-006 | Sesión web/PWA por cookie HttpOnly Secure SameSite=Lax + CSRF double-submit | Requisito; evita tokens en JS | JWT en localStorage (prohibido) | aceptada |
| D-007 | SQLx con consultas en tiempo de ejecución (`query_as`) y pruebas de integración contra PostgreSQL real | Las macros `query!` exigen BD en compilación/archivos `.sqlx`; con muchos agentes en paralelo frena el trabajo. Las pruebas cubren las consultas | `query!` + `cargo sqlx prepare` | aceptada |
| D-008 | Ownership con FK compuesta `(user_id, id)` entre tablas propias; enlaces polimórficos validados en el server | Garantía en la BD de que no se cruzan dueños | Solo validación en código | aceptada |
| D-009 | Dinero en centavos `BIGINT` + `currency CHAR(3)`; tasas `NUMERIC(18,8)`; `rust_decimal` solo para FX | Exactitud sin floats | NUMERIC para todo | aceptada |
| D-010 | Hábitos con identidad estable + `habit_versions` (vigencia) + `habit_pauses`; entradas por fecha. Sin fila = sin registro | Editar frecuencia no reescribe el pasado | Copiar checkboxes (vault) | aceptada |
| D-011 | Outbox en PostgreSQL (`jobs`) con leases y `SKIP LOCKED`; sin colas externas | Requisito; basta PostgreSQL | Redis/NATS | aceptada |
| D-012 | El bot procesa comandos en el server (reglas de negocio) a partir de updates persistidos; el bot solo transporta | Reglas en el server; durabilidad | Lógica en el bot | aceptada |
| D-013 | LLM: adaptador Anthropic Messages API con salida JSON validada contra esquemas Rust; modelo por defecto `claude-haiku-4-5-20251001` configurable (`LLM_MODEL`) | Adaptador real, barato para extracción | OpenAI | aceptada |
| D-014 | Credenciales OAuth cifradas con AES-256-GCM (`ENCRYPTION_KEY` 32 bytes base64) en `integration_connections` | Requisito de cifrado en servidor | KMS externo | aceptada |
| D-015 | Búsqueda global con tabla `search_index` alimentada por triggers + `unaccent` + `pg_trgm` | Tolerante a acentos sin servicio extra | Meilisearch | aceptada |
| D-016 | Proxy de producción: Caddy (HTTPS automático) con mismo origen para web y API; `/internal/*` solo en red interna | Portátil, sin servicio pagado | Nginx | aceptada |
| D-017 | Semana ISO lunes–domingo fija para periodos ISO; `week_start` del usuario solo afecta presentación | Periodos ISO estables | — | aceptada |
| D-018 | DST: hora local inexistente → primera hora válida posterior; ambigua → la más temprana | Regla explícita | — | aceptada |
| D-019 | Cumplimiento: día abierto (hoy, aún no cerrado) excluido del denominador "cerrado" y mostrado como avance de hoy; días cerrados sin registro cuentan como exigibles pero se marcan "sin registro" | Requisito §5 | — | aceptada |
| D-020 | Importación legacy de cuentas: snapshot con semántica `legacy_day_start_inclusive` (incluye movimientos del día base); tarjetas `legacy_after_day` (desde el día siguiente). Nuevos snapshots: instante `observed_at`, se incluyen movimientos con `occurred_at > observed_at`, o, sin hora, con `occurred_on > fecha local del snapshot` salvo marca explícita `after_snapshot` | Conservar semántica original sin mezclarla | — | aceptada |
| D-021 | Préstamo familiar: el gasto se registra como consumo con `financed_by_person_id`; si el usuario activa seguimiento, se crea una deuda `kind=family` cuyos abonos son `debt_payment` (no consumo) | Vault §5 de gasto.md | — | aceptada |
| D-022 | Tarjeta sugerida: excluye `cancelled`/`to_cancel`, sin día de corte, excluidas por el plan, con deuda prohibida por el plan, sin disponible para el monto, o con uso resultante > límite configurado (30% por defecto), o corte hoy; elige más días hasta el próximo corte; sin monto → provisional; ninguna → débito configurado | daily.md §3b | — | aceptada |
| D-023 | Excalidraw (`@excalidraw/excalidraw`) cargado con `React.lazy` | Notas visuales básicas | tldraw | aceptada |
| D-024 | Datos demo: usuarios con `is_demo=true` creados solo por `POST /api/v1/demo/seed` en desarrollo o por opt-in explícito en onboarding a un usuario demo separado; nunca mezclados con el usuario real | Aislamiento | — | aceptada |
| D-025 | Las filas de repositorio (`FromRow`) derivan `Serialize + ToSchema` y son los DTO de respuesta; los cuerpos de entrada son structs propios del server | Evita duplicar ~60 structs; el contrato OpenAPI sigue generándose del server | DTOs separados | aceptada |
| D-026 | Al crear un usuario se siembran solo los valores iniciales explícitos del requisito (preferencias es-MX/America/Monterrey/MXN, 4 áreas con proyecto, catálogo de categorías). Hábitos fijos, metas (peso, ejercicio, fallas) y presupuestos se **proponen** en onboarding y se crean solo al confirmar | No importar ejemplos como hechos | Sembrar todo | aceptada |
| D-027 | Snapshot `instant` y movimiento del mismo día sin hora: cuenta como posterior solo si se registró (`created_at`, reloj de la app) después de `observed_at` | Refinamiento de D-020 para conciliación del mismo día | — | aceptada |
| D-028 | El vínculo "registrar gasto → hábito" marca la fecha en que se registra (hoy local), no la fecha del gasto | El hábito mide el acto de registrar | Fecha del gasto | aceptada |
| D-029 | Importación: archivo modificado tanto en el vault como en la app → conflicto visible, no se sobrescribe; el usuario decide editando en la app | Nunca pisar datos recientes | Último gana | aceptada |
| D-030 | Suscripciones, ingresos fijos y pendientes recurrentes importados empiezan el mes siguiente | El vault ya registró las ocurrencias pasadas | Recalcular históricos | aceptada |
| D-031 | Calendar sincroniza por ventana (ayer → +7 días) con paginación; sin `syncToken` porque Google no lo combina con `timeMin/timeMax` | Limitación del proveedor | syncToken global | aceptada |
| D-032 | LLM vía HTTP directo a Messages API (no hay SDK oficial de Rust) con salida restringida por JSON Schema y validación posterior en Rust | Adaptador real y validado | — | aceptada |
