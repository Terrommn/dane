---
description: Vacía tus pendientes del día (texto libre) como checkboxes en la daily de hoy
argument-hint: <lista desordenada de pendientes>
---
Pendientes: $ARGUMENTS

Fecha de hoy en America/Monterrey (usa `date +%F` con TZ=America/Monterrey). Nota: `Daily/<fecha>.md`.

1. Si no hay texto arriba, pídeme el dump en una línea y espera.
2. Si la daily no existe, créala desde `Templates/Daily.md` sustituyendo la sintaxis de plantilla por valores reales.
3. Separa el texto en pendientes individuales: uno por acción, verbo al inicio, corto, en español. Conserva nombres, horas y fechas que yo haya dicho. Si menciono a alguien con nota en `Personas/` (también por apodo: "Juancho" → `[[Juan Pérez|Juancho]]`) o un proyecto de `Proyectos/` (`personal`, `trabajo`, `negocio`, `escuela`), enlázalo con `[[ ]]`.
4. Clasifica cada uno por área y añádelo como `- [ ] ` al final de su subsección dentro de `## Pendientes`: `### Personal`, `### Trabajo`, `### Negocio` o `### Escuela` (Usa `area` de la nota de la persona mencionada como pista; Escuela = escuela/clases; vida personal y lo que no encaje en ningún área → Personal). Si una subsección no existe, créala en ese orden. Quita la línea vacía `- [ ] ` del template en las subsecciones donde agregues algo. No borres ni reordenes lo que ya estaba y no dupliques uno que ya exista (marcado o no).
5. No crees tareas en ClickUp. Si un pendiente parece trabajo real (varios pasos, fecha límite, de Trabajo/Negocio/Escuela), márcalo al final de su línea con `#clickup` para moverlo después.
6. Respóndeme en 2 líneas: cuántos agregué y cuáles marqué `#clickup`.
