---
description: Inicio de semana: define los objetivos de la semana (y del mes si toca) con sus hábitos diarios
argument-hint: [semana | mes | próxima]
---
Argumentos: $ARGUMENTS

Fecha de hoy en America/Monterrey (usa `TZ=America/Monterrey date +%F`, semana ISO con `+%G-W%V`, mes con `+%Y-%m`). Notas en `Proyectos/personal/Objetivos/`:
- Semana: `Semana <GGGG-[W]WW>.md` desde `Templates/Objetivo semana.md`
- Mes: `Mes <YYYY-MM>.md` desde `Templates/Objetivo mes.md`

Al crear una nota desde template, sustituye toda la sintaxis Templater por valores reales (`periodo`, `inicio` = lunes / día 1, `fin` = domingo / último día del mes, `mes` = link al objetivo del mes de esa semana) y quita `tp.file.cursor()`.

1. **Qué toca.** Por defecto, la semana actual. Si `$ARGUMENTS` dice `próxima` (o hoy es sábado o domingo), usa la semana siguiente. Incluye también el **mes** si: `$ARGUMENTS` dice `mes`, el objetivo de ese mes no existe o no tiene ningún objetivo con título y hábitos, o la semana empieza en un mes nuevo. Si `$ARGUMENTS` dice solo `mes`, haz solo el mes.
2. **Contexto antes de preguntar** (léelo, no lo pegues entero):
   - Objetivos de la semana anterior (bloques `### N.`), tabla `## Seguimiento` (% de hábitos cumplidos y cuáles fallaron más) y `## Cierre`.
   - "Lo que hice mal hoy" / "Lo que hice bien hoy" de las dailies de la semana anterior.
   - Objetivos del mes vigente (para que la semana empuje hacia ellos).
   - La weekly más reciente en `Weekly/`, si existe.
   Dale al usuario un resumen de 3 líneas: cómo le fue, el patrón principal de lo que salió mal y lo que salió bien.
3. **Cierra lo anterior.** Si la semana anterior (o el mes anterior) sigue en `estado: activo`, pregúntale por **cada** objetivo: cambia su `**Estado:**` a `logrado` / `no logrado`, y el `estado` del frontmatter a `logrado` si logró todos, si no `no logrado`. Llena `## Cierre` con sus respuestas si está vacío.
4. **Mes (si toca), primero que la semana.** el usuario puede tener **varios objetivos del mes** (normalmente 1 a 4, p. ej. uno por área). Pregunta en un solo mensaje:
   - ¿Cuáles son tus objetivos del mes? (lista; cada uno concreto)
   - Para cada uno: ¿por qué importa? y ¿cómo sabrás que lo lograste? (algo medible)
   Si responde solo la lista, propón tú el porqué y la métrica de cada uno y pide un sí o no. Luego propón de 1 a 3 **hábitos por objetivo**: acciones que se hagan **todos los días**, que se puedan marcar como hecho o no hecho en una línea y que empujen directo a ese objetivo (ej. "Escribir 30 min en el proyecto X", no "Avanzar en X"). Cuida que el total de hábitos del mes sea realista (máx. ~6 al día) y avisa si se pasa. Ajusta con el usuario hasta que diga que sí.
5. **Semana.** También puede tener **varios objetivos**. Mismas preguntas que el mes (lista de objetivos; por qué y cómo sabrá que lo logró) y para cada uno a qué objetivo del mes apoya. Propón de 1 a 3 **hábitos por objetivo** con las mismas reglas y el mismo tope realista (suma mes + semana ≤ ~8 al día; avisa si se pasa). Deben apoyar los objetivos del mes y atacar lo que salió mal la semana pasada. No repitas los hábitos del mes ni los hábitos fijos (devocional, ejercicio, lectura). Ajusta hasta que el usuario confirme.
6. **Escribe.**
   - Mes y semana: dentro de `## Objetivos`, un bloque por objetivo: `### N. <objetivo>` con `- **Área:** [[<proyecto>]]`, (solo semana) `- **Apoya a (mes):** <N. objetivo del mes>`, `- **Por qué importa:**`, `- **Cómo sé que lo logré:**`, `- **Estado:** activo` y `#### Hábitos` con `- [ ] <hábito>`. Reusa el bloque vacío del template para el primero.
   - `estado: activo` en ambos. No toques `## Seguimiento`.
7. **Daily de hoy.** Si existe `Daily/<hoy>.md` y la semana o el mes nuevos incluyen hoy, copia los hábitos a `## Hábitos del mes` (todos los de todos los objetivos, cada uno con ` · <objetivo corto>` al final) / `## Hábitos de la semana` (igual, con ` · <objetivo corto>`; sin marcar, debajo de la línea `Objetivo(s): [[…]]`, sin duplicar).
8. Respóndeme en 4 líneas como máximo: los objetivos del mes si cambiaron, los de la semana y los hábitos diarios que quedaron.

Reglas: no inventes objetivos ni hábitos sin confirmación del usuario; si responde corto, propón tú y pide un sí o no. No guardes secretos. No borres lo que el usuario ya escribió en las notas.
