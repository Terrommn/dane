# Vault: segundo cerebro (vida, contenido, código)

> Instrucciones para cualquier agente (Claude Code, Codex, etc.). Este archivo es la fuente única: `CLAUDE.md` solo lo importa. Edita aquí, no en `CLAUDE.md`.

Este es el vault de Obsidian del usuario. Escribe en español, directo y conciso.

## Reglas duras
- **Nunca** guardes secretos aquí (API keys, tokens, `.env`, connection strings, contraseñas). Refiérete a ellos por nombre ("token de Render del trabajo en el gestor de contraseñas").
- **Nunca** copies videos, audio pesado ni renders al vault. Viven fuera del vault (ej. Google Drive). En la nota solo guarda la ruta en `raw_path` / `render_path`.
- Las tareas de equipo viven en **ClickUp**; en la daily solo se enlazan (ver "Daily"). No crees ni modifiques tareas de ClickUp sin que el usuario lo pida.
- No muevas ni renombres notas existentes sin pedirlo; usa `[[wikilinks]]` (formato corto) y deja que Obsidian actualice enlaces.
- No edites `.obsidian/` salvo que te lo pidan. Plugins instalados: Templater, Dataview, Excalidraw, Omnisearch.
- Sintaxis de Obsidian: `[[wikilinks]]`, callouts, frontmatter YAML, archivos `.base`. Si tu agente tiene las skills `obsidian-markdown` / `obsidian-bases` (kepano/obsidian-skills), úsalas.
- Un `[` al inicio del texto de un link markdown se escapa (`[\[Tag\] nombre](url)`) para que Obsidian no lo lea como wikilink.

## Áreas y proyectos
Cada área de Pendientes tiene su proyecto en `Proyectos/<p>/<p>.md`. Lo que pertenece a un área vive dentro de su carpeta de proyecto (ej. devocionales en `Proyectos/personal/Devocionales/`, clases en `Proyectos/escuela/Clases/`). El contexto de cada uno va en la sección "Qué es" de esa nota; **léela antes de trabajar en un proyecto**. Si está vacía, pregunta al usuario en vez de suponer.

| Área | Proyecto | Qué es |
|---|---|---|
| Personal | [[personal]] | Vida personal: familia, amigos, salud, y todo lo que no encaje en otra área |
| Trabajo | [[trabajo]] | Tu trabajo/empresa. Tareas en ClickUp (opcional) |
| Negocio | [[negocio]] | Por documentar |
| Escuela | [[escuela]] | Escuela; notas de clase en `Proyectos/escuela/Clases/` |

## Daily (`Daily/YYYY-MM-DD.md`)
Secciones en orden: Devocional, Agenda, Gastos de hoy, Hábitos del mes, Hábitos de la semana, Log, Personas, Pendientes, Lo que hice mal hoy, Lo que hice bien hoy.
- **Devocional**: solo el link a la nota del devocional del día (`[[Proyectos/personal/Devocionales/Devocional YYYY-MM-DD|Devocional de hoy]]`). El devocional en sí vive en `Proyectos/personal/Devocionales/`. Que lo hizo o no se marca en la propiedad `devocional` de la daily (hábito): `true` si la nota del devocional existe y tiene algo escrito en Pasaje o en las demás secciones.
- **Hábitos del mes / de la semana**: checkboxes copiados de los hábitos de los objetivos del mes (todos) y de la semana (ver "Objetivos"). Son distintos de los hábitos fijos (`devocional`, `ejercicio`, `lectura` en las propiedades y `Bases/Hábitos.base`); nunca los agregues como propiedades.
- **Lo que hice mal hoy / Lo que hice bien hoy**: reflexión del usuario, varios bullets, respecto a sus objetivos del mes y de la semana. No lo llenes por él: solo se escribe lo que él dicte (`/daily <dictado>`).
- **Gastos de hoy**: `Planeo gastar` (lo que el usuario diga; `/daily` siempre se lo pregunta si está vacío) y `Tarjeta` (la mejor tarjeta para hoy según día de corte, disponible, 30% de uso y el Plan de crédito; si ninguna sirve, Débito <tu banco>).
- **Agenda**: solo eventos de Google Calendar con hora (`- 10:00–11:00 Título`). Nunca días festivos ni observancias (se ignora el calendario `en.mexican#holiday@group.v.calendar.google.com`). Nada de ClickUp aquí.
- **Log**: bitácora de lo que pasó en el día (dictado + pendientes cerrados + eventos pasados + devlog + total de gastos). **Personas**: con quién habló el usuario y de qué (dictado + invitados de Calendar + correos enviados); actualiza `ultimo_contacto` de la persona.
- `/daily` llena todas las secciones y se puede correr varias veces (mañana planear, noche cerrar). Las propiedades `peso`, `energia`, `ejercicio`, `lectura` solo se llenan con lo que el usuario dicte.
- **Pendientes**: los pendientes reales del día (checkboxes), divididos en `### Personal`, `### Trabajo`, `### Negocio`, `### Escuela`. No borres ni reordenes lo que el usuario escribió y no dupliques.
- **Pendientes recurrentes**: tabla `## Lista` en `Proyectos/personal/Pendientes recurrentes.md` (día del mes, pendiente, área); `/daily` los agrega solos a `## Pendientes` el día que tocan.
- **ClickUp en la daily**: solo la vista de tareas de Trabajo (`view_id` `<VIEW_ID>`, <URL de tu vista de ClickUp>). "Abiertas" = status `new`. Van al final de `### Trabajo` como `- [ ] [nombre](url) · prioridad`; las más fáciles con `· 🟢 fácil: <por qué>`, las de `need info` con `· ⏸ need info`. No dupliques por URL.

## Objetivos (`Proyectos/personal/Objetivos/`)
- `Mes YYYY-MM.md` (template `Templates/Objetivo mes.md`) y `Semana GGGG-[W]WW.md` (template `Templates/Objetivo semana.md`).
- **Mes**: puede tener **varios objetivos**. Cada uno es un bloque `### N. <objetivo>` dentro de `## Objetivos`, con Área, Por qué importa, Cómo sé que lo logré, `**Estado:**` y `#### Hábitos` (checkboxes diarios). **Semana**: igual, varios objetivos con el mismo formato y además `**Apoya a (mes):**`.
- Los hábitos son lo que el usuario debe hacer **todos los días** para lograr el objetivo. `daily` copia los de todos los objetivos activos a cada daily (cada uno con ` · <objetivo corto>`) y registra el resultado del día anterior en `## Seguimiento` (`| fecha | hechos/total | faltó |`).
- `objetivos` (cada lunes o el domingo con `próxima`) cierra el objetivo anterior y define con el usuario los de la semana, y los del mes cuando toca, con sus hábitos diarios. `weekly-review` resume la semana y al final sugiere correr `objetivos`.

## Home y gastos
- `Home.md`: métricas en vivo (semana vs semana pasada, mes vs mes pasado: hábitos fijos, % de hábitos del mes/semana, peso, gastos, deudas). Lo calcula `Scripts/home.js` con el plugin **Dataview** (DataviewJS activado). Las metas (`meta_peso`, `meta_ejercicio_semana`, `max_fallas_devocional`) están en el frontmatter de `Home.md`. No reescribas los números a mano.
- Gastos: `Proyectos/personal/Finanzas/Gastos/Gastos YYYY-MM.md`, una tabla `| Fecha | Concepto | Monto | Categoría | Pago |` por mes (template `Templates/Gastos.md`; resumen con `Scripts/gastos.js`). El usuario dicta los gastos y el agente los registra con el comando `gasto`. Suscripciones fijas en `Proyectos/personal/Finanzas/Suscripciones.md` (tabla `## Lista`, embebida en cada nota de gastos); `/daily` las registra solas en el mes el día de cobro. Cada nota del mes tiene su propio `## Presupuesto` por categoría (se copia del mes anterior al crearla y se puede editar durante el mes; así queda el registro mes a mes). **Presupuesto por área**: un área con gastos propios (hoy Negocio) tiene su desglose por rubro en `## Presupuesto` de su nota de proyecto (`Proyectos/negocio/negocio.md`: Servidores, WhatsApp, Ads); el presupuesto de esa categoría en la nota del mes es la suma del desglose. Sus gastos se ven en la nota del proyecto con `Scripts/gastos-proyecto.js` (solo lee las notas de gastos; nunca registres gastos ahí). Ingresos: `Proyectos/personal/Finanzas/Ingresos/Ingresos YYYY-MM.md`, tabla `| Fecha | Concepto | Monto | Categoría | Cuenta |` (template `Templates/Ingresos.md`; comando `ingreso`; categorías: Sueldo, Negocio, Freelance, Préstamo, Otro). Ingresos fijos (sueldo, pagos mensuales) en `Proyectos/personal/Finanzas/Ingresos fijos.md`; `/daily` los registra solos el día que entran. **Tarjetas de crédito**: una nota por tarjeta en `Proyectos/personal/Finanzas/Tarjetas/` (`Tarjeta <nombre>.md`, template `Templates/Tarjeta.md`; frontmatter `limite`, `disponible`, `actualizado`, `dia_corte`, `dia_pago`, `estado`); panel en `Tarjetas de crédito.md` (`Scripts/tarjetas.js`). **Cuentas de débito**: una nota por cuenta en `Proyectos/personal/Finanzas/Cuentas/` (`Cuenta <banco>.md`, template `Templates/Cuenta.md`; frontmatter `banco`, `saldo` = saldo al **iniciar** el día `actualizado`, `estado`); el panel `Scripts/cuentas.js` (en el Dashboard) calcula en vivo lo que queda = `saldo` + ingresos cuya Cuenta menciona el banco − gastos cuyo Pago lo menciona, desde `actualizado` (inclusive). Si el usuario dice cuánto tiene hoy en una cuenta, pon `actualizado: <hoy>` y `saldo` = lo que dijo + gastos de hoy con esa cuenta − ingresos de hoy a esa cuenta. **Regla general:** en toda tabla de finanzas (gastos, suscripciones), si algo se paga con tarjeta de crédito, la columna Pago enlaza su nota (`[[Tarjeta A\|A]]`), nunca texto suelto. Gráficas en `Proyectos/personal/Finanzas/Dashboard.md` (`Scripts/finanzas.js`). Categorías: Comida, Gasolina, Casa, Salud/gym, Deporte, Suscripciones, Personal/ocio, Salidas con amigos, Pareja, Trabajo, Inversión, Negocio, Nómina, Pago de deuda, Otro.
- Weekly: propiedad `paz` (1–5), la pide `weekly-review`.

## Estructura
Las carpetas que todavía no existen se crean la primera vez que se necesiten.

| Carpeta | Qué va | Plantilla |
|---|---|---|
| `Daily/` | `YYYY-MM-DD.md`: ver sección "Daily" | `Templates/Daily.md` |
| `Proyectos/personal/Devocionales/` | `Devocional YYYY-MM-DD.md`: 1 nota por devocional. Secciones: Pasaje, ¿Cómo puedo agradar a Dios hoy? (tabla Versículo \| Acción), ¿Qué aprendí?, ¿Qué enseñanza puedo compartir?, Oración, Canción | `Templates/Devocional.md` |
| `Proyectos/personal/Objetivos/` | `Mes YYYY-MM.md` (varios objetivos), `Semana GGGG-[W]WW.md` (varios objetivos) | `Templates/Objetivo mes.md`, `Templates/Objetivo semana.md` |
| `Proyectos/personal/Finanzas/` | `Gastos/Gastos YYYY-MM.md` e `Ingresos/Ingresos YYYY-MM.md` (1 por mes), `Suscripciones.md`, `Dashboard.md` | `Templates/Gastos.md` |
| `Scripts/` | `.js` de vistas DataviewJS (`home.js`, `gastos.js`). No los borres | — |
| `Weekly/` | `GGGG-[W]WW.md` (ej. `2026-W45.md`): revisión semanal | `Templates/Weekly.md` |
| `Contenido/` | 1 nota = 1 pieza de video (reel/TikTok/ad). Guion + shot list + notas de edición | `Templates/Pieza.md` |
| `Swipe/` | referencias de otros creadores (URL, por qué funciona, hook transcrito) | — |
| `Proyectos/<p>/` | `personal`, `trabajo`, `negocio`, `escuela` (uno por área de Pendientes): overview `<p>.md`, `adr/ADR-NNNN <título>.md`, runbooks, y subcarpetas propias del área | `Templates/ADR.md` |
| `Dev/devlog/` | `YYYY-MM-DD.md` resumen de sesiones de código del día | `Templates/Devlog.md` |
| `Dev/til/`, `Dev/snippets/` | aprendizajes cortos y snippets, tags `#lang/x`, `#tool/x` | — |
| `Personas/` | 1 nota por persona (`Nombre Apellido.md`). Si el usuario menciona a alguien por apodo (ej. "Juancho", "Lalo"), búscalo aquí y enlaza `[[Nombre Apellido\|apodo]]` | `Templates/Persona.md` |
| `Proyectos/escuela/Clases/` | `<materia>/YYYY-MM-DD <tema>.md` | `Templates/Clase.md` |
| `Lecturas/` | libros/artículos | `Templates/Lectura.md` |
| `Bases/` | vistas `.base` (Pipeline, Personas, Hábitos, Objetivos, Devocionales, Lecturas, Proyectos) | — |
| `Inbox.md` | captura rápida; se vacía en la revisión semanal | — |

`Daily.md` y `Devocional.md` usan sintaxis de Templates core (`{{date:YYYY-MM-DD}}`, porque la daily la crea el plugin Daily notes); las demás usan Templater (`<% ... %>`). **Cuando crees una nota desde plantilla tú mismo, sustituye esa sintaxis por valores reales** (fecha, título) y quita `tp.file.cursor()`.

## Propiedades (frontmatter)
- `categories`: lista de links (`"[[Contenido]]"`, `"[[Personas]]"`, …). Toda nota la lleva.
- Fechas en `YYYY-MM-DD`. Zona horaria: America/Monterrey.
- Contenido: `estado` ∈ `idea | guion | grabado | editado | publicado`; `formato`, `plataforma` (lista), `hook`, `pilar`, `fecha_pub`, `raw_path`, `render_path`, `views`, `retencion` (%).
- Daily: `devocional`, `ejercicio`, `lectura` (checkbox, hábitos fijos), `energia` (1–5), `peso` (kg, número; el usuario se pesa en la mañana). Metas: `devocional` **todos los días** (máx. 5 fallas al mes), `ejercicio` **4 veces por semana** (se marca solo el día que se hace; no es diario).
- Objetivos: `tipo` ∈ `mes | semana`, `periodo` (`YYYY-MM` o `GGGG-[W]WW`), `inicio`, `fin`, `estado` ∈ `activo | logrado | no logrado`; la semana lleva `mes` (link a su objetivo del mes).
- Devocional: `fecha`, `pasaje` (referencia corta, ej. `Juan 15:1-8`).
- ADR: `proyecto` (link: `"[[trabajo]]"`), `estado` ∈ `propuesta | aceptada | reemplazada`, `fecha`. Devlog: `proyectos` (lista de links a proyectos).
- Personas: `area` (link al proyecto: `"[[trabajo]]"`, `"[[personal]]"`, …), `relacion` ∈ `jefe | subordinado | compañero | cliente | familia | pareja | amigo | otro`, `org`, `rol`, `ultimo_contacto`, `cumpleaños` (opcional, `YYYY-MM-DD`, o `MM-DD` si no se sabe el año; no lo borres si ya está), `contacto`. Quién es la persona para el usuario va en `## Contexto` de su nota.
- **El estado es una propiedad, nunca una carpeta.**

## Flujo de contenido (reels / motion)
idea → guion (10 hooks, elegir 1) → grabado (`raw_path`) → editado (al terminar poner `estado: editado` y `render_path`) → publicado (`fecha_pub`, luego `views`, `retencion`, y llenar "Resultado").
Antes de escribir guiones lee las 5 piezas publicadas más recientes de `Contenido/` (`views`, `retencion`, "Resultado").

## Herramientas disponibles
- MCP: Google Calendar, Gmail, ClickUp, Google Drive. Úsalos en `daily` y `weekly-review` (solo lectura salvo que el usuario pida otra cosa).
- Comandos del vault en `.claude/commands/`: `daily`, `dump`, `gasto`, `ingreso`, `objetivos`, `weekly-review`, `log-session`. En Claude Code se llaman como `/daily`, etc. **En Codex u otro agente**: si el usuario pide "daily", "dump", etc., lee `.claude/commands/<nombre>.md` y sigue sus pasos (`$ARGUMENTS` = lo que el usuario escribió después del nombre). Las herramientas MCP pueden llamarse distinto en cada agente; usa la equivalente.
