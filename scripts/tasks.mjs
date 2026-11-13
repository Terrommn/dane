// Genera docs/TASKS.md desde docs/tasks.json (espejo de la lista TaskCreate/TaskUpdate).
// Uso: node scripts/tasks.mjs [set ID estado "evidencia"]
import fs from "node:fs";
const path = new URL("../docs/tasks.json", import.meta.url);
const data = JSON.parse(fs.readFileSync(path, "utf8"));
const [cmd, id, status, ...ev] = process.argv.slice(2);
if (cmd === "set") {
  const t = data.tasks.find((x) => x.id === id);
  if (!t) throw new Error(`No existe ${id}`);
  t.status = status;
  if (ev.length) t.evidence = ev.join(" ");
  fs.writeFileSync(path, JSON.stringify(data, null, 2) + "\n");
}
const icon = { completed: "✅", in_progress: "🔄", pending: "⏳", blocked_external: "⛔" };
const count = (s) => data.tasks.filter((t) => t.status === s).length;
let md = `# Tareas\n\nEspejo persistente de la lista de tareas del entorno (TaskCreate/TaskUpdate, equivalente a TodoWrite; ver D-002).\nGenerado por \`node scripts/tasks.mjs\` desde \`docs/tasks.json\`. No editar a mano.\n\n`;
md += `Estados: ✅ completed · 🔄 in_progress · ⏳ pending · ⛔ blocked_external (en la herramienta queda \`pending\`).\n\n`;
md += `Resumen: ${count("completed")} completadas · ${count("in_progress")} en curso · ${count("pending")} pendientes · ${count("blocked_external")} bloqueadas externamente · ${data.tasks.length} total.\n\n`;
md += `| ID | Tarea | Responsable | Depende de | Estado | Criterio de aceptación | Evidencia |\n|---|---|---|---|---|---|---|\n`;
for (const t of data.tasks) {
  const esc = (s) => String(s ?? "").replace(/\|/g, "\\|").replace(/\n/g, " ");
  md += `| ${t.id} | ${esc(t.title)} | ${t.owner} | ${t.deps.join(", ") || "—"} | ${icon[t.status]} ${t.status} | ${esc(t.acceptance)} | ${esc(t.evidence) || "—"} |\n`;
  for (const s of t.subtasks ?? []) md += `| ${s.id} | ↳ ${esc(s.title)} | ${s.owner ?? t.owner} | ${t.id} | ${icon[s.status]} ${s.status} | ${esc(s.acceptance)} | ${esc(s.evidence) || "—"} |\n`;
}
if (data.external?.length) {
  md += `\n## Verificaciones externas pendientes\n\n| ID | Qué falta | Cómo retomarlo |\n|---|---|---|\n`;
  for (const e of data.external) md += `| ${e.id} | ${e.what} | ${e.how} |\n`;
}
fs.writeFileSync(new URL("../docs/TASKS.md", import.meta.url), md);
console.log("TASKS.md actualizado");
