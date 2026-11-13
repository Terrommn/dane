# Cómo usar este vault

Un "segundo cerebro" en Obsidian para la vida diaria, finanzas, objetivos, contenido y código. Está pensado para usarse **junto con un agente de IA** (Claude Code o Codex) que llena las notas por ti a partir de lo que le dictas.

Viene vacío: solo la estructura, las plantillas, las vistas y los comandos.

## 1. Instalación (5 min)
1. Descomprime la carpeta donde quieras (ej. `~/Obsidian`).
2. En Obsidian: **Open folder as vault** → elige la carpeta.
3. Cuando pregunte, **confía en el autor y activa los plugins de la comunidad**. Ya vienen incluidos: Templater, Dataview, Excalidraw, Omnisearch.
4. En Dataview → Settings, activa **Enable JavaScript Queries** (el Home y las finanzas usan DataviewJS).
5. Abre `Home.md`. Ponlo como nota de inicio si quieres.

## 2. Personalízalo (lo primero que debes cambiar)
- **Áreas**: vienen 4 de ejemplo: `personal`, `trabajo`, `negocio`, `escuela` (en `Proyectos/`). Renómbralas o agrega las tuyas. Si cambias un área, cámbiala también en:
  - `Templates/Daily.md` (subsecciones de `## Pendientes`)
  - `AGENTS.md` (tabla "Áreas y proyectos")
  - `.claude/commands/dump.md`, `daily.md`, `weekly-review.md`
- **`AGENTS.md`**: son las instrucciones del agente. Léelo completo y ajústalo a ti (zona horaria, metas, categorías de gastos). `CLAUDE.md` solo lo importa.
- **Metas del Home**: frontmatter de `Home.md` (`meta_peso`, `meta_ejercicio_semana`, `max_fallas_devocional`).
- **Categorías de gastos**: en `AGENTS.md`, `Templates/Gastos.md`, `.claude/commands/gasto.md` y los colores en `Scripts/gastos.js` / `Scripts/gastos-categorias.js`.
- **Devocional**: si no lo usas, quita la sección de `Templates/Daily.md` y la propiedad `devocional`, o cámbialo por otro hábito (ej. meditación).
- **ClickUp** (opcional): si usas ClickUp, pon tu vista en `AGENTS.md` y en `.claude/commands/daily.md` donde dice `<VIEW_ID>`. Si no, borra esos pasos.

## 3. Estructura
| Carpeta | Qué va |
|---|---|
| `Daily/` | Una nota por día (`YYYY-MM-DD.md`): agenda, hábitos, pendientes, log, reflexión |
| `Weekly/` | Revisión semanal |
| `Proyectos/<área>/` | Nota del área, runbooks, ADRs y lo propio de cada área |
| `Proyectos/personal/Objetivos/` | Objetivos del mes y de la semana con sus hábitos diarios |
| `Proyectos/personal/Finanzas/` | Gastos e ingresos por mes, tarjetas, cuentas, suscripciones, dashboard |
| `Proyectos/personal/Devocionales/` | Una nota por devocional |
| `Personas/` | Una nota por persona (contexto, último contacto, cumpleaños) |
| `Contenido/`, `Swipe/` | Piezas de video (guion, hooks, métricas) y referencias de otros creadores |
| `Lecturas/` | Libros y artículos |
| `Dev/` | Devlog de sesiones de código, TILs, snippets |
| `Bases/` | Vistas tipo base de datos (Personas, Hábitos, Objetivos, etc.) |
| `Templates/` | Plantillas de todo lo anterior |
| `Scripts/` | Código de las vistas DataviewJS. No los borres |
| `Inbox.md` | Captura rápida; se vacía en la revisión semanal |

Reglas: escribe primero, organiza después · una nota = una cosa · enlaza con `[[ ]]` · el estado es una propiedad, nunca una carpeta · **nada de contraseñas ni tokens en el vault**.

## 4. Uso con el agente (recomendado)
Abre una terminal en la carpeta del vault y corre `claude` (Claude Code). Comandos incluidos en `.claude/commands/`:

| Comando | Qué hace | Ejemplo |
|---|---|---|
| `/daily` | Crea o completa la daily: agenda (Google Calendar), hábitos, pendientes, log, personas, tarjeta recomendada. Córrelo en la mañana y en la noche | `/daily peso 72, energía 4, fui al gym, comí con Ana` |
| `/dump` | Convierte una lista desordenada en pendientes por área | `/dump llamar al dentista, terminar reporte, pagar luz` |
| `/gasto` | Registra gastos en la tabla del mes | `/gasto ayer 250 gasolina con débito, hoy 180 cena` |
| `/ingreso` | Registra ingresos | `/ingreso hoy me pagaron 15000 de sueldo` |
| `/objetivos` | Los lunes: cierra la semana anterior y define objetivos y hábitos diarios de la semana (y del mes si toca) | `/objetivos` o `/objetivos próxima` |
| `/weekly-review` | Revisión del domingo | `/weekly-review` |
| `/log-session` | Resume una sesión de código en el devlog | desde un repo, con `/add-dir ~/Obsidian` |

Con Codex u otro agente: pídele "daily", "gasto…", etc.; `AGENTS.md` le dice que lea `.claude/commands/<nombre>.md`.

Para `/daily` y `/weekly-review` conecta los MCP de **Google Calendar** y **Gmail** (y ClickUp si lo usas). Sin ellos funcionan, pero sin agenda ni correos.

## 5. Uso sin agente
Todo funciona a mano también:
- La daily se crea con el botón de **Daily note** (usa `Templates/Daily.md`).
- Las demás notas: `Templater: Create new note from template` y eliges la plantilla.
- Gastos: crea `Proyectos/personal/Finanzas/Gastos/Gastos YYYY-MM.md` con la plantilla `Gastos` y agrega filas a la tabla `## Registro`. El dashboard se calcula solo.

## 6. Rutina sugerida
- **Mañana**: `/daily` (planear) y responde qué planeas gastar.
- **Durante el día**: `/gasto`, `/dump`, captura en `Inbox.md`.
- **Noche**: `/daily <lo que pasó, qué hiciste bien y mal>` (cerrar).
- **Domingo**: `/weekly-review`. **Lunes**: `/objetivos`.
- **Primer día del mes**: `/objetivos mes`.

## 7. Finanzas en 1 minuto
1. Crea una nota por tarjeta en `Finanzas/Tarjetas/` (plantilla `Tarjeta`: límite, disponible, día de corte y de pago) y una por cuenta de débito en `Finanzas/Cuentas/` (plantilla `Cuenta`).
2. Llena `Suscripciones.md` e `Ingresos fijos.md`: `/daily` los registra solos el día que tocan.
3. Escribe tus reglas en `Plan de crédito.md`.
4. Pon tu presupuesto por categoría en `## Presupuesto` de la nota de gastos del mes; se copia solo al mes siguiente.
5. Mira todo en `Finanzas/Dashboard.md`.
