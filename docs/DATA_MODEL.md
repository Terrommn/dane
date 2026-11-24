# Modelo de datos

Fuente de verdad: `crates/persistence/migrations/*.sql` (PostgreSQL 17). Este documento explica intención y reglas.

## Convenciones
* `id uuid` (gen_random_uuid), `user_id` en toda tabla propia y `UNIQUE (user_id, id)`.
* **Ownership en la BD**: FKs compuestas `(user_id, x_id) → x(user_id, id)`; `ON DELETE SET NULL (x_id)` preserva `user_id`.
  Probado: insertar un proyecto del usuario B con el área del usuario A falla (`projects_user_id_area_id_fkey`).
* `version integer` en entidades editables → control optimista (`If-Match`/`version`, 409 con estado actual).
* `created_at/updated_at timestamptz` (UTC); `updated_at` por trigger `touch_updated_at`.
* Fechas de actividad como `date` **local** del usuario (`on_date`, `occurred_on`), zona en `user_settings.timezone`.
  Cambiar la zona afecta cálculos futuros; los `date` guardados no se reclasifican.
* Estado siempre como columna con `CHECK`, nunca como estructura de carpetas.
* `archived_at` para archivar/recuperar sin borrar.
* JSONB solo para contenido variable (`verse_actions`, `shot_list`, `links`, `proposal`, `summary`); números críticos y
  relaciones en columnas tipadas.

## Mapa por dominio

| Dominio | Tablas | Reglas clave |
|---|---|---|
| Identidad | `users`, `identities`, `sessions`, `user_settings`, `telegram_chats`, `oauth_flows` | `identities(provider, subject)` único; `telegram_user_id` vincula Mini App y OIDC al mismo `users.id`; sesiones guardan solo hash SHA-256 del token |
| Infra de mutación | `idempotency_keys`, `audit_log` | PK `(user_id, key)`; misma clave con otro cuerpo → 422 |
| Áreas | `areas`, `projects` | slug único por usuario; renombrar conserva ID |
| Objetivos | `goals`, `goal_progress_events` | semana → mes por `parent_goal_id` (solo `kind=week`); `UNIQUE(goal_id, source_type, source_id)` evita doble conteo; una fuente puede aportar a varios objetivos una vez a cada uno; exactamente uno de `amount`/`value`/`completed` |
| Hábitos | `habits`, `habit_versions`, `habit_pauses`, `habit_entries`, `habit_links` | identidad estable; versiones con vigencia; entrada PK `(habit_id, on_date)`; sin fila = sin registro; `not_applicable` exige motivo; `habit_links` un disparador → un hábito |
| Día | `daily_checkins`, `daily_log_entries`, `reflection_items`, `calendar_events` | `UNIQUE(user_id, on_date)`; eventos `UNIQUE(user, source, calendar, provider_event_id)` (upsert sin duplicar) |
| Personas | `people`, `person_notes`, `interactions`, `person_suggestions` | cumpleaños mes/día con año opcional; sugerencias no crean personas |
| Tareas | `tasks`, `task_people`, `task_events`, `task_recurrence_rules` | arrastre = evento, misma fila; recurrentes `UNIQUE(user, rule, period)`; externas `UNIQUE(user, source, external_id)` |
| Inbox | `inbox_items` | `proposal` revisable; `client_id` único para capturas offline idempotentes |
| Semana | `weekly_reviews` | `UNIQUE(user, iso_week)`; `peace` 1–5; ≤ 3 prioridades |
| Conocimiento | `notes`, `entity_links`, `favorites`, `search_index`, `drawings` | `search_index` por triggers con `unaccent` + trigramas |
| Módulos | `devotionals` (sin uso desde D-039: se conserva con sus datos, la app ya no la lee ni escribe), `readings`, `content_pieces`, `swipe_refs`, `subjects`, `class_notes`, `devlogs`, `devlog_sessions`, `devlog_session_projects`, `adrs` | ADR `UNIQUE(user, project, number)`; `superseded` exige `superseded_by_id`; hooks ≤ 10 |
| Finanzas | `fin_categories`, `fin_accounts`, `balance_snapshots`, `debts`, `fin_recurring_rules`, `budget_months`, `budget_lines`, `budget_items`, `transactions`, `credit_plans` | ver abajo |
| Operación | `jobs`, `telegram_updates`, `integration_connections`, `capture_proposals`, `import_runs`, `import_records` | ver abajo |

## Finanzas

### Clases de movimiento (`transactions.kind`)
| kind | Cuenta origen (`account_id`) | Contraparte (`counter_account_id`) | Consumo | Pago de deuda | Ingreso ganado | Financiamiento | Flujo de efectivo |
|---|---|---|---|---|---|---|---|
| `expense` (débito/efectivo) | − saldo | — | ✔ (según rol de categoría) | — | — | — | salida |
| `expense` con tarjeta | − disponible de la tarjeta | — | ✔ | — | — | — | (sin salida de efectivo hasta pagar) |
| `expense` financiado por persona (`financed_by_person_id`) | opcional | — | ✔ una vez | — | — | — | — |
| `income` | + saldo | — | — | — | ✔ (rol `earned`) | — | entrada |
| `loan_received` | + saldo | — | — | — | — | ✔ | entrada |
| `card_payment` | − saldo de la cuenta | + disponible de la tarjeta | — | ✔ | — | — | salida |
| `debt_payment` | − saldo | — | — | ✔ (reduce `debts`) | — | — | salida |
| `transfer` | − saldo | + saldo destino | — | — | — | — | neutro |
| `adjustment` | ± saldo (`adjustment_sign`) | — | — | — | — | — | — |

Roles de categoría (`fin_categories.role`): gasto `consumption | debt_payment | investment | payroll`;
ingreso `earned | financing | other_income`. Inversión y nómina se reportan aparte del consumo personal.

### Estados
`confirmed` (manual o conciliado) → afecta saldos y flujo. `pending_reconciliation` (generado por regla) → solo
proyecciones etiquetadas. Confirmar actualiza **la misma fila** (`UNIQUE(recurring_rule_id, recurrence_period)`), así que
no puede existir una segunda transacción para la misma ocurrencia. `void` anula sin borrar.

### Moneda
`amount_cents` + `currency`. Si el usuario registra USD con tasa: `currency=MXN`, `amount_cents` convertido y
`original_*` + `fx_rate/fx_date/fx_source` obligatorios (CHECK). Sin tasa: se guarda en USD y los totales en MXN lo
excluyen y lo reportan aparte.

### Saldos estimados (D-020)
`estimado = snapshot.balance + Σ entradas − Σ salidas` de movimientos `confirmed` posteriores al snapshot:
* `instant`: con `occurred_at` → `occurred_at > observed_at`; sin hora → `occurred_on > observed_on`.
* `legacy_day_start_inclusive` (cuentas importadas del vault): `occurred_on >= observed_on`.
* `legacy_after_day` (tarjetas importadas del vault): `occurred_on > observed_on`.

### Presupuestos
`budget_months` es un snapshot por mes; al crearse copia `budget_lines` y `budget_items` del mes anterior
(`copied_from`). Editar el mes actual no toca los anteriores. Si una categoría tiene rubros de proyecto, su línea es la
suma de los rubros (lo mantiene el server). Asignación de rubro: `transactions.budget_item_id` explícito > keywords.

## Operación
* `jobs`: `UNIQUE NULLS NOT DISTINCT (user_id, kind, occurrence_key)`; claim con `FOR UPDATE SKIP LOCKED`, lease
  (`lease_token`, `lease_expires_at`); estados `pending → leased → sent | failed | cancelled | expired`.
* `telegram_updates`: PK `update_id`; `pending` hasta que el procesador aplica efectos y encola respuestas en la misma
  transacción y lo marca `processed`.
* `integration_connections`: credenciales cifradas AES-256-GCM (`credentials_enc`), `config` (calendarios, vista ClickUp,
  mapa de estados), `cursor` (syncToken/paginación), última sincronización y error.
* `import_records`: `PK(user_id, source_key, entity_type)` → reimportar actualiza en vez de duplicar; si la entidad cambió
  en la app después de importarse y el archivo cambió, se reporta conflicto.
