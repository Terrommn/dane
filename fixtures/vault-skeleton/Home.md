---
categories:
  - "[[Home]]"
meta_peso: 70
meta_ejercicio_semana: 4
max_fallas_devocional: 5
---
```dataviewjs
await dv.view("Scripts/home")
```

## Vistas
[[Personas.base|Personas]] · [[Hábitos.base|Hábitos]] · [[Objetivos.base|Objetivos]] · [[Devocionales.base|Devocionales]] · [[Pipeline.base|Contenido]] · [[Lecturas.base|Lecturas]] · [[Proyectos.base|Proyectos y Dev]]

## Proyectos (áreas)
[[personal]] · [[trabajo]] · [[negocio]] · [[escuela]]

## Captura rápida
→ [[Inbox]] (se vacía en la revisión semanal)

## Reglas
1. Escribe primero, organiza después. Buscar (`Cmd+O` / Omnisearch `Cmd+Shift+O`) > archivar.
2. Una nota = una cosa. Enlaza con `[[ ]]`.
3. Pendientes del día → `## Pendientes` de la daily, por área. Tareas de equipo → ClickUp (la daily solo las enlaza).
4. Videos y archivos pesados → Drive, nunca al vault. En la nota solo la ruta.
5. Nada de contraseñas, tokens ni `.env`.

## Comandos
Claude Code: `claude` en esta carpeta · Codex: pide el nombre del comando (instrucciones en `AGENTS.md`).
- `/daily` – arma la nota de hoy: devocional, hábitos, agenda (Calendar) y pendientes de ClickUp en Trabajo
- `/dump <pendientes>` – vacía tus pendientes del día como checkboxes en la daily, por área
- `/gasto <texto>` – registra gastos en la tabla del mes (ej. "ayer 250 gasolina")
- `/objetivos [próxima | mes]` – al inicio de la semana: cierra la anterior y define los objetivos de la semana (y del mes si toca) con sus hábitos diarios
- `/weekly-review` – revisión del domingo
- `/log-session` – resumen de una sesión de código (desde un repo con `/add-dir ~/Obsidian`)
