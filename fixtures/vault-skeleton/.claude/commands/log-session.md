---
description: Resume la sesión de código actual en el devlog del vault
---
Vault: `~/Obsidian`. Si no tienes acceso, pide `/add-dir ~/Obsidian` (Codex: abre la sesión con acceso a esa carpeta). Lee `~/Obsidian/AGENTS.md` antes de escribir.

1. Resume lo que se hizo en esta sesión: repo y rama, cambios principales, bugs encontrados y su causa, comandos o rarezas útiles, y qué queda pendiente. Usa `git log`/`git diff --stat` del repo actual si ayuda.
2. Añádelo (no sobrescribas) a `Dev/devlog/<YYYY-MM-DD>.md` (créalo desde `Templates/Devlog.md` si no existe) bajo un encabezado `### <repo> — <hora>`. Agrega el proyecto a `proyectos` como link (`"[[trabajo]]"`, `"[[negocio]]"`, …; ver áreas en `AGENTS.md`).
3. Si hubo una decisión de arquitectura, crea `Proyectos/<p>/adr/ADR-NNNN <título>.md` desde `Templates/ADR.md` (siguiente número libre) y enlázalo.
4. Si aprendiste una rareza de infraestructura reutilizable, añádela al runbook del proyecto.
5. **Nunca** copies secretos, tokens ni valores de `.env`.
Respóndeme con 2 líneas y los enlaces creados.
$ARGUMENTS
