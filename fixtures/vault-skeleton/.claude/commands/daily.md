---
description: Crea o completa TODAS las secciones de la daily de hoy (Calendar, ClickUp, objetivos, log, personas, reflexión, propiedades)
argument-hint: [opcional: dictado libre — peso, energía, qué hice, con quién hablé, qué hice bien/mal, gastos]
---
Dictado del usuario: $ARGUMENTS

Fecha de hoy en America/Monterrey (usa `date +%F` con TZ=America/Monterrey). Nota: `Daily/<fecha>.md`. Si el usuario dice "ayer" o da una fecha, trabaja sobre esa daily.

Se puede correr varias veces al día (mañana para planear, noche para cerrar): cada paso **completa** sin borrar, reordenar ni duplicar lo que ya está. Llena cada sección en el orden del template.

## 0. Crear la nota
Si no existe, créala desde `Templates/Daily.md` sustituyendo la sintaxis de plantilla (`{{date:YYYY-MM-DD}}`, `{{date:GGGG-[W]WW}}`, etc.) por valores reales.

## 1. Propiedades
- `devocional`: si `Proyectos/personal/Devocionales/Devocional <fecha>.md` tiene algo escrito en Pasaje o en las demás secciones → `true`. Revisa también el de **ayer** y corrige su daily si quedó en `false`.
- `peso` (número en kg), `energia` (1–5), `ejercicio`, `lectura`: **solo** si el usuario lo dice en el dictado ("peso 94.6", "energía 3", "fui al gym", "leí"). Nunca los inventes ni los supongas por el calendario. Si marcas `peso`, marca también el hábito "Pesarme…" de `## Hábitos del mes`.

## 2. Devocional
Si no existe `Proyectos/personal/Devocionales/Devocional <fecha>.md`, créalo desde `Templates/Devocional.md`. En `## Devocional` de la daily solo va `- [[Proyectos/personal/Devocionales/Devocional <fecha>|Devocional de hoy]]`.

## 3. Agenda
Lee los eventos de hoy de Google Calendar (todos sus calendarios **excepto** `en.mexican#holiday@group.v.calendar.google.com`; nunca días festivos ni observancias) y escríbelos en `## Agenda` como `- 10:00–11:00 Título`, en orden por hora. Quita el comentario `<!-- … -->` del template. Si un evento ya está (misma hora y título), no lo dupliques; si cambió de hora, actualízalo. Solo eventos de calendario, nada de ClickUp. No crees eventos.

## 3b. Gastos de hoy y tarjeta
Sección `## Gastos de hoy` (después de `## Agenda`), con dos líneas: `- Planeo gastar: <qué y cuánto, o "nada">` y `- Tarjeta: <recomendación> · <por qué en pocas palabras>`.
- Si el usuario ya dijo en el dictado qué planea gastar hoy (o "nada"), escríbelo en `Planeo gastar`. Si la línea sigue vacía, **siempre** pregúntale al final de la respuesta: "¿Planeas gastar algo hoy? (qué y cuánto)". No lo supongas.
- Con o sin respuesta, llena `Tarjeta` con la mejor tarjeta para gastar hoy, calculada con las notas de `Proyectos/personal/Finanzas/Tarjetas/` (`estado` ≠ `cancelada`) y las reglas de `Proyectos/personal/Finanzas/Plan de crédito.md`:
  1. **Me queda** de cada tarjeta = `disponible` − gastos pagados con ella después de `actualizado` + pagos a ella (`Pago de deuda` que la enlaza) después de esa fecha (misma cuenta que `Scripts/tarjetas.js`).
  2. Descarta las que: no tienen `dia_corte`; tienen deuda sin liquidar en el objetivo de deudas del mes; el Plan de crédito dice que se van a cancelar; o con el gasto planeado quedarían arriba del 30% de su `limite` o sin suficiente `Me queda`.
  3. De las que quedan, la mejor es la que tiene **más días hasta su próximo corte** (el corte justo pasó: la compra entra al siguiente estado de cuenta y tarda más en cobrarse). Si hoy es el día de corte de una, no la uses hoy.
  4. Si ninguna pasa, recomienda `Débito <tu banco>` y di por qué (ej. "Tarjeta A aún con deuda").
  Actualiza la línea cada vez que corras `/daily` (cambian los días al corte y el disponible).

## 4. Hábitos del mes / de la semana
Objetivos en `Proyectos/personal/Objetivos/Mes <YYYY-MM>.md` y `Semana <GGGG-[W]WW>.md`.
- Copia los checkboxes de **todos** los `#### Hábitos` de cada objetivo activo (`### N. …` con `**Estado:** activo`) del mes a `## Hábitos del mes` y de la semana a `## Hábitos de la semana`, cada uno con ` · <objetivo corto>` al final, sin marcar, debajo de la línea `Objetivo(s): [[…]]`. No dupliques.
- Si la nota no existe o no tiene hábitos, no la crees: deja `- Sin hábitos todavía: corre /objetivos` y avísale al usuario.
- Si el usuario dice en el dictado que cumplió un hábito, márcalo `[x]`. No marques nada por tu cuenta, salvo el de pesarse cuando `peso` ya tiene valor.
- Seguimiento de **ayer**: por cada objetivo, agrega a su `## Seguimiento` la fila `| <fecha de ayer> | <hechos>/<total> | <hábitos sin marcar> |` con los checkboxes de la daily de ayer (si ayer era otra semana/mes, escribe en ese objetivo). No dupliques la fila de una fecha.
- Estos hábitos **no** van en las propiedades ni en `Bases/Hábitos.base` (solo para devocional, ejercicio, lectura).

## 5. Log
Bitácora de lo que pasó hoy, bullets cortos en pasado (`- Cerré el fix de asistencias de [[negocio]]`). Junta de estas fuentes:
- Lo que el usuario dicte.
- Checkboxes marcados `[x]` hoy en `## Pendientes` (resúmelos; no copies el link completo de ClickUp, basta el nombre corto).
- Eventos de la Agenda que ya pasaron (solo los relevantes: reuniones, clases, entregas; no "skool" repetido sin contexto).
- `Dev/devlog/<fecha>.md` si existe: 1 bullet por proyecto trabajado con link al devlog.
- Gastos de hoy en `Proyectos/personal/Finanzas/Gastos/Gastos <YYYY-MM>.md`: un solo bullet con el total del día.
Enlaza personas y proyectos. Quita la línea vacía `- ` del template al agregar. No dupliques algo ya escrito con otras palabras.

## 6. Personas
Con quién habló el usuario hoy y de qué: `- [[Nombre Apellido|apodo]]: <tema en pocas palabras>`. Fuentes: el dictado, invitados de eventos de hoy en Calendar, correos que el usuario **envió** hoy en Gmail (solo lectura; `in:sent after:<fecha>`), y personas mencionadas en pendientes marcados hoy.
- Busca a cada persona en `Personas/` (también por apodo). Si existe, enlázala y actualiza `ultimo_contacto: <fecha>` en su nota (no toques otras propiedades). Si no existe, escríbela en texto plano y menciónala en la respuesta por si el usuario quiere crearle nota; no crees notas de personas sin que lo pida.
- Ignora correos automáticos, newsletters y notificaciones. Una línea por persona; si ya está, agrega el tema a su línea.

## 7. Pendientes
Subsecciones `### Personal`, `### Trabajo`, `### Negocio`, `### Escuela`. No borres, no reordenes, no dupliques. Quita la línea vacía `- [ ] ` del template en las subsecciones donde agregues algo.
- **Arrastre de ayer**: los checkboxes sin marcar de `## Pendientes` de la daily de ayer (en dailies viejas puede llamarse `## Pendientes rápidos`) se copian a la misma subsección (si la vieja no tenía áreas, clasifícalos tú). Los de ClickUp también, sin duplicar por URL.
- **Recurrentes**: lee `## Lista` de `Proyectos/personal/Pendientes recurrentes.md`. Por cada fila `activo` cuyo día es hoy (`penúltimo` = último día del mes − 1, `último` = último día del mes), agrega `- [ ] <Pendiente>` a la subsección de su Área, si no está ya. Si se arrastra sin hacer, se queda como cualquier pendiente.
- **Del dictado**: pendientes nuevos que el usuario mencione, como en `/dump` (uno por acción, verbo al inicio, enlaza personas/proyectos, `#clickup` si parece trabajo real). Si dice que ya hizo uno, márcalo `[x]`.
- **ClickUp**: lee la vista de tareas de Trabajo con `clickup_filter_tasks` y `view_id: "<VIEW_ID>"` (<URL de tu vista de ClickUp>). No uses otras listas. "Abiertas" = status `new`. Van al final de `### Trabajo` como `- [ ] [nombre](url) · prioridad`:
  - Primero las 3–5 `new` más fáciles de cerrar hoy (asignadas al usuario o sin asignar), juzgando por nombre/tags (config, campos, textos, ajustes chicos > pantallas o automatizaciones completas), con `· 🟢 fácil: <por qué en pocas palabras>`.
  - Luego el resto de `new` asignadas al usuario.
  - Al final las de `need info`, con `· ⏸ need info`.
  - Si una tarea que ya está en la daily cambió en ClickUp a un status cerrado/hecho, márcala `[x]`.
  - Quita el prefijo `[Pipedrive config]` o escapa los corchetes (`\[Automatización\]`). No dupliques por URL. No crees ni modifiques tareas en ClickUp.

## 8. Lo que hice mal hoy / Lo que hice bien hoy
Reflexión del usuario respecto a sus objetivos del mes y de la semana. **Solo con lo que él dicte**: ordénalo en bullets cortos en primera persona, sin agregar juicios, conclusiones ni cosas que no dijo. Si no dictó nada, déjalas como están. Si dice algo en "bien hoy", marca el hábito "Escribir al menos 1 cosa en 'Lo que hice bien hoy'…" si existe.

## 9. Gastos del dictado
Si el usuario menciona gastos ("250 de gasolina"), regístralos siguiendo `.claude/commands/gasto.md` y, si existe, marca el hábito de anotar gastos.

**Nota del mes**: si no existe `Proyectos/personal/Finanzas/Gastos/Gastos <YYYY-MM>.md`, créala como dice el paso 3 de `.claude/commands/gasto.md` (con el presupuesto copiado del mes anterior). Igual con `Proyectos/personal/Finanzas/Ingresos/Ingresos <YYYY-MM>.md` desde `Templates/Ingresos.md`.

**Ingresos fijos**: igual con `## Lista` de `Proyectos/personal/Finanzas/Ingresos fijos.md`: por cada uno `activo` cuyo día ya llegó este mes (`último` = último día del mes), si en `Ingresos/Ingresos <YYYY-MM>.md` no hay fila de este mes con esa categoría y ese monto, agrégala como `| <YYYY-MM-DD del día> | <Fuente> | <Monto> | <Categoría> | <Cuenta> |` (crea la nota desde `Templates/Ingresos.md` si no existe). Si el usuario dice que un ingreso fijo llegó otro día o con otro monto, corrige esa fila en vez de duplicarla. Menciónalo en la respuesta.

**Suscripciones**: lee `## Lista` de `Proyectos/personal/Finanzas/Suscripciones.md`. Por cada suscripción `activa` cuyo día de cobro ya llegó este mes (si el mes tiene menos días, el último día), si en `Gastos <YYYY-MM>.md` no hay una fila de este mes cuyo concepto contenga el nombre del servicio, agrégala: `| <YYYY-MM-DD del día de cobro> | Suscripción <Servicio> | <Monto> | <Categoría> | <Pago> |` (si el Área no es `[[personal]]`, agrega el link al concepto: `Suscripción Render ([[negocio]])`), en orden por fecha. No la dupliques. Menciónalo en la respuesta.

## 10. Qué falta
Si el dictado vino vacío y después de la tarde (≥ 18:00) faltan `peso`, `energia`, `ejercicio`, `lectura`, Log, Personas o la reflexión, pídeselo al usuario en **una sola pregunta** al final de la respuesta (ej. "¿Peso, energía 1–5, gym/lectura sí o no, y qué hiciste bien/mal hoy?"). En la mañana no preguntes por la reflexión. Con su respuesta, vuelve a los pasos 1, 4–9.

## Respuesta
5 líneas máximo: lo más importante del día y choques de agenda, cuántas tareas abiertas hay y cuál es la más fácil, hábitos hechos/total de hoy, la tarjeta recomendada para hoy, y qué falta por llenar (o la pregunta del paso 10). Si `Planeo gastar` está vacío, termina con la pregunta del paso 3b (junto con la del paso 10 si toca, en una sola pregunta).
