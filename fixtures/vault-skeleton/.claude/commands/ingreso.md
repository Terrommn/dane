---
description: Registra uno o varios ingresos (texto libre) en la tabla de ingresos del mes
argument-hint: <ej. "hoy me pagaron 15000 de Trabajo a mi banco, el lunes 8000 de un cliente de Negocio">
---
Ingresos: $ARGUMENTS

Fecha de hoy en America/Monterrey (`TZ=America/Monterrey date +%F`). Nota del mes: `Proyectos/personal/Finanzas/Ingresos/Ingresos <YYYY-MM>.md` (el mes de la **fecha del ingreso**, no necesariamente el de hoy).

1. Si no hay texto arriba, pídeme los ingresos en una línea y espera.
2. Separa el texto en ingresos individuales. Para cada uno saca:
   - **Fecha** `YYYY-MM-DD`: el día en que me entró el dinero. Resuelve "hoy", "ayer", "el viernes", "el 15" contra la fecha de hoy. Si no digo, pregunta (no supongas que fue hoy salvo que lo diga).
   - **Concepto**: corto ("Sueldo quincena 1", "Pago de [[Juan Pérez|Juancho]] por chatbot"). Enlaza personas de `Personas/` y proyectos (`[[negocio]]`).
   - **Monto**: en pesos, formato `$15,000`. Si viene en dólares, conviértelo y pon los USD en el concepto.
   - **Categoría**, exactamente una de: `Sueldo`, `Negocio`, `Freelance`, `Préstamo`, `Otro`. Si es ambiguo, pregunta.
   - **Cuenta**: a dónde entró (banco, tarjeta o `Efectivo`) si lo digo; si no, vacío.
3. Si la nota del mes no existe, créala desde `Templates/Ingresos.md` sustituyendo la sintaxis de Templater por el mes.
4. Agrega cada ingreso como fila de `## Registro` (`| Fecha | Concepto | Monto | Categoría | Cuenta |`), ordenada por fecha. No borres ni cambies filas existentes; si una fila idéntica ya existe, no la dupliques y avísame.
5. **Préstamo**: si alguien me presta dinero, regístralo como `Préstamo` con la persona en el concepto y recuérdame que es deuda (no se agrega sola al objetivo de deudas).
6. **Ingresos fijos**: si digo que un ingreso es fijo o mensual (o que cambió o se acabó), agrégalo o actualízalo en `## Lista` de `Proyectos/personal/Finanzas/Ingresos fijos.md` (`| Fuente | Monto | Día | Categoría | Cuenta | Estado |`, día = número o `último`, estado `activo` o `terminado`; nunca borres filas). Si el día de este mes ya pasó, regístralo también en la tabla del mes. Si me llega uno que /daily ya registró, no lo dupliques: corrige fecha o monto en esa fila.
7. Respóndeme en 2 líneas: qué registré (fecha, monto y categoría) y el total de ingresos del mes vs gastos del mes.
