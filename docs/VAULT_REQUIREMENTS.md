# Matriz de requisitos del vault → aplicación

Fuente: `Vault-Obsidian-esqueleto.zip` (copia sin plugins en `fixtures/vault-skeleton/`).

> **Devocionales quitados (D-039).** Las filas de devocional de esta matriz ya no aplican: la app no tiene pantalla,
> API, ajuste ni hábito de devocional, y el importador omite `Proyectos/personal/Devocionales/` y la propiedad
> `devocional` de las dailies. El esqueleto del vault se conserva tal cual como especificación original.
Inventario leído (T01): `AGENTS.md`, `CLAUDE.md`, `LEEME - Cómo usar este vault.md`, `Home.md`, `Inbox.md`,
5 notas de proyecto/finanzas, `Pendientes recurrentes.md`, **15 plantillas**, **7 Bases**, **7 comandos**, **8 scripts DataviewJS**,
y `.obsidian/` (solo para el mapeo de plugins; no se ejecuta nada).

El ZIP se trata como especificación. Sus valores (70 kg, 4 ejercicios/semana, 5 fallas devocionales,
presupuesto de Negocio $300/$1,000/$6,500, banco "<tu banco>", `<VIEW_ID>`) son **ejemplos**: el onboarding
los propone como valores editables y los datos demo opt-in los usan; nunca se importan como hechos del usuario.

Columnas: **Fuente** → **Comportamiento** → **Entidad / API / Pantalla** → **Tarea** → **Prueba**.
Rutas API relativas a `/api/v1`. Pantallas relativas a la SPA (`apps/web`).

## 1. Notas raíz y reglas generales

| Fuente | Comportamiento | Entidad / API / Pantalla | Tarea | Prueba |
|---|---|---|---|---|
| AGENTS.md "Reglas duras" | Nunca guardar secretos en datos del usuario | Export excluye `sessions`, `identities.secret`, credenciales cifradas; validador de notas no bloquea pero export/import nunca incluyen tokens | T54, T13 | `export_excludes_secrets` |
| AGENTS.md | Videos/renders fuera; solo rutas | `content_pieces.raw_url/render_url` (URL validada, sin descarga) | T30, T31 | `content_rejects_non_http_urls` |
| AGENTS.md | Tareas de equipo en ClickUp, solo enlazadas; no modificar ClickUp | `tasks.external_*`; adaptador ClickUp solo lectura; marcar en la app no llama a ClickUp | T51, T24 | `clickup_adapter_is_read_only`, `completing_external_task_does_not_call_provider` |
| AGENTS.md "Áreas y proyectos" | 4 áreas iniciales con proyecto homónimo; renombrables | `areas`, `projects`; `/areas`, `/projects`; pantalla Proyectos | T16, T15 | `areas_crud_and_rename_keeps_ids` |
| AGENTS.md "El estado es una propiedad" | Estado como campo, nunca carpeta | Columnas `status` con CHECK en cada entidad | T08 | migraciones (CHECK) |
| Home.md frontmatter | `meta_peso`, `meta_ejercicio_semana`, `max_fallas_devocional` | `user_settings.weight_goal_kg/weight_direction`, hábito fijo ejercicio `times_per_week=4`, `user_settings.max_devotional_failures` | T15, T23 | `dashboard_devotional_failures_vs_max` |
| Home.md "Vistas" | 7 vistas Bases enlazadas | Pantalla `/vistas/*` (ver §4) | T34 | E2E vistas |
| Home.md "Reglas" | Captura rápida → Inbox; buscar > archivar | `/inbox`, búsqueda global | T25, T34 | `inbox_capture_and_process` |
| Inbox.md | Captura sin pensar, se vacía en weekly | `inbox_items` + propuesta revisable | T25 | `inbox_requires_confirmation_before_move` |
| LEEME §2 | Áreas, metas, categorías, devocional opcional, ClickUp opcional | Onboarding + Ajustes (módulos activables, devocional↔meditación↔off) | T15, T28 | E2E onboarding |
| LEEME §6 | Rutina mañana/noche/domingo/lunes/día 1 | Recordatorios diario (21:00), semanal (domingo), mensual (día 1) | T48 | `reminder_schedule_local_time_dst` |
| .gitignore | Excluir `.env*`, `*.key` | `.gitignore` del repo | T06 | revisión |

## 2. Plantillas (15)

| Fuente | Comportamiento | Entidad / API / Pantalla | Tarea | Prueba |
|---|---|---|---|---|
| Templates/Daily.md | Propiedades `devocional/ejercicio/lectura` (hábitos fijos), `energia`, `peso`; secciones Devocional, Agenda, Gastos de hoy, Hábitos del mes, Hábitos de la semana, Log, Personas, Pendientes por área, Lo que hice mal/bien | `daily_checkins` (1 por usuario+fecha), `habit_entries`, `daily_log_entries`, `reflection_items`, `calendar_events`, `interactions`, `tasks.planned_on`; `GET/PUT /checkins/{date}`; pantalla **Hoy** | T20 | `checkin_upsert_same_row`, `checkin_partial_save_keeps_null_as_unknown` |
| Templates/Devocional.md | Pasaje; tabla Versículo/Acción; Qué aprendí; Enseñanza; Oración; Canción | `devotionals` (`verse_actions` JSONB validado); `/devotionals`; pantalla Devocionales | T28 | `devotional_with_content_completes_habit`, `empty_devotional_does_not` |
| Templates/Objetivo mes.md | Varios objetivos con Área, Por qué, Cómo sé, Estado, Hábitos diarios; Seguimiento; Cierre | `goals(kind=month)`, `habits(kind=goal)`, cierre en `goals.closing`; Seguimiento se **calcula** desde `habit_entries` | T17, T19 | `month_goal_with_habits` |
| Templates/Objetivo semana.md | Igual + "Apoya a (mes)" | `goals.parent_goal_id` (FK compuesta mismo dueño) | T17 | `week_goal_parent_must_be_same_owner_month` |
| Templates/Weekly.md | `semana`, `paz`; secciones Qué pasó…Prioridades (3) | `weekly_reviews`; `/weekly-reviews/{iso_week}` + `/summary`; pantalla Semanal | T26 | `weekly_summary_counts_habits`, `peace_range_1_5` |
| Templates/Gastos.md | Presupuesto copiado del mes anterior; registro Fecha/Concepto/Monto/Categoría/Pago; suscripciones embebidas | `budget_months`, `budget_lines` (copia al crear), `transactions(kind=expense)` | T39, T38 | `budget_month_initialized_from_previous`, `editing_current_does_not_touch_previous` |
| Templates/Ingresos.md | Registro Fecha/Concepto/Monto/Categoría/Cuenta; ingresos fijos | `transactions(kind=income|loan_received)`, `fin_recurring_rules(kind=fixed_income)` | T38, T40 | `loan_is_financing_not_earned_income` |
| Templates/Tarjeta.md | `limite`, `disponible`, `actualizado`, `dia_corte`, `dia_pago`, `estado` | `fin_accounts(kind=credit_card)`, `balance_snapshots` | T37 | `card_available_estimated_after_snapshot` |
| Templates/Cuenta.md | `banco`, `saldo` al **iniciar** el día `actualizado`, `estado` | `fin_accounts(kind=debit)`, `balance_snapshots(semantics=legacy_day_start_inclusive)` | T37 | `legacy_account_includes_base_day`, `legacy_card_excludes_base_day` |
| Templates/Persona.md | `area`, `relacion` (enum), `org`, `rol`, `ultimo_contacto`, `cumpleaños` (YYYY-MM-DD o MM-DD), `contacto`; Contexto; Notas cronológicas | `people`, `person_notes`, `interactions`; `/people` | T27 | `birthday_without_year`, `last_contact_from_interactions` |
| Templates/Pieza.md | estado pipeline, formato, plataformas, hook, pilar, fecha_pub, raw/render, views, retención; 10 hooks; guion; shot list; edición; resultado | `content_pieces`; `/content` | T30 | `content_pipeline_transitions`, `hooks_max_10_choose_one` |
| Templates/Lectura.md | tipo, autor, estado, rating, inicio, fin; ideas, citas, aplicación | `readings`; `/readings` | T29 | `finishing_reading_contributes_once_to_books_goal` |
| Templates/Clase.md | materia, fecha; temas, dudas, entregas | `subjects`, `class_notes`, `tasks.class_note_id` | T32 | `class_note_with_deliverable_task` |
| Templates/Devlog.md | fecha, proyectos; qué hice, bloqueos, decisiones, siguiente | `devlogs`, `devlog_sessions` | T33 | `devlog_multiple_sessions_same_day` |
| Templates/ADR.md | proyecto, estado (propuesta/aceptada/reemplazada), fecha; contexto, decisión, alternativas, consecuencias | `adrs` (número por proyecto) | T33 | `adr_numbering_per_project` |

Las plantillas Templater/Templates core **no** se ejecutan: el importador reconoce `<% … %>` y `{{date…}}` y las descarta como datos (T53, prueba `importer_skips_templates_and_placeholders`). Las plantillas personalizables de la app son estructuras de secciones en `user_settings.templates` (devocional) y valores por defecto de formularios.

## 3. Comandos (7) → bot + app

| Fuente | Comportamiento | Entidad / API / Pantalla | Tarea | Prueba |
|---|---|---|---|---|
| .claude/commands/daily.md §0–1 | Crear/completar la daily varias veces sin duplicar; peso/energía/ejercicio/lectura solo con dato del usuario; devocional=true si la nota tiene contenido | `PUT /checkins/{date}` (merge parcial, versión); `devotionals` → hábito vía `habit_links(trigger=devotional_content)` | T20, T28 | `checkin_repeat_no_duplicate_effects` |
| daily.md §1 | "Si marcas peso, marca el hábito Pesarme" | `habit_links(trigger=weight_logged)` explícito y configurable, nunca por texto | T20 | `weight_link_marks_single_occurrence` |
| daily.md §2 | Devocional del día enlazado | Pantalla Hoy → `/devocionales/{date}` | T28 | E2E |
| daily.md §3 | Agenda de Calendar con hora; excluir festivos; sin duplicar; actualizar si cambia hora | Adaptador Calendar, `calendar_events` UPSERT por `(calendar_id, provider_event_id)`; exclusión de calendarios holiday/observancias | T49 | `calendar_sync_upserts_without_duplicates`, `calendar_excludes_holidays` |
| daily.md §3b | "Planeo gastar" + tarjeta sugerida (reglas: disponible, no corte, 30%, plan, deudas; más días al corte; débito si ninguna) | `daily_checkins.planned_spend_*`; `GET /finance/card-suggestion?date&amount` | T42 | `card_suggestion_rules` (8 casos) |
| daily.md §4 | Copiar hábitos de objetivos activos mes/semana; Seguimiento del día anterior | La app **no copia**: la daily lista hábitos vigentes en la fecha; Seguimiento = cálculo | T19, T22 | `habits_for_date_respect_validity` |
| daily.md §5 | Log: dictado + pendientes cerrados + eventos pasados + devlog + total de gastos | `daily_log_entries` (manual) + `GET /checkins/{date}/derived` (cerrados, eventos, devlog, total gastos) — derivados, no duplicados | T20 | `derived_log_not_persisted_twice` |
| daily.md §6 | Personas: dictado + invitados + enviados; actualizar ultimo_contacto; no crear personas | `interactions`; `person_suggestions` (pendiente) | T27, T50 | `calendar_attendee_creates_suggestion_not_person` |
| daily.md §7 | Pendientes por área; arrastre sin duplicar; recurrentes día N/penúltimo/último; dictado; ClickUp ordenado (fáciles, resto, need info) | `tasks` (arrastre conserva id), `task_recurrence_rules`, adaptador ClickUp | T24, T51 | `carry_over_keeps_id`, `recurrence_penultimate_feb`, `clickup_ordering` |
| daily.md §8 | Reflexión solo dictada; "bien hoy" marca hábito | `reflection_items`; `habit_links(trigger=positive_reflection)` | T20 | `positive_reflection_link` |
| daily.md §9 | Gastos del dictado; ingresos fijos y suscripciones el día que tocan | Captura libre (T52) + ocurrencias recurrentes (T40) | T40, T52 | `recurring_occurrence_idempotent_catch_up` |
| daily.md §10 | Preguntar lo que falta en la tarde en una sola pregunta | Recordatorio nocturno del bot + estado "pendiente" en Hoy | T48 | bot test |
| commands/dump.md | Texto libre → pendientes por área, enlazar personas/proyectos, `#clickup` sugerido | `/capture` (LLM opcional) → propuestas → confirmar; manual: Inbox | T52, T25 | `capture_requires_confirmation` |
| commands/gasto.md | Varios gastos; fecha relativa; categoría única; pago (tarjeta enlazada); preguntar ambiguo; presupuesto del mes; rubros; préstamos familiares; pago a tarjeta; pago de deuda y checkbox al saldar; suscripciones | `POST /finance/transactions/batch` (idempotency), `/finance/budgets`, `/finance/debts`, `/finance/recurring` | T36–T41 | `duplicate_looking_purchases_kept`, `retry_same_key_no_duplicate`, `family_loan_counts_once`, `debt_paid_completes_milestone` |
| commands/ingreso.md | Fecha obligatoria (no suponer hoy), categorías, USD convertido, préstamo = deuda, ingresos fijos | `transactions(kind=income)`, FX explícito obligatorio para moneda ≠ base | T38 | `foreign_currency_requires_explicit_rate` |
| commands/objetivos.md | Semana actual/próxima/mes; contexto previo; cerrar anterior preguntando; mes antes que semana; 1–3 hábitos por objetivo; avisos ~6 / ~8 | Pantalla **Objetivos › Planificar**; `/goals/planning?week=` con avisos | T17 | `habit_load_warnings` |
| commands/weekly-review.md | Resumen de dailies, gastos, inbox, contenido, devlog, Calendar, ClickUp; paz; devocionales; objetivos; Inbox propuesto sin mover; 3 prioridades | `GET /weekly-reviews/{week}/summary` + edición | T26 | `weekly_summary_sources` |
| commands/log-session.md | Resumen de sesión en devlog, ADR si hay decisión, runbook | `/dev/devlogs/{date}/sessions` + importación de texto pegado | T33 | `log_session_import_text` |

Equivalencia de comandos Telegram (nombres válidos `[a-z0-9_]`): `/daily`, `/checkin`, `/objetivos`, `/dump`, `/gasto`,
`/ingreso`, `/weekly_review` (= `weekly-review`), `/log_session` (= `log-session`), `/dashboard`, `/settings`, `/help`, `/pause`, `/resume`, `/start`.

## 4. Bases (7) → vistas

| Fuente | Comportamiento | Pantalla | Tarea | Prueba |
|---|---|---|---|---|
| Bases/Devocionales.base | Tabla por fecha desc con pasaje | `/devocionales` tabla | T28 | E2E |
| Bases/Hábitos.base | Últimos 30 días: devocional, ejercicio, lectura, energía | `/habitos` vista 30 días + heatmap | T19, T23 | `last_30_days_view` |
| Bases/Lecturas.base | Tarjetas agrupadas por estado; tabla por rating | `/lecturas` (tarjetas/tabla) | T29 | E2E |
| Bases/Objetivos.base | Tabla agrupada por tipo, periodo, inicio/fin, estado | `/objetivos/historial` | T17 | E2E |
| Bases/Personas.base | Agrupado por área; días sin hablar (fórmula); cumpleaños | `/personas` | T27 | `days_without_talking` |
| Bases/Pipeline.base | Tabla por estado, días para publicar; tarjetas; publicados por views | `/contenido` (tabla/kanban/publicados) | T30 | `days_to_publish` |
| Bases/Proyectos.base | Dev reciente (excluye devocionales/clases/objetivos/finanzas) y ADRs | `/proyectos` actividad + `/dev/adrs` | T16, T33 | `recent_activity_excludes` |

## 5. Scripts DataviewJS (8) → cálculos en Rust

| Fuente | Comportamiento | Implementación | Tarea | Prueba |
|---|---|---|---|---|
| Scripts/home.js | Semana vs semana pasada; mes vs mes pasado (hasta hoy); hábitos fijos; % hábitos mes/semana; peso último/cambio; gasto sin deudas; pagado a deudas; fallas devocionales vs máx; ejercicio vs meta; deudas liquidadas | `domain::metrics`, `GET /dashboard?date` | T22, T23 | `dashboard_week_vs_previous_same_span` |
| Scripts/finanzas.js | KPIs; presupuesto queda/excedido; proyección = variable/díaTranscurrido×díasMes + suscripciones; ingreso esperado vs presupuesto; acumulado vs ritmo; categorías vs presupuesto; 6 meses; historial; top 5 | `domain::finance::dashboard`, `GET /finance/dashboard?month` | T43 | `projection_formula_no_double_subscriptions` |
| Scripts/gastos.js | Total, sin deuda, por categoría vs presupuesto, fuera de presupuesto | `GET /finance/months/{ym}` | T39, T43 | `budget_vs_spent` |
| Scripts/gastos-categorias.js | Barras por categoría (incluye vacías) | Gráfica Recharts en Finanzas › Mes | T43 | E2E |
| Scripts/gastos-proyecto.js | Gastos del proyecto (categoría o enlace), rubro por keywords, aviso si desglose ≠ categoría, suscripciones del área | `GET /finance/projects/{id}`; rubro explícito > keyword | T39 | `explicit_budget_item_beats_keyword` |
| Scripts/ingresos.js | Total ingresos vs esperados vs gastos | `GET /finance/dashboard` (ingresos registrados/confirmados/esperados) | T43 | `income_registered_confirmed_expected` |
| Scripts/cuentas.js | Saldo + ingresos − gastos desde `actualizado` **inclusive**; coincidencia por texto del banco | `domain::finance::balances` con cuenta por **ID**; semántica legacy solo para snapshots importados | T37 | `account_estimate_*` |
| Scripts/tarjetas.js | Disponible − gastos + pagos **después** de `actualizado`; % usado; próximo pago | `domain::finance::balances` (tarjetas) | T37 | `card_estimate_*` |

## 6. Correcciones deliberadas frente al vault

| # | Comportamiento original | Problema | Corrección en la app | Prueba |
|---|---|---|---|---|
| C1 | home.js: "Días sin daily cuentan como no hechos" | Mezcla sin registro con "no" | Sin registro sigue siendo exigible en días cerrados pero se muestra distinto; el día abierto no penaliza | `open_day_not_penalized`, `missing_counts_but_flagged` |
| C2 | Ejercicio contado por días con `true` | Frecuencia N/semana no es diaria | Frecuencia `times_per_week` con racha por semanas que alcanzan N | `times_per_week_streak` |
| C3 | finanzas.js "Me quedó" = ingresos − todo lo registrado (incluye pagos de tarjeta) | Compra con tarjeta + pago posterior cuenta dos veces | Separar **consumo**, **pagos de deuda**, **ingreso ganado**, **financiamiento** y **flujo de efectivo**; neto de consumo = ingreso ganado − consumo; flujo = entradas − salidas de cuentas de efectivo | `net_correction_card_purchase_and_payment` |
| C4 | cuentas.js/tarjetas.js asocian por texto (`includes(banco)`) | Falsos positivos | Asociación por `account_id`; texto solo en importación con advertencia | `import_payment_text_maps_to_account` |
| C5 | gasto.md: fila idéntica = duplicado | Compras legítimas iguales | Idempotency key por envío; dos compras iguales con claves distintas se conservan | `duplicate_looking_purchases_kept` |
| C6 | daily.md suscripciones: "si no hay fila cuyo concepto contenga el servicio" | Dedupe por texto | UNIQUE `(recurring_rule_id, period)` | `recurring_unique_per_period` |
| C7 | Suscripción registrada el día de cobro = gasto | Fecha programada ≠ cobro | Ocurrencia `pending_reconciliation`: solo proyección hasta confirmar | `pending_occurrence_excluded_from_confirmed` |
| C8 | Ingreso en USD: "conviértelo" | Cotización inventada | Monto original + tasa/fecha/fuente explícitas; sin tasa, no se suma a MXN | `foreign_currency_requires_explicit_rate` |
| C9 | Hábitos copiados como checkboxes a cada daily | Copia, pierde identidad | Hábito con identidad y versiones; la daily lista los vigentes | `frequency_change_preserves_history` |
| C10 | Pesarme / gastos / algo bueno detectados por texto del hábito | Frágil | `habit_links` explícitos configurables | `weight_link_marks_single_occurrence` |
| C11 | Deuda del Home: regex `$` en checkboxes de sección "deuda" | Frágil | Entidad `debts` con abonos y relación explícita a objetivos | `debt_progress` |
| C12 | Proyección lineal aplicada a todo | Sin sentido para peso | Proyección solo para acumulativos; valor actual muestra último y cambio | `no_linear_projection_for_current_value` |
| C13 | Comparación mes actual (parcial) vs mes pasado completo | Tramos desiguales | Comparación por mismo tramo; opción etiquetada de periodos completos | `same_span_comparison` |
| C14 | Persona no encontrada → texto plano | — | `person_suggestions` pendiente de aceptar | `suggestion_not_created_silently` |

## 7. Plugins de Obsidian → mapeo

| Plugin | Uso en el vault | Equivalente |
|---|---|---|
| Templater / Templates core | Crear notas desde plantillas | Formularios con valores por defecto; plantilla de devocional personalizable |
| Dataview / DataviewJS | Home y finanzas | Cálculos en `crates/domain` y endpoints de dashboard |
| Excalidraw | Notas visuales | Editor Excalidraw cargado bajo demanda (`/dibujos`), escena en `drawings.scene` |
| Omnisearch | Búsqueda | Búsqueda global con `unaccent` + `pg_trgm` |
| Surfing, Table editor, Cycle through panes | Navegación/edición de autoría | No se clonan (autoría general de Obsidian) |
| Bases (core) | 7 vistas | Pantallas de vistas con filtros/orden/paginación |
