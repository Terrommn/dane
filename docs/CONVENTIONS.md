# Convenciones para trabajo en paralelo

Lee también `docs/ARCHITECTURE.md`, `docs/DATA_MODEL.md`, `docs/DECISIONS.md` y `docs/VAULT_REQUIREMENTS.md`.
Código e identificadores en inglés; textos visibles, mensajes de error y documentación de uso en **español**.

## Propiedad de archivos

| Agente | Tareas | Archivos propios |
|---|---|---|
| lead | integración, auth, base | `apps/server/src/{lib,main,config,error,crypto,state,workers}.rs`, `auth/`, `http/`, `api/{mod,areas,settings}.rs`, `persistence/{lib,hooks}.rs`, `repos/{identity,areas}.rs`, migraciones `0001–0006`, `domain/{time,money,text}.rs`, `tests/common/`, `docs/*` (salvo lo indicado) |
| A-core | T17–T20, T22, T23 (API), T26, T28, T29 | `api/{goals,habits,checkins,dashboard,weekly,readings}.rs`, `repos/{goals,habits,checkins,readings,weekly,dashboard}.rs`, `domain/{habits,goals,metrics,checkin}.rs`, `tests/core_*.rs`, migraciones `01xx_*.sql` |
| B-modules | T24, T25, T27, T30–T34, T35 (API) | `api/{tasks,inbox,people,content,school,dev,notes,drawings,agenda,search}.rs`, `repos/{tasks,inbox,people,content,school,dev,notes,drawings,agenda}.rs`, `domain/tasks.rs`, `tests/modules_*.rs`, migraciones `02xx_*.sql` |
| C-finance | T36–T43 | `api/finance.rs` (puede volverse `api/finance/`), `repos/finance.rs` (o `repos/finance/`), `domain/finance.rs` (o `domain/finance/`), `tests/finance_*.rs`, migraciones `03xx_*.sql` |
| D-telegram | T44–T48 | `apps/telegram-bot/**`, `crates/contracts/**`, `apps/server/src/telegram/**`, `apps/server/src/internal.rs`, `api/reminders.rs`, `repos/{jobs,telegram}.rs`, `domain/reminders.rs`, `tests/telegram_*.rs`, migraciones `04xx_*.sql` |
| E-integrations | T49–T54 | `apps/server/src/integrations/**`, `api/{integrations,captures,imports,exports}.rs`, `repos/{integrations,captures,imports}.rs`, `domain/{vault,capture}.rs`, `tests/integrations_*.rs`, migraciones `05xx_*.sql` |
| F-web | T14, T15, T35 (UI), T55–T58 y pantallas | `apps/web/**` |

Reglas:
* No edites archivos de otro agente. Si necesitas algo de otro módulo, usa los **contratos** de abajo o consulta la BD
  directamente desde tu repositorio (el esquema es común).
* Cambios de esquema: **nueva migración** en tu rango (`crates/persistence/migrations/01xx_descripcion.sql`, etc.).
  Nunca modifiques `0001–0006`.
* Dependencias nuevas: agrégalas a `[workspace.dependencies]` del `Cargo.toml` raíz y al crate (línea propia, al final
  de la sección) y menciónalas en tu reporte. Verifica la versión vigente (`cargo search`).
* **No** regeneres ni commitees `packages/api-client/*` (lo hace el lead tras integrar).
* `tests/common/mod.rs` es del lead: úsalo, no lo modifiques.

## Patrones obligatorios

**Repositorio** (`crates/persistence/src/repos/<m>.rs`): funciones libres con `impl PgExecutor<'_>` o `&mut Tx`,
`user_id` explícito en **todas** las consultas, `crate::sqlf!` para interpolar listas de columnas constantes (nunca datos),
`bind` para datos. Filas `#[derive(FromRow, Serialize, ToSchema)]` (son los DTO de respuesta, D-025). Updates con
`WHERE user_id = $1 AND id = $2 AND version = $3` + `version = version + 1`; si no afecta filas,
`Err(crate::repos::areas::conflict_or_missing(get(...).await))`. Ejemplo completo: `repos/areas.rs`.

**API** (`apps/server/src/api/<m>.rs`): `pub fn router() -> OpenApiRouter<AppState>` ya registrado en `api/mod.rs`.
Cada handler con `#[utoipa::path(<método>, path = "/<ruta>", tag = "<módulo>", request_body = .., responses(..))]`
(ruta relativa a `/api/v1`; parámetros `{id}`), extractor `auth: AuthUser` y `ApiResult<..>`. Validación →
`ApiError::validation(campo, "mensaje en español")` o `DomainError` (se convierte a 422). Conflicto → 409 con `current`.
Cuerpos de entrada con `#[derive(Deserialize, ToSchema)]`; parámetros de query con `IntoParams`. Mutaciones
relevantes llaman `crate::api::audit(..)`. Ejemplo completo: `api/areas.rs`. Listas: `crate::http::{ListParams, Page}`.

**Fechas**: `domain::time` (`local_date`, `iso_week_period`, `parse_iso_week`, `month_period`, `DayOfMonth::resolve`,
`local_to_utc`). "Hoy" del usuario = `local_date(state.now(), tz_del_usuario)`; nunca `Utc::now()` directo (usa
`state.now()`, que en pruebas es un reloj fijo: 2026-11-12 12:00 America/Monterrey).

**Dinero**: centavos `i64`; parseo con `domain::money::parse_amount_to_cents`; nada de `f64` en cálculos.

**Pruebas**: lógica pura en `domain` con `#[cfg(test)]`; integración en `apps/server/tests/<prefijo>_*.rs` con
`#[sqlx::test(migrator = "persistence::MIGRATOR")]` y el arnés `mod common; let t = common::TestApp::new(pool); let a = t.login(111).await;`
(allowlist de pruebas: 111 y 222 — úsalos para probar aislamiento). Las pruebas deben verificar comportamiento del
requisito, no reflejar la implementación.

Comandos (en tu worktree):
```bash
cp -cR <ruta-del-checkout-principal>/target ./target 2>/dev/null || true   # reutiliza dependencias compiladas (APFS clone)
node scripts/gen-test-keys.mjs                   # llaves RSA de prueba para auth_oidc (no se suben al repo)
export DATABASE_URL=postgres://dane:dane@127.0.0.1:5433/dane_test CARGO_BUILD_JOBS=3
cargo test -p server --test <tu_archivo>       # y cargo test -p domain
cargo clippy -p server --all-targets -- -D warnings && cargo fmt --all
```
PostgreSQL local ya corre en el puerto 5433 (`scripts/dev-db.sh start` si no).

## Contratos entre módulos

1. **Vínculos acción → hábito** (`persistence::hooks::mark_linked_habit(tx, user, HabitTrigger::X, fecha)`):
   peso registrado (A), gasto registrado (C), reflexión positiva (A); usar `unmark_linked_habit` si la fuente se
   borra. Configurables en `habit_links` (A expone CRUD).
2. **Progreso de objetivos** (`hooks::record_goal_progress` / `remove_goal_progress` / `contributing_goals`): una
   entidad aporta a los objetivos a los que el usuario la enlazó con `entity_links(src_type, src_id, 'goal', goal_id, 'contributes_to')`.
   Lectura terminada → `Amount(1)` (A); deuda saldada → `Completed` con `source_type='debt'` (C); abono parcial no completa.
   `source_type` ∈ `manual, reading, habit_entry, debt, transaction, weight, content`.
3. **Resumen financiero para el dashboard general**: `persistence::repos::finance::period_summary(db, user, start, end)`
   (firma fija; C implementa; A lo consume).
4. **Capturas desde Telegram**: D inserta `capture_proposals (user_id, input_text, source='telegram', intent, reference_date, status='pending')`
   con `intent` ∈ `daily, dump, gasto, ingreso, objetivos, weekly_review, log_session` y responde con un botón a
   `/captura/{id}`. E implementa `GET/POST /captures…` (extracción LLM o modo manual, vista previa, confirmación, aplicar).
5. **Recordatorios**: D cancela/omite el recordatorio diario si `daily_checkins.evening_done_at` de esa fecha no es nulo.
   A fija `evening_done_at` cuando el usuario marca el check-in nocturno como terminado.
6. **Tipos de entidad** (enlaces, favoritos, búsqueda, export): `area, project, goal, habit, checkin, person, task, inbox,
   weekly_review, reading, content, swipe, subject, class_note, devlog_session, adr, note, drawing, transaction`.
7. **Rutas de la web** (para enlaces del bot y del dashboard): `/hoy?date=YYYY-MM-DD`, `/objetivos`, `/objetivos/planificar`,
   `/habitos`, `/dashboard`, `/tareas`, `/inbox`, `/semanal?week=GGGG-Www`, `/finanzas`, `/finanzas/movimientos`,
   `/personas`, `/lecturas`, `/contenido`, `/swipe`, `/escuela`, `/dev`, `/notas`, `/dibujos`,
   `/buscar`, `/captura/{id}`, `/ajustes`, `/onboarding`, `/importar`.
8. **Tareas y recurrencias**: B materializa pendientes recurrentes y C ocurrencias financieras en sus `workers()`
   (registrados en `workers.rs`), de forma idempotente (UNIQUE por regla+periodo) y con catch-up tras reinicios.

## Entrega
Haz commits en tu rama/worktree. Reporte final: qué quedó implementado (por tarea), archivos, pruebas ejecutadas con
resultado real (copiar el resumen `test result`), dependencias agregadas, desviaciones y pendientes reales.
