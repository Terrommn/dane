---
description: Revisión semanal (dailies + Calendar + ClickUp + contenido) → Weekly/
---
Semana ISO actual en America/Monterrey (o la que indique: $ARGUMENTS). Nota: `Weekly/GGGG-WWW.md` (ej. `2026-W45.md`) desde `Templates/Weekly.md`.

1. Lee todas las `Daily/` de esa semana (incluye `peso`), la nota de gastos del mes en `Proyectos/personal/Finanzas/`, `Inbox.md`, las piezas de `Contenido/` modificadas en la semana y `Dev/devlog/` de la semana.
2. Consulta Google Calendar (eventos de la semana y de la próxima) y ClickUp, solo la vista de tareas de Trabajo (`view_id` `<VIEW_ID>`): tareas cerradas esta semana y las que siguen en `new` / `need info` asignadas al usuario.
3. Llena cada sección del template con bullets concretos (en "Proyectos (por área)" agrupa por Personal, Trabajo, Negocio, Escuela usando los pendientes de las dailies y `Dev/devlog/`) y enlaces `[[ ]]` a las notas fuente. En "Personas" enlaza las notas de `Personas/` mencionadas; si alguien relevante no tiene nota, sugiérelo (no la crees).
4. Hábitos (sección "Hábitos y devocional"): cuenta días con `devocional`, `ejercicio`, `lectura` en true (meta de ejercicio: 4 por semana; de devocional: diario, máx. 5 fallas al mes). Pregúntale al usuario su nivel de paz de la semana (1–5) y guárdalo en la propiedad `paz` de la weekly (el Home la usa). Lee también las notas de `Proyectos/personal/Devocionales/` de la semana y resume en una línea qué acciones ("¿Cómo puedo agradar a Dios hoy?") se repitieron o qué aprendió.
5. Objetivos: lee los objetivos de la semana (`Proyectos/personal/Objetivos/Semana <GGGG-[W]WW>.md`) y su tabla `## Seguimiento`, y las secciones "Lo que hice mal hoy" / "Lo que hice bien hoy" de las dailies. En la weekly resume: por objetivo, si se logró y % de sus hábitos cumplidos; patrones de lo que salió mal y lo que salió bien. Llena el `## Cierre` del objetivo semanal solo si el usuario lo confirma. No crees el objetivo de la próxima semana: al final sugiere al usuario correr `/objetivos próxima` (que también hace el del mes si toca).
6. Inbox: propón a dónde va cada item (nota existente, nueva nota, ClickUp o borrar). **No muevas nada hasta que el usuario confirme.**
7. Propón 3 prioridades para la próxima semana, justificadas en una línea cada una.
Respóndeme solo con el resumen de 5 líneas y el enlace a la nota.
