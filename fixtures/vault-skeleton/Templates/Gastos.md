<%*
// Copia el presupuesto del mes anterior (si existe); si no, deja las categorías vacías.
const ym = tp.file.title.replace("Gastos ", "");
const prev = moment(ym + "-01").subtract(1, "month").format("YYYY-MM");
const f = app.vault.getAbstractFileByPath(`Proyectos/personal/Finanzas/Gastos/Gastos ${prev}.md`);
let pres = `| Categoría | Presupuesto |
|---|---|
| Comida |  |
| Gasolina |  |
| Casa |  |
| Salud/gym |  |
| Deporte |  |
| Suscripciones |  |
| Personal/ocio |  |
| Salidas con amigos |  |
| Pareja |  |
| Trabajo |  |
| Inversión |  |
| Negocio |  |
| Nómina |  |
| Pago de deuda |  |
| Otro |  |`;
if (f) {
  const m = (await app.vault.read(f)).match(/^## Presupuesto[^\n]*\n(?:<!--.*-->\n)?([\s\S]*?)(?=\n## |(?![\s\S]))/m);
  if (m && m[1].trim()) pres = m[1].trim();
}
-%>
---
categories:
  - "[[Gastos]]"
mes: <% ym %>
---
# Gastos <% ym %>

```dataviewjs
await dv.view("Scripts/gastos")
```

## Gastos por categoría
```dataviewjs
await dv.view("Scripts/gastos-categorias")
```

## Presupuesto
<!-- Cuánto puedo gastar este mes por categoría. Se copia del mes pasado al crear la nota; edítalo cuando quieras. -->
<% pres %>

## Suscripciones
![[Suscripciones#Lista]]

## Registro
| Fecha | Concepto | Monto | Categoría | Pago |
|---|---|---|---|---|
